//! Immutable event ingest. Identical replay succeeds; conflicting reuse is refused.
use crate::{ApiError, AppState, Store, internal};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use rroflow_core::capture::CaptureEvent;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Batch {
    events: Vec<CaptureEvent>,
}
#[derive(Deserialize)]
pub(crate) struct Filter {
    agent: Option<String>,
    capture_id: Option<String>,
    limit: Option<usize>,
}

pub(crate) async fn ingest(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(batch): Json<Batch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    if batch.events.len() > st.max_batch {
        return Err(ApiError::BadRequest("capture batch exceeds limit".into()));
    }
    for event in &batch.events {
        event
            .validate()
            .map_err(|m| ApiError::BadRequest(m.into()))?;
        if crate::pg::timestamp(&event.observed_at).is_none() || event.sequence > i64::MAX as u64 {
            return Err(ApiError::BadRequest(
                "invalid capture time or sequence".into(),
            ));
        }
    }
    let saved = match st.store.as_ref() {
        Store::Postgres(p) => p.capture_insert(&ds, &batch.events).await,
        Store::Jsonl(s) => s.capture_insert(&ds, &batch.events),
    };
    // Conflicts have a stable error type; database/disk errors remain retryable 500s.
    match saved {
        Ok(()) => {}
        Err(e) if e.downcast_ref::<Conflict>().is_some() => return Err(ApiError::Conflict),
        Err(e) => return Err(crate::allowances::ingest_error(e)),
    }
    Ok(Json(
        serde_json::json!({"devspace": ds, "accepted_ids": batch.events.iter().map(|e| &e.event_id).collect::<Vec<_>>()}),
    ))
}
pub(crate) async fn list(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(f): Query<Filter>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    let limit = f.limit.unwrap_or(100).min(1000);
    let events = match st.store.as_ref() {
        Store::Postgres(p) => {
            p.capture_list(&ds, f.agent.as_deref(), f.capture_id.as_deref(), limit)
                .await
        }
        Store::Jsonl(s) => s.capture_list(&ds, f.agent.as_deref(), f.capture_id.as_deref(), limit),
    }
    .map_err(internal)?;
    Ok(Json(serde_json::json!({"devspace": ds, "events": events})))
}

#[derive(Debug)]
pub(crate) struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("capture identity already holds a different event")
    }
}
impl std::error::Error for Conflict {}
