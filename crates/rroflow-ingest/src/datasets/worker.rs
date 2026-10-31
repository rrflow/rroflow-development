use super::*;
use sqlx::Row;

pub(super) async fn schedule(pg: &PgStore, cfg: &DatasetConfig) -> anyhow::Result<bool> {
    let mut tx = pg.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    let row=sqlx::query("SELECT devspace_id,purpose,revision,document,last_input_digest FROM dataset_policies WHERE enabled AND next_scan_at<=clock_timestamp() ORDER BY next_scan_at,devspace_id,purpose FOR UPDATE SKIP LOCKED LIMIT 1")
        .fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let ds: String = row.get("devspace_id");
    let purpose: String = row.get("purpose");
    let revision: i64 = row.get("revision");
    let policy: SqlJson<Policy> = row.get("document");
    let previous: Option<String> = row.get("last_input_digest");
    policy.0.validate().map_err(anyhow::Error::msg)?;
    sqlx::query("UPDATE dataset_policies SET last_scan_at=clock_timestamp(),next_scan_at=clock_timestamp()+make_interval(secs=>$3) WHERE devspace_id=$1 AND purpose=$2")
        .bind(&ds).bind(&purpose).bind(cfg.refresh_seconds as f64).execute(&mut *tx).await?;
    let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM dataset_jobs WHERE devspace_id=$1 AND purpose=$2 AND status IN ('queued','running'))")
        .bind(&ds).bind(&purpose).fetch_one(&mut *tx).await?;
    if pending {
        tx.commit().await?;
        return Ok(false);
    }
    // SAVEPOINT lets a bounded projection failure be recorded without aborting policy progress.
    sqlx::query("SAVEPOINT projection")
        .execute(&mut *tx)
        .await?;
    let input = match projection::snapshot(&mut tx, &ds, &policy.0, cfg).await {
        Ok(input) => input,
        Err(error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT projection")
                .execute(&mut *tx)
                .await?;
            let code = match error.to_string().as_str() {
                "row_capacity" => "row_capacity",
                "snapshot_capacity" => "snapshot_capacity",
                _ => "source_unavailable",
            };
            sqlx::query(
                "UPDATE dataset_policies SET last_error=$3 WHERE devspace_id=$1 AND purpose=$2",
            )
            .bind(&ds)
            .bind(&purpose)
            .bind(code)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(false);
        }
    };
    let input_digest = digest(&input);
    sqlx::query("UPDATE dataset_policies SET last_source_count=$3,last_error=NULL,last_input_digest=$4,last_exclusions=$5 WHERE devspace_id=$1 AND purpose=$2")
        .bind(&ds).bind(&purpose).bind(input.rows.len() as i64).bind(&input_digest).bind(SqlJson(&input.exclusions)).execute(&mut *tx).await?;
    if previous.as_deref() == Some(&input_digest) || input.rows.is_empty() {
        tx.commit().await?;
        return Ok(false);
    }
    let id = digest(&(&ds, &purpose, revision, &input_digest));
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dataset_jobs WHERE devspace_id=$1 AND id=$2)",
    )
    .bind(&ds)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        // A source can return to an earlier snapshot. Do not evict a current
        // artifact for an already-known job, or resurrect an expired version.
        tx.commit().await?;
        return Ok(false);
    }
    // Retain only the configured number of artifacts within their consent's lifetime.
    sqlx::query("UPDATE dataset_jobs SET status='expired',input=NULL,artifact=NULL WHERE devspace_id=$1 AND purpose=$2 AND status='published' AND id IN (SELECT id FROM dataset_jobs WHERE devspace_id=$1 AND purpose=$2 AND status='published' ORDER BY created_at DESC,id OFFSET $3)")
        .bind(&ds).bind(&purpose).bind(cfg.max_retained_per_purpose-1).execute(&mut *tx).await?;
    let inserted=sqlx::query("INSERT INTO dataset_jobs(devspace_id,id,purpose,policy_revision,status,input_digest,input,row_count,exclusions,expires_at) VALUES($1,$2,$3,$4,'queued',$5,$6,$7,$8,clock_timestamp()+make_interval(days=>$9)) ON CONFLICT DO NOTHING")
        .bind(&ds).bind(id).bind(&purpose).bind(revision).bind(input_digest).bind(SqlJson(&input)).bind(input.rows.len() as i64).bind(SqlJson(&input.exclusions)).bind(policy.0.retention_days as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(inserted.rows_affected() == 1)
}

