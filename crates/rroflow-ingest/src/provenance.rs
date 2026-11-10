//! Server receipts, never evidence that the claimed provider performed an action.
use crate::{ApiError, AppState, Caller, Store, internal};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction, types::Json as PgJson};
use std::collections::HashMap;

pub(crate) const SCHEMA: &str = "rroflow.provenance/v1";

#[derive(Debug)]
pub(crate) struct ReplacementDenied;
impl std::fmt::Display for ReplacementDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("this project key cannot replace another sender's record")
    }
}
impl std::error::Error for ReplacementDenied {}

pub(crate) fn ingest_error(error: anyhow::Error) -> ApiError {
    if error.downcast_ref::<ReplacementDenied>().is_some() {
        ApiError::Forbidden("this project key cannot replace another sender's record")
    } else {
        crate::allowances::ingest_error(error)
    }
}

// Turn v1 historically tolerated unknown fields. Reserve only the server proof fields,
// preserving that compatibility for unrelated extensions.
pub(crate) fn deserialize_turns<'de, D>(d: D) -> Result<Vec<rroflow_core::TurnRecord>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<Value>::deserialize(d)?;
    values
        .into_iter()
        .map(|value| {
            if value.get("attested").is_some() || value.get("provenance").is_some() {
                return Err(serde::de::Error::custom(
                    "server provenance cannot be submitted",
                ));
            }
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        })
        .collect()
}

fn select_records(kind: &str) -> &'static str {
    match kind {
        "turn" => {
            "SELECT t.event_id,t.record,p.receipt FROM turns t LEFT JOIN record_provenance p ON p.devspace_id=t.devspace_id AND p.kind='turn' AND p.record_id=t.event_id WHERE t.devspace_id=$1 AND t.event_id=ANY($2)"
        }
        "event" => {
            "SELECT t.event_id,t.record,p.receipt FROM capture_events t LEFT JOIN record_provenance p ON p.devspace_id=t.devspace_id AND p.kind='event' AND p.record_id=t.event_id WHERE t.devspace_id=$1 AND t.event_id=ANY($2)"
        }
        "source" => {
            "SELECT t.source_id,t.observation->'report',p.receipt FROM source_health t LEFT JOIN record_provenance p ON p.devspace_id=t.devspace_id AND p.kind='source' AND p.record_id=t.source_id WHERE t.devspace_id=$1 AND t.source_id=ANY($2)"
        }
        _ => unreachable!("validated record kind"),
    }
}
type Existing = (String, PgJson<Value>, Option<PgJson<Value>>);

fn claims(value: &Value) -> Value {
    let mut claims = serde_json::Map::new();
    let mut omitted = vec![];
    // No prompt, response, tool payload, local path, key or identity-provider claims.
    for field in [
        "agent",
        "agent_version",
        "provider",
        "model",
        "session_id",
        "collector",
        "collector_version",
        "started_at",
        "ended_at",
        "observed_at",
        "git_branch",
        "project",
    ] {
        if let Some(text) = value.get(field).and_then(Value::as_str) {
            if text.chars().take(501).count() > 500 {
                omitted.push(field);
            } else {
                claims.insert(field.into(), Value::String(text.into()));
            }
        }
    }
    if !omitted.is_empty() {
        claims.insert("omitted_over_limit".into(), json!(omitted));
    }
    Value::Object(claims)
}

#[cfg(test)]
mod tests {
    #[test]
    fn claims_do_not_copy_content_invent_methods_or_silently_shorten_identifiers() {
        let input = serde_json::json!({"collector":"c".repeat(501),"agent":"codex",
            "content":{"input":"private prompt"},"cwd":"private path",
            "credential":"private key","capture_method":"client speculation"});
        assert_eq!(
            super::claims(&input),
            serde_json::json!({"agent":"codex","omitted_over_limit":["collector"]})
        );
    }
}

