use super::*;
use rroflow_core::capture::CaptureEvent;

pub(super) struct Lease {
    pub ds: String,
    pub capture: String,
    pub token: String,
}
pub(super) async fn claim(pg: &PgStore, cfg: &ActivityConfig) -> anyhow::Result<Option<Lease>> {
    // Exhausted crashed leases must become visible errors instead of waiting forever.
    sqlx::query("UPDATE sdk_activity_work SET status='error',last_error='lease_attempts_exhausted',lease_token=NULL,lease_until=NULL WHERE status='running' AND lease_until<=clock_timestamp() AND attempts>=$1").bind(cfg.max_attempts).execute(&pg.pool).await?;
    let token = format!("{:032x}", rand::random::<u128>());
    let row=sqlx::query("WITH picked AS (SELECT devspace_id,capture_id FROM sdk_activity_work WHERE (status IN ('queued','error') OR (status='running' AND lease_until<=clock_timestamp())) AND attempts<$3 AND next_attempt_at<=clock_timestamp() ORDER BY next_attempt_at,changed_at,devspace_id,capture_id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE sdk_activity_work w SET status='running',lease_token=$1,lease_until=clock_timestamp()+make_interval(secs=>$2),attempts=attempts+1 FROM picked p WHERE w.devspace_id=p.devspace_id AND w.capture_id=p.capture_id RETURNING w.devspace_id,w.capture_id")
        .bind(&token).bind(cfg.lease_seconds as f64).bind(cfg.max_attempts).fetch_optional(&pg.pool).await?;
    Ok(row.map(|r| Lease {
        ds: r.get("devspace_id"),
        capture: r.get("capture_id"),
        token,
    }))
}
pub(super) async fn snapshot(
    pg: &PgStore,
    lease: &Lease,
    cfg: &ActivityConfig,
) -> anyhow::Result<(i64, Vec<Activity>)> {
    let mut tx = pg.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    let rev: i64 = sqlx::query_scalar(
        "SELECT source_revision FROM sdk_activity_work WHERE devspace_id=$1 AND capture_id=$2",
    )
    .bind(&lease.ds)
    .bind(&lease.capture)
    .fetch_one(&mut *tx)
    .await?;
    let bounds=sqlx::query("SELECT count(*) AS n,coalesce(sum(octet_length((record-'payload')::text)),0)::bigint AS bytes FROM capture_events WHERE devspace_id=$1 AND capture_id=$2").bind(&lease.ds).bind(&lease.capture).fetch_one(&mut *tx).await?;
    anyhow::ensure!(
        bounds.get::<i64, _>("n") <= cfg.max_events_per_capture as i64,
        "event_capacity"
    );
    anyhow::ensure!(
        bounds.get::<i64, _>("bytes") <= cfg.max_snapshot_bytes as i64,
        "snapshot_capacity"
    );
    let rows:Vec<SqlJson<CaptureEvent>>=sqlx::query_scalar("SELECT record-'payload' FROM capture_events WHERE devspace_id=$1 AND capture_id=$2 ORDER BY sequence").bind(&lease.ds).bind(&lease.capture).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let activities = tokio::task::spawn_blocking(move || {
        rroflow_core::activity::project(&rows.into_iter().map(|r| r.0).collect::<Vec<_>>())
    })
    .await?
    .map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        serde_json::to_vec(&activities)?.len() <= cfg.max_snapshot_bytes,
        "snapshot_capacity"
    );
    Ok((rev, activities))
}
pub(super) async fn publish(
    pg: &PgStore,
    lease: &Lease,
    rev: i64,
    activities: &[Activity],
) -> anyhow::Result<bool> {
    let mut tx = pg.pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    // Follow ingest's devspace -> work lock order; don't deadlock with FK checks.
    sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR KEY SHARE")
        .bind(&lease.ds)
        .fetch_one(&mut *tx)
        .await?;
    let valid:bool=sqlx::query_scalar("SELECT status='running' AND lease_token=$3 AND lease_until>clock_timestamp() AND source_revision>=$4 FROM sdk_activity_work WHERE devspace_id=$1 AND capture_id=$2 FOR UPDATE").bind(&lease.ds).bind(&lease.capture).bind(&lease.token).bind(rev).fetch_one(&mut *tx).await?;
    if !valid {
        return Ok(false);
    }
    sqlx::query("DELETE FROM sdk_activities WHERE devspace_id=$1 AND capture_id=$2")
        .bind(&lease.ds)
        .bind(&lease.capture)
        .execute(&mut *tx)
        .await?;
    for a in activities {
        anyhow::ensure!(a.capture_id == lease.capture, "projection capture mismatch");
        let signals: Vec<_> = a
            .findings
            .iter()
            .map(|f| f.signal.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let search = format!(
            "{} {} {} {}",
            a.agent,
            a.session_id,
            a.models.join(" "),
            a.tools
                .iter()
                .filter_map(|t| t.name.clone())
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        sqlx::query("INSERT INTO sdk_activities(devspace_id,activity_id,capture_id,source_revision,revision,agent,session_id,outcome,first_observed_at,signals,search_text,record) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(&lease.ds).bind(&a.activity_id).bind(&a.capture_id).bind(rev).bind(revision(a)).bind(&a.agent).bind(&a.session_id).bind(serde_json::to_value(a.outcome)?.as_str()).bind(crate::pg::timestamp(&a.first_observed_at)).bind(signals).bind(search).bind(SqlJson(a)).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE sdk_activity_work SET published_revision=$4,status=CASE WHEN source_revision=$4 THEN 'ready' ELSE 'queued' END,attempts=0,lease_token=NULL,lease_until=NULL,last_error=NULL,next_attempt_at=clock_timestamp() WHERE devspace_id=$1 AND capture_id=$2 AND lease_token=$3")
        .bind(&lease.ds).bind(&lease.capture).bind(&lease.token).bind(rev).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
pub async fn run_once(pg: &PgStore, cfg: &ActivityConfig) -> anyhow::Result<Value> {
    cfg.validate()?;
    let Some(lease) = claim(pg, cfg).await? else {
        return Ok(json!({"worked":false}));
    };
    match snapshot(pg, &lease, cfg).await {
        Ok((rev, activities)) => {
            Ok(json!({"worked":true,"published":publish(pg,&lease,rev,&activities).await?}))
        }
        Err(e) => {
            let code = match e.to_string().as_str() {
                "event_capacity" => "event_capacity",
                "snapshot_capacity" => "snapshot_capacity",
                s if s.starts_with("conflicting")
                    || s.starts_with("inconsistent")
                    || s.starts_with("cyclic") =>
                {
                    "invalid_events"
                }
                _ => "source_unavailable",
            };
            sqlx::query("UPDATE sdk_activity_work SET status='error',last_error=$4,lease_token=NULL,lease_until=NULL,next_attempt_at=clock_timestamp()+make_interval(secs=>$5) WHERE devspace_id=$1 AND capture_id=$2 AND lease_token=$3")
                .bind(&lease.ds).bind(&lease.capture).bind(&lease.token).bind(code).bind(cfg.retry_seconds as f64).execute(&pg.pool).await?;
            Ok(json!({"worked":true,"published":false,"error":code}))
        }
    }
}
