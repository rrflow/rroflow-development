//! Turn routes and handlers.
use super::model::{LIST_MAX, ListQuery, TurnsBody, check_record};
use crate::signals::catalogue;
use crate::{ApiError, AppState, Store, TurnFilter, internal, pg, provenance, reviews};
use axum::Router;
use axum::routing::{get, post};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/turns", post(post_turns).get(get_turns))
        .route("/v1/summary", get(get_summary))
}

pub(crate) async fn post_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(mut body): Json<TurnsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = st.allowed(&headers, crate::Permission::ProjectSend).await?;
    let devspace = caller.devspace.clone();
    if body.turns.len() > st.max_batch {
        return Err(ApiError::BadRequest(format!(
            "batch of {} exceeds the limit of {}",
            body.turns.len(),
            st.max_batch
        )));
    }
    for (i, r) in body.turns.iter().enumerate() {
        check_record(r).map_err(|m| ApiError::BadRequest(format!("turns[{i}]: {m}")))?;
    }
    let settings = st.store.signal_config(&devspace).await.map_err(internal)?;
    for turn in &mut body.turns {
        // This namespace belongs to the receiving devspace, never the submitting client.
        turn.signals
            .retain(|hit| !hit.signal.starts_with("custom_"));
        for signal in settings.custom.iter().filter(|s| s.enabled) {
            if let rroflow_core::custom::Evaluation::Match(hit) = signal.evaluate(turn) {
                turn.signals.push(hit);
            }
        }
    }
    let up = match st.store.as_ref() {
        Store::Postgres(pg) => {
            pg.upsert_received(&devspace, &body.turns, Some(&caller))
                .await
        }
        Store::Jsonl(_) => st.store.upsert(&devspace, &body.turns).await,
    }
    .map_err(provenance::ingest_error)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "accepted": body.turns.len(),
        "new": up.new,
        "replaced": up.replaced,
    })))
}

pub(crate) async fn get_turns(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let limit = q.limit.unwrap_or(50).clamp(1, LIST_MAX);
    let search = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if search.is_some_and(|s| s.chars().count() > 200) {
        return Err(ApiError::BadRequest(
            "Search must be 200 characters or fewer".into(),
        ));
    }
    let since = q
        .since
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid start date".into()))
        })
        .transpose()?;
    let until = q
        .until
        .as_deref()
        .map(|value| {
            pg::timestamp(value).ok_or_else(|| ApiError::BadRequest("Invalid end date".into()))
        })
        .transpose()?;
    if since.zip(until).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::BadRequest("Start must be before end".into()));
    }
    let cursor: Option<(String, String)> = q
        .cursor
        .as_deref()
        .map(|value| {
            if value.len() > 4096 {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            URL_SAFE_NO_PAD
                .decode(value)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .ok_or_else(|| ApiError::BadRequest("Invalid page cursor".into()))
        })
        .transpose()?;
    let before = cursor
        .as_ref()
        .map(|(at, id)| {
            if id.is_empty() || (!at.is_empty() && pg::timestamp(at).is_none()) {
                return Err(ApiError::BadRequest("Invalid page cursor".into()));
            }
            Ok((pg::timestamp(at), id.as_str()))
        })
        .transpose()?;
    let mut turns = st
        .store
        .list(
            &devspace,
            limit + 1,
            &TurnFilter {
                agent: q.agent.as_deref().filter(|a| !a.is_empty()),
                signal: q.signal.as_deref().filter(|s| !s.is_empty()),
                project: q.project.as_deref().filter(|s| !s.is_empty()),
                session: q.session.as_deref().filter(|s| !s.is_empty()),
                search,
                since,
                until,
                before,
                event: q.event.as_deref().filter(|s| !s.is_empty()),
            },
        )
        .await
        .map_err(internal)?;
    let has_more = turns.len() > limit;
    turns.truncate(limit);
    let next_cursor = if has_more {
        turns.last().map(|turn| {
            URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(&(
                    pg::timestamp(&turn.started_at)
                        .map(|at| at.to_rfc3339())
                        .unwrap_or_default(),
                    &turn.event_id,
                ))
                .expect("string tuple serializes"),
            )
        })
    } else {
        None
    };
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "next_cursor": next_cursor, "revisions":turns.iter().map(|t| (t.event_id.clone(), reviews::digest(t))).collect::<std::collections::BTreeMap<_,_>>(), "turns": turns }),
    ))
}

pub(crate) async fn get_summary(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    let (summary, settings) = tokio::try_join!(
        st.store.summary(&devspace),
        st.store.signal_config(&devspace)
    )
    .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "devspace": devspace,
        "summary": summary,
        "catalogue": catalogue(),
        "settings": settings,
    })))
}
