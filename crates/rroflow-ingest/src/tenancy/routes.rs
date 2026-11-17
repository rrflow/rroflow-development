//! Tenancy routes, and the handlers moved here so far. The rest move here in refactor R3.
use super::{Role, db, signed_in};
use crate::permissions::Grant;
use crate::{ApiError, AppState};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router, extract::State, http::HeaderMap};
use serde_json::{Value, json};
use sqlx::types::Json as SqlJson;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/me", get(me))
        .route("/v1/organizations", post(super::create_organization))
        .route(
            "/v1/organizations/{organization}/projects",
            post(super::create_project),
        )
        .route(
            "/v1/organizations/{organization}/members",
            get(super::members),
        )
        .route(
            "/v1/organizations/{organization}/members/{user}",
            put(super::set_role).delete(super::remove_member),
        )
        .route(
            "/v1/organizations/{organization}/invitations",
            get(super::invitations).post(super::invite),
        )
        .route(
            "/v1/organizations/{organization}/invitations/{invitation}",
            delete(super::revoke_invitation),
        )
        .route(
            "/v1/organizations/{organization}/audit",
            get(super::audit_log),
        )
        .route("/v1/invitations/accept", post(super::accept_invitation))
        .route(
            "/v1/projects/{project}/keys",
            get(super::keys).post(super::create_key),
        )
        .route(
            "/v1/projects/{project}/keys/{key}",
            delete(super::revoke_key),
        )
}

/// The account, its organizations and their projects: what a console needs to start.
pub(crate) async fn me(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let name: Option<String> = sqlx::query_scalar("SELECT display_name FROM users WHERE id=$1")
        .bind(&user)
        .fetch_one(&pg.pool)
        .await
        .map_err(db)?;
    let default_project: Option<String> = sqlx::query_scalar("SELECT d.id FROM devspaces d JOIN user_identities i ON i.issuer=d.owner_issuer AND i.subject=d.owner_subject WHERE i.user_id=$1 ORDER BY d.created_at,d.id LIMIT 1")
        .bind(&user).fetch_optional(&pg.pool).await.map_err(db)?;
    let organizations: Vec<SqlJson<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'slug',o.slug,'name',o.name,'personal',o.personal,'role',m.role,'projects',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',d.id,'slug',d.slug,'name',d.name) ORDER BY d.created_at,d.id) FROM devspaces d WHERE d.organization_id=o.id),'[]'::jsonb)) FROM organization_members m JOIN organizations o ON o.id=m.organization_id WHERE m.user_id=$1 ORDER BY o.personal DESC,o.created_at,o.id")
        .bind(&user).fetch_all(&pg.pool).await.map_err(db)?;
    let instance_admin = crate::settings::is_instance_admin(&pg.pool, &user)
        .await
        .map_err(db)?;
    Ok(Json(json!({
        "user": {"id": user, "name": name},
        "instance_admin": instance_admin,
        "default_project": default_project,
        "organizations": organizations.into_iter().map(|o| with_permissions(o.0)).collect::<Vec<_>>(),
    })))
}

/// Adds what the member's role allows there, so a console offers only what will work.
fn with_permissions(mut organization: Value) -> Value {
    let permissions = organization["role"]
        .as_str()
        .and_then(|role| Role::parse(role).ok())
        .map(|role| Grant::Role(role).permissions().to_vec())
        .unwrap_or_default();
    organization["permissions"] = json!(permissions);
    organization
}
