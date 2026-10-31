//! Consented metadata datasets. Collection grants no training or publication rights.
mod projection;
#[cfg(test)]
mod tests;
mod worker;
pub use worker::run_once;

use crate::{ApiError, AppState, Caller, PgStore, Store, internal};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::types::Json as SqlJson;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatasetConfig {
    pub max_rows: usize,
    pub max_snapshot_bytes: usize,
    pub max_retained_per_purpose: i64,
    pub refresh_seconds: u64,
    pub poll_seconds: u64,
    pub lease_seconds: u64,
    pub max_attempts: i32,
    pub job_history_days: i32,
}
impl Default for DatasetConfig {
    fn default() -> Self {
        Self {
            max_rows: 1000,
            max_snapshot_bytes: 2 * 1024 * 1024,
            max_retained_per_purpose: 10,
            refresh_seconds: 300,
            poll_seconds: 5,
            lease_seconds: 60,
            max_attempts: 3,
            job_history_days: 90,
        }
    }
}
impl DatasetConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=10000).contains(&self.max_rows)
                && (1024..=16 * 1024 * 1024).contains(&self.max_snapshot_bytes),
            "invalid dataset snapshot bounds"
        );
        anyhow::ensure!(
            (1..=100).contains(&self.max_retained_per_purpose)
                && (1..=86400).contains(&self.refresh_seconds)
                && (1..=60).contains(&self.poll_seconds),
            "invalid dataset retention or scan bounds"
        );
        anyhow::ensure!(
            (5..=3600).contains(&self.lease_seconds) && (1..=10).contains(&self.max_attempts),
            "invalid dataset lease bounds"
        );
        anyhow::ensure!(
            (1..=3650).contains(&self.job_history_days),
            "invalid dataset job-history bound"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    SignalReviews,
    Patterns,
}
impl Purpose {
    fn name(&self) -> &'static str {
        match self {
            Self::SignalReviews => "signal_reviews",
            Self::Patterns => "patterns",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub purpose: Purpose,
    pub enabled: bool,
    pub consent: bool,
    pub agents: Vec<String>,
    pub all_projects: bool,
    pub projects: Vec<String>,
    pub retention_days: u32,
}
impl Policy {
    fn validate(&self) -> Result<(), String> {
        if self.enabled != self.consent {
            return Err("Enabling collection requires explicit dataset consent; stopping it withdraws consent.".into());
        }
        if !(1..=90).contains(&self.retention_days)
            || self.agents.len() > 32
            || self.projects.len() > 64
        {
            return Err("Dataset policy exceeds supported bounds.".into());
        }
        if self.agents.iter().any(|s| !crate::valid_devspace_id(s))
            || self
                .projects
                .iter()
                .any(|s| s.is_empty() || s.len() > 1024 || s.chars().any(char::is_control))
        {
            return Err("Invalid source or project.".into());
        }
        if self.enabled
            && (self.agents.is_empty() || (!self.all_projects && self.projects.is_empty()))
        {
            return Err(
                "Choose at least one source and project, or explicitly choose all projects.".into(),
            );
        }
        if self.all_projects && !self.projects.is_empty() {
            return Err("Choose all projects or a project list, not both.".into());
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    expected_revision: u64,
    policy: Policy,
}
pub(crate) fn digest(value: &impl Serialize) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("serialize dataset value"),
    ))
}
fn reference(ds: &str, kind: &str, id: &str) -> String {
    digest(&(ds, kind, id))
}
fn member(caller: &Caller) -> Result<(), ApiError> {
    if caller.via == "nomen" {
        Ok(())
    } else {
        Err(ApiError::MemberRequired)
    }
}
fn configured(st: &AppState) -> Result<(&PgStore, &DatasetConfig), ApiError> {
    match (st.store.as_ref(), st.datasets.as_ref()) {
        (Store::Postgres(pg), Some(cfg)) => Ok((pg, cfg)),
        _ => Err(ApiError::BadRequest(
            "Dataset collection is not enabled on this service.".into(),
        )),
    }
}

