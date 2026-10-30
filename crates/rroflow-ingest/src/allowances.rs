//! Database-owned account allowances. This is the enforcement seam, not billing.
use crate::{ApiError, AppState, PgStore, Store, internal};
use axum::{Json, extract::State, http::HeaderMap};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction, types::Json as SqlJson};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Allowance {
    pub plan: String,
    pub label: String,
    pub turns_per_month: u64,
    pub events_per_month: u64,
}
impl Allowance {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            crate::valid_devspace_id(&self.plan),
            "invalid plan identifier"
        );
        anyhow::ensure!(
            !self.label.trim().is_empty()
                && self.label.len() <= 100
                && !self.label.chars().any(char::is_control),
            "invalid plan label"
        );
        anyhow::ensure!(
            self.turns_per_month <= i64::MAX as u64 && self.events_per_month <= i64::MAX as u64,
            "allowance exceeds supported range"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: String,
    pub revision: u64,
    pub default: Allowance,
}
impl Policy {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == "rroflow.allowance-policy/v1",
            "unsupported allowance policy schema"
        );
        revision(self.revision)?;
        self.default.validate()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub schema: String,
    pub revision: u64,
    pub allowance: Allowance,
    /// Expiry falls back to the current default, without resetting usage.
    pub expires_at: String,
}
impl Grant {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.schema == "rroflow.allowance-grant/v1",
            "unsupported allowance grant schema"
        );
        revision(self.revision)?;
        self.allowance.validate()?;
        anyhow::ensure!(
            crate::pg::timestamp(&self.expires_at).is_some(),
            "expires_at must be RFC3339"
        );
        Ok(())
    }
}
fn revision(value: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        value > 0 && value <= i64::MAX as u64,
        "revision must be a positive signed 64-bit value"
    );
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct QuotaExceeded {
    pub resource: &'static str,
    pub used: i64,
    pub limit: u64,
    pub requested: i64,
    pub resets_at: String,
}
impl std::fmt::Display for QuotaExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("account allowance reached")
    }
}
impl std::error::Error for QuotaExceeded {}

#[derive(Debug)]
struct PolicyUnavailable;
impl std::fmt::Display for PolicyUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("required allowance policy unavailable")
    }
}
impl std::error::Error for PolicyUnavailable {}

pub(crate) fn ingest_error(error: anyhow::Error) -> ApiError {
    if error.is::<PolicyUnavailable>() {
        return ApiError::AllowanceUnavailable;
    }
    match error.downcast::<QuotaExceeded>() {
        Ok(quota) => ApiError::Quota(quota),
        Err(error) => internal(error),
    }
}

fn month(at: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Utc
        .with_ymd_and_hms(at.year(), at.month(), 1, 0, 0, 0)
        .unwrap();
    let (year, next) = if at.month() == 12 {
        (at.year() + 1, 1)
    } else {
        (at.year(), at.month() + 1)
    };
    (start, Utc.with_ymd_and_hms(year, next, 1, 0, 0, 0).unwrap())
}

struct Effective {
    policy: Option<Policy>,
    grant: Option<Grant>,
    at: DateTime<Utc>,
}
impl Effective {
    fn allowance(&self) -> Option<&Allowance> {
        self.policy.as_ref().map(|policy| {
            self.grant
                .as_ref()
                .filter(|g| crate::pg::timestamp(&g.expires_at).is_some_and(|t| t > self.at))
                .map_or(&policy.default, |g| &g.allowance)
        })
    }
}

async fn effective(tx: &mut Transaction<'_, Postgres>, ds: &str) -> anyhow::Result<Effective> {
    // SHARE prevents a policy change halfway through this ingest transaction.
    let policy: Option<SqlJson<Policy>> =
        sqlx::query_scalar("SELECT document FROM allowance_policy WHERE singleton FOR SHARE")
            .fetch_optional(&mut **tx)
            .await?;
    let grant: Option<SqlJson<Grant>> =
        sqlx::query_scalar("SELECT document FROM allowance_grants WHERE devspace_id=$1")
            .bind(ds)
            .fetch_optional(&mut **tx)
            .await?;
    let at = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let policy = policy.map(|p| p.0);
    let grant = grant.map(|g| g.0);
    if let Some(p) = &policy {
        p.validate()?;
    }
    if let Some(g) = &grant {
        g.validate()?;
    }
    Ok(Effective { policy, grant, at })
}

async fn usage(
    tx: &mut Transaction<'_, Postgres>,
    ds: &str,
    kind: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM ingest_identities WHERE devspace_id=$1 AND kind=$2 AND accepted_at >= $3 AND accepted_at < $4")
        .bind(ds).bind(kind).bind(start).bind(end).fetch_one(&mut **tx).await?)
}