/// Must run under the project's write lock, before writing the records, in that same
/// transaction. Round trips stay bounded per batch rather than per record.
pub(crate) async fn record_batch(
    tx: &mut Transaction<'_, Postgres>,
    project: &str,
    kind: &str,
    records: &[(String, Value)],
    caller: Option<&Caller>,
) -> anyhow::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let ids: Vec<_> = records.iter().map(|(id, _)| id.clone()).collect();
    let old: Vec<Existing> = sqlx::query_as(select_records(kind))
        .bind(project)
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    let old: HashMap<_, _> = old
        .into_iter()
        .map(|(id, value, receipt)| (id, (value.0, receipt.map(|r| r.0))))
        .collect();
    let organization: Option<String> =
        sqlx::query_scalar("SELECT organization_id FROM devspaces WHERE id=$1")
            .bind(project)
            .fetch_one(&mut **tx)
            .await?;
    let mut changed_ids = vec![];
    let mut receipts = vec![];
    for (id, value) in records {
        if let Some((stored, receipt)) = old.get(id) {
            if stored == value {
                continue;
            }
            if let Some(caller) = caller.filter(|c| c.write_only) {
                let owner = receipt
                    .as_ref()
                    .and_then(|r| r.pointer("/attested/credential/id"))
                    .and_then(Value::as_str);
                let is_key = receipt
                    .as_ref()
                    .and_then(|r| r.pointer("/attested/credential/kind"))
                    .and_then(Value::as_str)
                    == Some("project_key");
                if !is_key || caller.key.as_deref().is_none() || owner != caller.key.as_deref() {
                    return Err(ReplacementDenied.into());
                }
            }
        }
        changed_ids.push(id.clone());
        if let Some(caller) = caller {
            anyhow::ensure!(caller.devspace == project, "receipt project mismatch");
            let (kind, credential_id) = if caller.write_only {
                ("project_key", caller.key.as_deref())
            } else if caller.user.is_some() {
                ("member", caller.user.as_deref())
            } else {
                ("operator_key", None)
            };
            receipts.push(json!({
                "schema": SCHEMA, "status": "recorded",
                "attested": {
                    "organization": organization, "project": project,
                    "credential": {"kind":kind,"id":credential_id},
                    "received_at": chrono::Utc::now().to_rfc3339(),
                    "service_revision": crate::build_info().revision,
                    "record_sha256": hex::encode(Sha256::digest(serde_json::to_vec(value)?)),
                },
                "claimed": claims(value),
            }));
        }
    }
    if caller.is_some() {
        sqlx::query("INSERT INTO record_provenance(devspace_id,kind,record_id,receipt) SELECT $1,$2,* FROM UNNEST($3::text[],$4::jsonb[]) ON CONFLICT(devspace_id,kind,record_id) DO UPDATE SET receipt=EXCLUDED.receipt")
            .bind(project).bind(kind).bind(&changed_ids).bind(&receipts).execute(&mut **tx).await?;
    } else {
        // Internal fixture/import paths cannot keep an old receipt attached to changed data.
        sqlx::query(
            "DELETE FROM record_provenance WHERE devspace_id=$1 AND kind=$2 AND record_id=ANY($3)",
        )
        .bind(project)
        .bind(kind)
        .bind(&changed_ids)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lookup {
    kind: String,
    record_id: String,
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<Lookup>,
) -> Result<Json<Value>, ApiError> {
    let caller = st.allowed(&headers, crate::Permission::ProjectRead).await?;
    if !["turn", "event", "source"].contains(&q.kind.as_str())
        || q.record_id.is_empty()
        || q.record_id.len() > 1024
    {
        return Err(ApiError::BadRequest(
            "invalid provenance record reference".into(),
        ));
    }
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Err(ApiError::BadRequest(
            "receipt provenance requires PostgreSQL storage".into(),
        ));
    };
    let row: Option<Existing> = sqlx::query_as(select_records(&q.kind))
        .bind(&caller.devspace)
        .bind(vec![&q.record_id])
        .fetch_optional(&pg.pool)
        .await
        .map_err(|e| internal(e.into()))?;
    let (_, _, receipt) = row.ok_or(ApiError::Missing)?;
    Ok(Json(
        json!({"project":caller.devspace,"kind":q.kind,"record_id":q.record_id,
        "provenance":receipt.map(|r| r.0).unwrap_or_else(|| json!({"schema":SCHEMA,"status":"unknown","attested":null,"claimed":null}))}),
    ))
}

pub(crate) async fn coverage(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let caller = st.allowed(&headers, crate::Permission::ProjectRead).await?;
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Err(ApiError::BadRequest(
            "receipt provenance requires PostgreSQL storage".into(),
        ));
    };
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "WITH records AS (SELECT 'turn' kind,event_id id FROM turns WHERE devspace_id=$1 UNION ALL SELECT 'event',event_id FROM capture_events WHERE devspace_id=$1 UNION ALL SELECT 'source',source_id FROM source_health WHERE devspace_id=$1) SELECT r.kind,count(*),count(p.record_id) FROM records r LEFT JOIN record_provenance p ON p.devspace_id=$1 AND p.kind=r.kind AND p.record_id=r.id GROUP BY r.kind")
        .bind(&caller.devspace).fetch_all(&pg.pool).await.map_err(|e| internal(e.into()))?;
    let counts: Vec<_> = ["turn", "event", "source"]
        .into_iter()
        .map(|kind| {
            let (total, recorded) = rows
                .iter()
                .find(|(k, _, _)| k == kind)
                .map(|(_, total, recorded)| (*total, *recorded))
                .unwrap_or((0, 0));
            json!({"kind":kind,"total":total,"recorded":recorded,"unknown":total-recorded})
        })
        .collect();
    Ok(Json(
        json!({"schema":SCHEMA,"project":caller.devspace,"records":counts}),
    ))
}
