//! Bounded latest-only source reports. Replay cannot manufacture fresh liveness.
use crate::{ApiError, AppState, JsonlStore, PgStore, Store, internal};
use axum::{Json, extract::State, http::HeaderMap};
use rroflow_core::source_health::{SourceObservation, SourceReport};
use serde_json::json;
use sqlx::types::Json as PgJson;
use std::io::Write;

const MAX_SOURCES: usize = 128;

#[derive(Debug)]
struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source report sequence conflict")
    }
}
impl std::error::Error for Conflict {}

#[derive(Debug)]
struct Capacity;
impl std::fmt::Display for Capacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source reporting supports at most 128 sources per devspace in this version")
    }
}
impl std::error::Error for Capacity {}

fn accept(
    old: Option<&SourceObservation>,
    report: &SourceReport,
) -> anyhow::Result<Option<SourceObservation>> {
    if let Some(old) = old {
        if old.report == *report {
            return Ok(Some(old.clone()));
        }
        if report.sequence <= old.report.sequence {
            return Err(Conflict.into());
        }
    }
    Ok(None)
}

impl JsonlStore {
    fn health_read(&self, ds: &str) -> anyhow::Result<Vec<SourceObservation>> {
        let dir = self.root.join(ds).join("source-health");
        let mut out = vec![];
        if !dir.exists() {
            return Ok(out);
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|e| e == "json") {
                anyhow::ensure!(
                    entry.file_type()?.is_file(),
                    "source report must be a regular file"
                );
                out.push(serde_json::from_slice(&std::fs::read(entry.path())?)?);
            }
        }
        Ok(out)
    }
    fn health_save(&self, ds: &str, report: SourceReport) -> anyhow::Result<SourceObservation> {
        let _guard = self.health_lock.lock().expect("source health lock");
        let all = self.health_read(ds)?;
        let old = all.iter().find(|o| o.report.source_id == report.source_id);
        if let Some(replay) = accept(old, &report)? {
            return Ok(replay);
        }
        if old.is_none() && all.len() >= MAX_SOURCES {
            return Err(Capacity.into());
        }
        let saved = SourceObservation {
            report,
            received_at: chrono::Utc::now().to_rfc3339(),
        };
        let dir = self.root.join(ds).join("source-health");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", saved.report.source_id));
        let tmp = path.with_extension("tmp");
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&serde_json::to_vec(&saved)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(tmp, path)?;
        #[cfg(unix)]
        std::fs::File::open(dir)?.sync_all()?;
        Ok(saved)
    }
}

impl PgStore {
    async fn health_read(&self, ds: &str) -> anyhow::Result<Vec<SourceObservation>> {
        let rows: Vec<PgJson<SourceObservation>> = sqlx::query_scalar("SELECT observation FROM source_health WHERE devspace_id=$1 ORDER BY source_id LIMIT 128")
            .bind(ds).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
    async fn health_save(
        &self,
        ds: &str,
        report: SourceReport,
    ) -> anyhow::Result<SourceObservation> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO devspaces (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(ds)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR UPDATE")
            .bind(ds)
            .fetch_one(&mut *tx)
            .await?;
        let old: Option<PgJson<SourceObservation>> = sqlx::query_scalar(
            "SELECT observation FROM source_health WHERE devspace_id=$1 AND source_id=$2",
        )
        .bind(ds)
        .bind(&report.source_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(replay) = accept(old.as_ref().map(|o| &o.0), &report)? {
            return Ok(replay);
        }
        if old.is_none() {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM source_health WHERE devspace_id=$1")
                    .bind(ds)
                    .fetch_one(&mut *tx)
                    .await?;
            if count >= MAX_SOURCES as i64 {
                return Err(Capacity.into());
            }
        }
        let saved = SourceObservation {
            report,
            received_at: chrono::Utc::now().to_rfc3339(),
        };
        sqlx::query("INSERT INTO source_health (devspace_id,source_id,observation) VALUES ($1,$2,$3) ON CONFLICT (devspace_id,source_id) DO UPDATE SET observation=EXCLUDED.observation")
            .bind(ds).bind(&saved.report.source_id).bind(PgJson(&saved)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(saved)
    }
}

pub(crate) async fn ingest(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(report): Json<SourceReport>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    report
        .validate()
        .map_err(|m| ApiError::BadRequest(m.into()))?;
    if crate::pg::timestamp(&report.observed_at).is_none()
        || report
            .oldest_pending_at
            .as_ref()
            .is_some_and(|t| crate::pg::timestamp(t).is_none())
    {
        return Err(ApiError::BadRequest(
            "invalid source report timestamp".into(),
        ));
    }
    let result = match st.store.as_ref() {
        Store::Jsonl(s) => s.health_save(&ds, report),
        Store::Postgres(p) => p.health_save(&ds, report).await,
    };
    let saved = result.map_err(|e| {
        if e.downcast_ref::<Conflict>().is_some() {
            ApiError::SourceConflict
        } else if e.downcast_ref::<Capacity>().is_some() {
            ApiError::BadRequest(e.to_string())
        } else {
            internal(e)
        }
    })?;
    Ok(Json(json!({"devspace":ds,"observation":saved})))
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    let mut sources = match st.store.as_ref() {
        Store::Jsonl(s) => s.health_read(&ds),
        Store::Postgres(p) => p.health_read(&ds).await,
    }
    .map_err(internal)?;
    sources.sort_by(|a, b| {
        b.received_at
            .cmp(&a.received_at)
            .then(a.report.source_id.cmp(&b.report.source_id))
    });
    Ok(Json(
        json!({"devspace":ds,"sources":sources,"as_of":chrono::Utc::now().to_rfc3339(),"recent_for_seconds":120}),
    ))
}