/// Caller holds the devspace row lock. Ledger and records commit together.
pub(crate) async fn reserve(
    tx: &mut Transaction<'_, Postgres>,
    ds: &str,
    kind: &'static str,
    ids: &[String],
    required: bool,
) -> anyhow::Result<()> {
    let policy = effective(tx, ds).await?;
    if required && policy.policy.is_none() {
        return Err(PolicyUnavailable.into());
    }
    let new: Vec<String> = sqlx::query_scalar("SELECT DISTINCT id FROM unnest($3::text[]) AS ids(id) WHERE NOT EXISTS (SELECT 1 FROM ingest_identities i WHERE i.devspace_id=$1 AND i.kind=$2 AND i.event_id=ids.id)")
        .bind(ds).bind(kind).bind(ids).fetch_all(&mut **tx).await?;
    let (start, end) = month(policy.at);
    if let Some(allowance) = policy.allowance() {
        let used = usage(tx, ds, kind, start, end).await?;
        let limit = if kind == "turns" {
            allowance.turns_per_month
        } else {
            allowance.events_per_month
        };
        if !new.is_empty() && (new.len() as u64 > limit.saturating_sub(used as u64)) {
            return Err(QuotaExceeded {
                resource: kind,
                used,
                limit,
                requested: new.len() as i64,
                resets_at: end.to_rfc3339(),
            }
            .into());
        }
    }
    sqlx::query("INSERT INTO ingest_identities (devspace_id,kind,event_id,accepted_at) SELECT $1,$2,id,$4 FROM unnest($3::text[]) AS ids(id) ON CONFLICT DO NOTHING")
        .bind(ds).bind(kind).bind(&new).bind(policy.at).execute(&mut **tx).await?;
    Ok(())
}

impl PgStore {
    /// Explicit operator control, with compare-and-swap revisions and identical retry.
    pub async fn set_allowance_policy(&self, policy: &Policy) -> anyhow::Result<()> {
        policy.validate()?;
        let result = sqlx::query("INSERT INTO allowance_policy (singleton,revision,document) SELECT true,$1,$2 WHERE $1=1 OR EXISTS (SELECT 1 FROM allowance_policy) ON CONFLICT (singleton) DO UPDATE SET revision=EXCLUDED.revision, document=EXCLUDED.document WHERE allowance_policy.revision=EXCLUDED.revision-1 OR (allowance_policy.revision=EXCLUDED.revision AND allowance_policy.document=EXCLUDED.document)")
            .bind(policy.revision as i64).bind(SqlJson(policy)).execute(&self.pool).await?;
        anyhow::ensure!(
            result.rows_affected() == 1,
            "stale policy revision: use the next revision"
        );
        Ok(())
    }

    pub async fn set_allowance_grant(&self, ds: &str, grant: &Grant) -> anyhow::Result<()> {
        grant.validate()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR UPDATE")
            .bind(ds)
            .fetch_one(&mut *tx)
            .await?;
        let result = sqlx::query("INSERT INTO allowance_grants (devspace_id,revision,document) SELECT $1,$2,$3 WHERE $2=1 OR EXISTS (SELECT 1 FROM allowance_grants WHERE devspace_id=$1) ON CONFLICT (devspace_id) DO UPDATE SET revision=EXCLUDED.revision, document=EXCLUDED.document WHERE allowance_grants.revision=EXCLUDED.revision-1 OR (allowance_grants.revision=EXCLUDED.revision AND allowance_grants.document=EXCLUDED.document)")
            .bind(ds).bind(grant.revision as i64).bind(SqlJson(grant)).execute(&mut *tx).await?;
        anyhow::ensure!(
            result.rows_affected() == 1,
            "stale grant revision: use the next revision"
        );
        tx.commit().await?;
        Ok(())
    }

    pub async fn allowance_status(&self, ds: &str) -> anyhow::Result<serde_json::Value> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR UPDATE")
            .bind(ds)
            .fetch_one(&mut *tx)
            .await?;
        let effective = effective(&mut tx, ds).await?;
        let (start, end) = month(effective.at);
        let turns = usage(&mut tx, ds, "turns", start, end).await?;
        let events = usage(&mut tx, ds, "events", start, end).await?;
        let result = serde_json::json!({
            "schema": "rroflow.allowance-status/v1", "devspace": ds,
            "mode": if effective.policy.is_some() { "enforced" } else if self.require_allowance_policy { "unavailable" } else { "unconfigured" },
            "allowance": effective.allowance(), "policy_revision": effective.policy.as_ref().map(|p|p.revision),
            "grant_revision": effective.grant.as_ref().map(|g|g.revision),
            "grant_expires_at": effective.grant.as_ref().map(|g|&g.expires_at),
            "grant_active": effective.policy.is_some() && effective.grant.as_ref().is_some_and(|g| crate::pg::timestamp(&g.expires_at).is_some_and(|t|t>effective.at)),
            "period_start": start.to_rfc3339(), "resets_at": end.to_rfc3339(),
            "used": { "turns": turns, "events": events }, "as_of": effective.at.to_rfc3339(),
            "billing_connected": false
        });
        tx.commit().await?;
        Ok(result)
    }
}

pub(crate) async fn status(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    let result = match st.store.as_ref() {
        Store::Postgres(pg) => pg.allowance_status(&ds).await.map_err(internal)?,
        Store::Jsonl(_) => {
            serde_json::json!({"schema":"rroflow.allowance-status/v1","devspace":ds,"mode":"local_unmetered","billing_connected":false})
        }
    };
    Ok(Json(result))
}

#[cfg(test)]
mod tests;