pub(super) struct Lease {
    pub ds: String,
    pub id: String,
    pub token: String,
    pub purpose: String,
    pub revision: i64,
    pub input: projection::Input,
    pub input_digest: String,
    pub created: String,
    pub expires: String,
}
pub(super) async fn claim(pg: &PgStore, cfg: &DatasetConfig) -> anyhow::Result<Option<Lease>> {
    let token = format!("{:032x}", rand::random::<u128>());
    let row=sqlx::query("WITH picked AS (SELECT devspace_id,id FROM dataset_jobs WHERE (status='queued' OR (status='running' AND lease_until<=clock_timestamp())) AND expires_at>clock_timestamp() AND attempts<$3 ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE dataset_jobs j SET status='running',lease_token=$1,lease_until=clock_timestamp()+make_interval(secs=>$2),attempts=attempts+1 FROM picked p WHERE j.devspace_id=p.devspace_id AND j.id=p.id RETURNING j.*")
        .bind(&token).bind(cfg.lease_seconds as f64).bind(cfg.max_attempts).fetch_optional(&pg.pool).await?;
    row.map(|r| {
        Ok(Lease {
            ds: r.try_get("devspace_id")?,
            id: r.try_get("id")?,
            token,
            purpose: r.try_get("purpose")?,
            revision: r.try_get("policy_revision")?,
            input: r.try_get::<SqlJson<projection::Input>, _>("input")?.0,
            input_digest: r.try_get("input_digest")?,
            created: r
                .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?
                .to_rfc3339(),
            expires: r
                .try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at")?
                .to_rfc3339(),
        })
    })
    .transpose()
}
pub(super) async fn publish(pg: &PgStore, lease: &Lease, artifact: Value) -> anyhow::Result<bool> {
    let mut tx = pg.pool.begin().await?;
    let allowed:Option<bool>=sqlx::query_scalar("SELECT enabled AND revision=$3 FROM dataset_policies WHERE devspace_id=$1 AND purpose=$2 FOR SHARE")
        .bind(&lease.ds).bind(&lease.purpose).bind(lease.revision).fetch_optional(&mut *tx).await?;
    if allowed != Some(true) {
        return Ok(false);
    }
    let hash = digest(&artifact);
    let result=sqlx::query("UPDATE dataset_jobs SET status='published',artifact=$4,artifact_digest=$5,published_at=clock_timestamp(),input=NULL,lease_token=NULL,lease_until=NULL WHERE devspace_id=$1 AND id=$2 AND status='running' AND lease_token=$3 AND lease_until>clock_timestamp() AND expires_at>clock_timestamp()")
        .bind(&lease.ds).bind(&lease.id).bind(&lease.token).bind(SqlJson(artifact)).bind(hash).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub async fn run_once(pg: &PgStore, cfg: &DatasetConfig) -> anyhow::Result<Value> {
    cfg.validate()?;
    sqlx::query("DELETE FROM dataset_jobs WHERE status IN ('failed','expired','revoked') AND created_at<clock_timestamp()-make_interval(days=>$1)")
        .bind(cfg.job_history_days).execute(&pg.pool).await?;
    sqlx::query("UPDATE dataset_jobs SET status='expired',input=NULL,artifact=NULL,lease_token=NULL,lease_until=NULL WHERE expires_at<=clock_timestamp() AND status NOT IN ('expired','revoked')")
        .execute(&pg.pool).await?;
    sqlx::query("UPDATE dataset_jobs SET status='failed',error_code='attempts_exhausted',input=NULL,lease_token=NULL,lease_until=NULL WHERE status='running' AND lease_until<=clock_timestamp() AND attempts >= $1")
        .bind(cfg.max_attempts).execute(&pg.pool).await?;
    let scheduled = schedule(pg, cfg).await?;
    let Some(lease) = claim(pg, cfg).await? else {
        return Ok(json!({"scheduled":scheduled,"published":false}));
    };
    let result = projection::build(
        &lease.input,
        &lease.ds,
        &lease.id,
        lease.revision,
        &lease.input_digest,
        &lease.created,
        &lease.expires,
    );
    let published = match result {
        Ok(artifact) => publish(pg, &lease, artifact).await?,
        Err(_) => {
            sqlx::query("UPDATE dataset_jobs SET status='failed',error_code='input_integrity',input=NULL,lease_token=NULL,lease_until=NULL WHERE devspace_id=$1 AND id=$2 AND status='running' AND lease_token=$3")
                .bind(&lease.ds).bind(&lease.id).bind(&lease.token).execute(&pg.pool).await?;
            false
        }
    };
    Ok(json!({"scheduled":scheduled,"published":published}))
}
