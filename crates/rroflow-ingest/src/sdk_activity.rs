//! Durable, account-scoped SDK monitoring. Raw events remain the source of truth.
mod worker;
use crate::{ApiError, AppState, PgStore, Store, internal};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use rroflow_core::activity::{Activity, revision};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, types::Json as SqlJson};
pub use worker::run_once;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivityConfig {
    pub embedded_worker: bool,
    pub max_events_per_capture: usize,
    pub max_snapshot_bytes: usize,
    pub poll_seconds: u64,
    pub lease_seconds: u64,
    pub max_attempts: i32,
    pub retry_seconds: u64,
}
impl Default for ActivityConfig {
    fn default() -> Self {
        Self {
            embedded_worker: true,
            max_events_per_capture: 20000,
            max_snapshot_bytes: 16 * 1024 * 1024,
            poll_seconds: 2,
            lease_seconds: 60,
            max_attempts: 3,
            retry_seconds: 30,
        }
    }
}
impl ActivityConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=500000).contains(&self.max_events_per_capture)
                && (1024..=256 * 1024 * 1024).contains(&self.max_snapshot_bytes)
                && (1..=60).contains(&self.poll_seconds)
                && (5..=3600).contains(&self.lease_seconds)
                && (1..=10).contains(&self.max_attempts)
                && (1..=3600).contains(&self.retry_seconds),
            "invalid SDK projection bounds"
        );
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter {
    pub agent: Option<String>,
    pub session: Option<String>,
    pub outcome: Option<String>,
    pub signal: Option<String>,
    pub search: Option<String>,
    pub id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub has_findings: Option<bool>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    at: String,
    id: String,
}
macro_rules! cohort { () => { "a.devspace_id=$1 AND ($2::text IS NULL OR a.agent=$2) AND ($3::text IS NULL OR a.session_id=$3) AND ($4::text IS NULL OR a.outcome=$4) AND ($5::text IS NULL OR $5=ANY(a.signals)) AND ($6::text IS NULL OR strpos(a.search_text,lower($6))>0) AND ($7::text IS NULL OR a.activity_id=$7) AND ($8::timestamptz IS NULL OR a.first_observed_at >= $8) AND ($9::timestamptz IS NULL OR a.first_observed_at < $9) AND (NOT coalesce($10::boolean,false) OR cardinality(a.signals)>0)" }; }
pub(crate) async fn list(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(f): Query<Filter>,
) -> Result<Json<Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Ok(Json(
            json!({"available":false,"reason":"SDK activity indexing needs the configured PostgreSQL service. Raw SDK events remain available in Sources."}),
        ));
    };
    let bad = || ApiError::BadRequest("Invalid SDK activity filter or cursor.".into());
    for v in [
        &f.agent, &f.session, &f.outcome, &f.signal, &f.search, &f.id,
    ] {
        if v.as_ref()
            .is_some_and(|v| v.len() > 256 || v.chars().any(char::is_control))
        {
            return Err(bad());
        }
    }
    let since = f
        .since
        .as_deref()
        .map(crate::pg::timestamp)
        .transpose_option()
        .ok_or_else(bad)?;
    let until = f
        .until
        .as_deref()
        .map(crate::pg::timestamp)
        .transpose_option()
        .ok_or_else(bad)?;
    if since.zip(until).is_some_and(|(a, b)| a >= b) {
        return Err(bad());
    }
    let cursor = f
        .cursor
        .as_ref()
        .map(|s| {
            if s.len() > 512 {
                return Err(bad());
            }
            serde_json::from_str::<Cursor>(s).map_err(|_| bad())
        })
        .transpose()?;
    let cursor_at = cursor
        .as_ref()
        .map(|c| crate::pg::timestamp(&c.at))
        .transpose_option()
        .ok_or_else(bad)?;
    let limit = f.limit.unwrap_or(50).clamp(1, 100);
    let mut tx = pg.pool.begin().await.map_err(|e| internal(e.into()))?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    sqlx::query("SET LOCAL statement_timeout='10s'")
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    const QUERY: &str = concat!(
        "SELECT CASE WHEN $7::text IS NOT NULL THEN a.record ELSE jsonb_build_object('activity_id',a.activity_id,'agent',a.agent,'session_id',a.session_id,'kind',a.record->'kind','outcome',a.outcome,'outcome_basis',a.record->'outcome_basis','first_observed_at',a.record->'first_observed_at','models',jsonb_path_query_array(a.record,'$.models[0 to 4]'),'finding_count',jsonb_array_length(a.record->'findings'),'event_count',jsonb_array_length(a.record->'events'),'tool_count',jsonb_array_length(a.record->'tools'),'signals',to_jsonb(a.signals)) END AS record,a.revision,w.source_revision<>a.source_revision AS pending FROM sdk_activities a JOIN sdk_activity_work w USING(devspace_id,capture_id) WHERE ",
        cohort!(),
        " AND ($11::timestamptz IS NULL OR a.first_observed_at<$11 OR (a.first_observed_at=$11 AND a.activity_id>$12)) ORDER BY a.first_observed_at DESC,a.activity_id LIMIT $13"
    );
    let rows = sqlx::query(QUERY)
        .bind(&ds)
        .bind(&f.agent)
        .bind(&f.session)
        .bind(&f.outcome)
        .bind(&f.signal)
        .bind(&f.search)
        .bind(&f.id)
        .bind(since)
        .bind(until)
        .bind(f.has_findings)
        .bind(cursor_at)
        .bind(cursor.as_ref().map(|c| &c.id))
        .bind((limit + 1) as i64)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    let count = sqlx::query_scalar::<_, i64>(concat!(
        "SELECT count(*) FROM sdk_activities a WHERE ",
        cohort!()
    ))
    .bind(&ds)
    .bind(&f.agent)
    .bind(&f.session)
    .bind(&f.outcome)
    .bind(&f.signal)
    .bind(&f.search)
    .bind(&f.id)
    .bind(since)
    .bind(until)
    .bind(f.has_findings)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    let coverage=sqlx::query("SELECT count(*) FILTER(WHERE status<>'ready') AS pending,count(*) FILTER(WHERE status='error') AS errors FROM sdk_activity_work WHERE devspace_id=$1").bind(&ds).fetch_one(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let mut activities = vec![];
    let more = rows.len() > limit;
    for r in rows.into_iter().take(limit) {
        let activity = r.get::<SqlJson<Value>, _>("record").0;
        activities.push(json!({"activity":activity,"revision":r.get::<String,_>("revision"),"new_events_pending":r.get::<bool,_>("pending")}));
    }
    let next = if more {
        activities.last().map(|v| {
            serde_json::to_string(&Cursor {
                at: v["activity"]["first_observed_at"].as_str().unwrap().into(),
                id: v["activity"]["activity_id"].as_str().unwrap().into(),
            })
            .expect("cursor")
        })
    } else {
        None
    };
    tx.commit().await.map_err(|e| internal(e.into()))?;
    Ok(Json(
        json!({"available":true,"devspace":ds,"activities":activities,"matched_activities":count,"next_cursor":next,"coverage":{"pending_captures":coverage.get::<i64,_>("pending"),"error_captures":coverage.get::<i64,_>("errors")},"as_of":chrono::Utc::now().to_rfc3339()}),
    ))
}
// Option<Option<T>>: distinguish an absent optional timestamp from an invalid one.
trait OptionalTimestamp<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> OptionalTimestamp<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(Some(v)) => Some(Some(v)),
            Some(None) => None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewRequest {
    review_id: String,
    activity_id: String,
    revision: String,
    signal: String,
    rule_version: u32,
    verdict: String,
}
#[derive(Clone, Deserialize, Serialize)]
struct Review {
    request: ReviewRequest,
    reviewer: String,
    recorded_at: String,
    snapshot: Activity,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewFilter {
    activity_id: Option<String>,
    review_id: Option<String>,
}
pub(crate) async fn reviews(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(f): Query<ReviewFilter>,
) -> Result<Json<Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    if [&f.activity_id, &f.review_id]
        .iter()
        .any(|v| v.as_ref().is_some_and(|v| v.len() > 80))
    {
        return Err(ApiError::BadRequest("Invalid SDK review filter.".into()));
    }
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Ok(Json(json!({"reviews":[],"available":false})));
    };
    let rows:Vec<SqlJson<Value>>=sqlx::query_scalar("SELECT CASE WHEN $3::text IS NOT NULL THEN record ELSE record-'snapshot' END FROM sdk_activity_reviews WHERE devspace_id=$1 AND ($2::text IS NULL OR activity_id=$2) AND ($3::text IS NULL OR review_id=$3) ORDER BY recorded_at DESC,review_id LIMIT 1000")
        .bind(ds).bind(f.activity_id).bind(f.review_id).fetch_all(&pg.pool).await.map_err(|e|internal(e.into()))?;
    Ok(Json(
        json!({"available":true,"reviews":rows.into_iter().map(|r|r.0).collect::<Vec<_>>(),"limit":1000}),
    ))
}
pub(crate) async fn save_review(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(r): Json<ReviewRequest>,
) -> Result<Json<Value>, ApiError> {
    let caller = st.caller(&headers).await?;
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Err(ApiError::BadRequest("SDK reviews need PostgreSQL.".into()));
    };
    let uuid = r.review_id.len() == 36
        && r.review_id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        });
    if !uuid
        || r.activity_id.len() > 80
        || r.revision.len() != 64
        || !["match", "not_match"].contains(&r.verdict.as_str())
        || r.signal.len() > 128
    {
        return Err(ApiError::BadRequest("Invalid SDK review.".into()));
    }
    let reviewer = caller.subject.unwrap_or_else(|| "devspace-key".into());
    let mut tx = pg.pool.begin().await.map_err(|e| internal(e.into()))?;
    let old: Option<SqlJson<Review>> = sqlx::query_scalar(
        "SELECT record FROM sdk_activity_reviews WHERE devspace_id=$1 AND review_id=$2",
    )
    .bind(&caller.devspace)
    .bind(&r.review_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    if let Some(old) = old {
        if old.0.request != r || old.0.reviewer != reviewer {
            return Err(ApiError::Conflict);
        }
        return Ok(Json(json!({"saved":true,"review_id":r.review_id})));
    }
    let snapshot: Option<SqlJson<Activity>> = sqlx::query_scalar(
        "SELECT record FROM sdk_activities WHERE devspace_id=$1 AND activity_id=$2 FOR SHARE",
    )
    .bind(&caller.devspace)
    .bind(&r.activity_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    let snapshot = snapshot
        .ok_or_else(|| {
            ApiError::BadRequest("SDK activity was not found in your workspace.".into())
        })?
        .0;
    if revision(&snapshot) != r.revision
        || !snapshot
            .findings
            .iter()
            .any(|f| f.signal == r.signal && f.rule_version == r.rule_version)
    {
        return Err(ApiError::BadRequest(
            "Evidence changed or the signal is absent. Refresh before reviewing.".into(),
        ));
    }
    let review = Review {
        request: r.clone(),
        reviewer,
        recorded_at: chrono::Utc::now().to_rfc3339(),
        snapshot,
    };
    sqlx::query("INSERT INTO sdk_activity_reviews(devspace_id,review_id,activity_id,record) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(&caller.devspace).bind(&r.review_id).bind(&r.activity_id).bind(SqlJson(&review)).execute(&mut *tx).await.map_err(|e|internal(e.into()))?;
    let stored: SqlJson<Review> = sqlx::query_scalar(
        "SELECT record FROM sdk_activity_reviews WHERE devspace_id=$1 AND review_id=$2",
    )
    .bind(&caller.devspace)
    .bind(&r.review_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    if stored.0.request != r || stored.0.reviewer != review.reviewer {
        return Err(ApiError::Conflict);
    }
    tx.commit().await.map_err(|e| internal(e.into()))?;
    Ok(Json(json!({"saved":true,"review_id":r.review_id})))
}

pub async fn retry_errors(pg: &PgStore) -> anyhow::Result<()> {
    sqlx::query("UPDATE sdk_activity_work SET status='queued',attempts=0,next_attempt_at=clock_timestamp(),last_error=NULL WHERE status='error'").execute(&pg.pool).await?;
    Ok(())
}
pub fn start_worker(
    store: std::sync::Arc<Store>,
    cfg: ActivityConfig,
) -> Option<tokio::task::JoinHandle<()>> {
    if !cfg.embedded_worker || !matches!(store.as_ref(), Store::Postgres(_)) {
        return None;
    }
    Some(tokio::spawn(async move {
        let Store::Postgres(pg) = store.as_ref() else {
            return;
        };
        loop {
            match run_once(pg, &cfg).await {
                Ok(status) if status["worked"] == true && status["error"].is_null() => {
                    tokio::task::yield_now().await;
                    continue;
                }
                Ok(_) => {}
                Err(_) => eprintln!("SDK activity worker unavailable; durable work remains queued"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(cfg.poll_seconds)).await;
        }
    }))
}
#[cfg(test)]
mod tests;
