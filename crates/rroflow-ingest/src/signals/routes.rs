//! Signal handlers.
use super::model::catalogue;
use crate::{ApiError, AppState, Permission, internal, permissions, store};
use axum::{Json, extract::State, http::HeaderMap};
use rroflow_core::signals::SignalConfig;

pub(crate) async fn get_signals() -> Json<serde_json::Value> {
    Json(catalogue())
}

/// Collectors read this before evaluating, including with a write-only key. Custom rules run
/// on the server and can describe private work, so that key does not receive them.
pub(crate) async fn get_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = st.authenticate(&headers).await?;
    let reader = caller.grant().allows(Permission::ProjectRead);
    if !reader {
        permissions::require(caller.grant(), Permission::ProjectSend)?;
    }
    let mut settings = st
        .store
        .signal_config(&caller.devspace)
        .await
        .map_err(internal)?;
    if !reader {
        settings.custom.clear();
    }
    Ok(Json(
        serde_json::json!({ "devspace": caller.devspace, "settings": settings }),
    ))
}

pub(crate) async fn put_signal_settings(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(mut cfg): Json<SignalConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectWrite)
        .await?
        .devspace;
    cfg.validate().map_err(ApiError::BadRequest)?;
    let old = st.store.signal_config(&devspace).await.map_err(internal)?;
    if old.revision != cfg.revision {
        return Err(ApiError::SettingsConflict);
    }
    cfg.validate_update(&old).map_err(ApiError::BadRequest)?;
    let saved = st
        .store
        .set_signal_config(&devspace, &cfg)
        .await
        .map_err(internal)?;
    if !saved {
        return Err(ApiError::SettingsConflict);
    }
    cfg.revision += 1;
    Ok(Json(
        serde_json::json!({ "devspace": devspace, "settings": cfg }),
    ))
}

pub(crate) async fn preview_signal(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(rule): Json<rroflow_core::custom::CustomSignal>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devspace = st
        .allowed(&headers, crate::Permission::ProjectRead)
        .await?
        .devspace;
    rule.validate().map_err(ApiError::BadRequest)?;
    let turns = st
        .store
        .list(
            &devspace,
            101,
            &store::TurnFilter {
                agent: rule.agent.as_deref(),
                project: rule.project.as_deref(),
                ..Default::default()
            },
        )
        .await
        .map_err(internal)?;
    let mut matches = vec![];
    let mut evaluated = 0;
    let mut unavailable = 0;
    for turn in turns.iter().take(100) {
        match rule.evaluate(turn) {
            rroflow_core::custom::Evaluation::Match(hit) => {
                evaluated += 1;
                matches.push(serde_json::json!({"event_id":turn.event_id,"hit":hit}));
            }
            rroflow_core::custom::Evaluation::NoMatch => evaluated += 1,
            rroflow_core::custom::Evaluation::Unavailable => unavailable += 1,
            rroflow_core::custom::Evaluation::OutOfScope => {}
        }
    }
    Ok(Json(
        serde_json::json!({"devspace":devspace,"scanned":turns.len().min(100),"evaluated":evaluated,"unavailable":unavailable,"more_history":turns.len()>100,"matches":matches}),
    ))
}