impl PgStore {
    async fn save_dataset_policy(
        &self,
        ds: &str,
        actor: &str,
        request: &Update,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        // A row exists before locking it; concurrent first writes still compare against zero.
        sqlx::query("INSERT INTO dataset_policies(devspace_id,purpose,revision,enabled,document,actor) VALUES($1,$2,1,false,$3,$4) ON CONFLICT DO NOTHING")
            .bind(ds).bind(request.policy.purpose.name()).bind(SqlJson(&request.policy)).bind(actor).execute(&mut *tx).await?;
        let current:i64=sqlx::query_scalar("SELECT CASE WHEN EXISTS(SELECT 1 FROM dataset_consent_history h WHERE h.devspace_id=p.devspace_id AND h.purpose=p.purpose) THEN revision ELSE 0 END FROM dataset_policies p WHERE devspace_id=$1 AND purpose=$2 FOR UPDATE")
            .bind(ds).bind(request.policy.purpose.name()).fetch_one(&mut *tx).await?;
        if current as u64 != request.expected_revision {
            return Ok(false);
        }
        let revision = current + 1;
        sqlx::query("UPDATE dataset_policies SET revision=$3,enabled=$4,document=$5,actor=$6,updated_at=clock_timestamp(),next_scan_at=clock_timestamp(),last_input_digest=NULL,last_source_count=NULL,last_error=NULL,last_scan_at=NULL,last_exclusions='{}'::jsonb WHERE devspace_id=$1 AND purpose=$2")
            .bind(ds).bind(request.policy.purpose.name()).bind(revision).bind(request.policy.enabled).bind(SqlJson(&request.policy)).bind(actor).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO dataset_consent_history(devspace_id,purpose,revision,document,actor) VALUES($1,$2,$3,$4,$5)")
            .bind(ds).bind(request.policy.purpose.name()).bind(revision).bind(SqlJson(&request.policy)).bind(actor).execute(&mut *tx).await?;
        // Scope changes and withdrawal invalidate every artifact under the old consent.
        sqlx::query("UPDATE dataset_jobs SET status='revoked',input=NULL,artifact=NULL,lease_token=NULL,lease_until=NULL WHERE devspace_id=$1 AND purpose=$2 AND status NOT IN ('revoked','expired')")
            .bind(ds).bind(request.policy.purpose.name()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}

pub(crate) async fn status(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let caller = st.caller(&headers).await?;
    let Ok((pg, cfg)) = configured(&st) else {
        return Ok(Json(json!({"mode":"disabled","training_enabled":false})));
    };
    if caller.via != "nomen" {
        return Ok(Json(
            json!({"mode":"member_signin_required","training_enabled":false}),
        ));
    }
    let policies:Vec<SqlJson<Value>>=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'policy',document,'last_source_count',last_source_count,'last_error',last_error,'last_scan_at',last_scan_at,'exclusions',last_exclusions,'next_scan_at',next_scan_at) FROM dataset_policies WHERE devspace_id=$1 ORDER BY purpose")
        .bind(&caller.devspace).fetch_all(&pg.pool).await.map_err(|e| internal(e.into()))?;
    let jobs:Vec<SqlJson<Value>>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'purpose',purpose,'policy_revision',policy_revision,'status',CASE WHEN expires_at<=clock_timestamp() AND status NOT IN ('revoked','failed') THEN 'expired' ELSE status END,'rows',row_count,'exclusions',exclusions,'created_at',created_at,'expires_at',expires_at,'artifact_digest',artifact_digest,'error_code',error_code) FROM dataset_jobs WHERE devspace_id=$1 ORDER BY created_at DESC,id LIMIT 50")
        .bind(&caller.devspace).fetch_all(&pg.pool).await.map_err(|e| internal(e.into()))?;
    Ok(Json(
        json!({"mode":"available","devspace":caller.devspace,"policies":policies.into_iter().map(|x|x.0).collect::<Vec<_>>(),"jobs":jobs.into_iter().map(|x|x.0).collect::<Vec<_>>(),"limits":cfg,"training_enabled":false}),
    ))
}
pub(crate) async fn save(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Update>,
) -> Result<Json<Value>, ApiError> {
    let caller = st.caller(&headers).await?;
    member(&caller)?;
    let (pg, _) = configured(&st)?;
    request.policy.validate().map_err(ApiError::BadRequest)?;
    if request.expected_revision >= i64::MAX as u64 {
        return Err(ApiError::BadRequest("Invalid consent revision.".into()));
    }
    if !pg
        .save_dataset_policy(
            &caller.devspace,
            caller.subject.as_deref().expect("member subject"),
            &request,
        )
        .await
        .map_err(internal)?
    {
        return Err(ApiError::DatasetPolicyConflict);
    }
    Ok(Json(
        json!({"saved":true,"revision":request.expected_revision+1,"training_enabled":false}),
    ))
}
pub(crate) async fn artifact(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let caller = st.caller(&headers).await?;
    member(&caller)?;
    let (pg, _) = configured(&st)?;
    // Check current consent and retention on every download, including when the worker is down.
    let value:Option<(SqlJson<Value>,String)>=sqlx::query_as("SELECT j.artifact,j.artifact_digest FROM dataset_jobs j JOIN dataset_policies p ON p.devspace_id=j.devspace_id AND p.purpose=j.purpose AND p.revision=j.policy_revision WHERE j.devspace_id=$1 AND j.id=$2 AND j.status='published' AND j.expires_at>clock_timestamp() AND p.enabled")
        .bind(&caller.devspace).bind(id).fetch_optional(&pg.pool).await.map_err(|e| internal(e.into()))?;
    let Some((artifact, hash)) = value else {
        return Err(ApiError::NotFound);
    };
    if digest(&artifact.0) != hash {
        return Err(ApiError::Internal);
    }
    Ok(Json(json!({"artifact":artifact.0,"sha256":hash})))
}
