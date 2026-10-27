//! Postgres store for the hosted rroflow.ai ingest. Schema: `migrations/` (embedded and
//! applied on start). A batch is written in one transaction with one statement per table
//! (`UNNEST` over column arrays), so cost per request does not grow with round trips.

use crate::store::{Summary, TurnFilter, Upserted, last_per_event};
use chrono::{DateTime, Utc};
use rroflow_core::signals::SignalConfig;
use rroflow_core::{ToolStatus, TurnRecord};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::types::Json;
use std::collections::BTreeMap;
use std::time::Duration;

pub struct PgStore {
    pub(crate) pool: PgPool,
}

impl PgStore {
    pub(crate) async fn capture_insert(
        &self,
        ds: &str,
        events: &[rroflow_core::capture::CaptureEvent],
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM devspaces WHERE id = $1 FOR UPDATE")
            .bind(ds)
            .fetch_one(&mut *tx)
            .await?;
        for e in events {
            sqlx::query("INSERT INTO capture_events (devspace_id,event_id,capture_id,sequence,agent,observed_at,record) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
                .bind(ds).bind(&e.event_id).bind(&e.capture_id).bind(e.sequence as i64).bind(&e.agent)
                .bind(timestamp(&e.observed_at)).bind(Json(e)).execute(&mut *tx).await?;
            let stored: Option<Json<rroflow_core::capture::CaptureEvent>> = sqlx::query_scalar(
                "SELECT record FROM capture_events WHERE devspace_id=$1 AND event_id=$2",
            )
            .bind(ds)
            .bind(&e.event_id)
            .fetch_optional(&mut *tx)
            .await?;
            if stored.as_ref().map(|s| &s.0) != Some(e) {
                return Err(crate::capture::Conflict.into());
            }
        }
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn capture_list(
        &self,
        ds: &str,
        agent: Option<&str>,
        capture: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<rroflow_core::capture::CaptureEvent>> {
        let rows: Vec<Json<rroflow_core::capture::CaptureEvent>> = sqlx::query_scalar("SELECT record FROM capture_events WHERE devspace_id=$1 AND ($2::text IS NULL OR agent=$2) AND ($3::text IS NULL OR capture_id=$3) ORDER BY observed_at DESC, capture_id, sequence DESC LIMIT $4")
            .bind(ds).bind(agent).bind(capture).bind(limit as i64).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}

/// Parse an RFC 3339 timestamp; empty or unparseable is `None` (the handler rejects
/// unparseable values before they get here).
pub(crate) fn timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Devspace id for a Nomen account: the account ID itself when it is a valid id (Nomen's
/// are numeric), else `n-` and a digest of issuer and ID.
pub(crate) fn owner_devspace_id(issuer: &str, subject: &str) -> String {
    if crate::valid_devspace_id(subject) {
        subject.to_string()
    } else {
        use sha2::{Digest, Sha256};
        let d = Sha256::digest(format!("{issuer}\n{subject}").as_bytes());
        format!("n-{}", &hex::encode(d)[..40])
    }
}

fn tokens(v: Option<u64>) -> Option<i64> {
    v.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
}

fn count(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

impl PgStore {
    /// Connect and bring the schema up to date.
    pub async fn connect(url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn ensure_devspaces(&self, ids: &[String]) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO devspaces (id) SELECT * FROM UNNEST($1::text[]) ON CONFLICT DO NOTHING",
        )
        .bind(ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The devspace owned by `(issuer, subject)`; created on first sign-in with the account
    /// ID as its id (or a digest of it when the ID has other characters). The account's
    /// org is kept current.
    pub async fn devspace_for_owner(
        &self,
        issuer: &str,
        subject: &crate::Subject,
    ) -> anyhow::Result<String> {
        let found: Option<String> = sqlx::query_scalar(
            r#"
            UPDATE devspaces SET owner_org = $3
            WHERE owner_issuer = $1 AND owner_subject = $2
            RETURNING id
            "#,
        )
        .bind(issuer)
        .bind(&subject.id)
        .bind(&subject.org)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(id) = found {
            return Ok(id);
        }
        let id = owner_devspace_id(issuer, &subject.id);
        let created: Option<String> = sqlx::query_scalar(
            r#"
            INSERT INTO devspaces (id, owner_issuer, owner_subject, owner_org)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT DO NOTHING
            RETURNING id
            "#,
        )
        .bind(&id)
        .bind(issuer)
        .bind(&subject.id)
        .bind(&subject.org)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(id) = created {
            return Ok(id);
        }
        // Either a concurrent first sign-in created it, or the id is someone else's.
        let raced: Option<String> = sqlx::query_scalar(
            "SELECT id FROM devspaces WHERE owner_issuer = $1 AND owner_subject = $2",
        )
        .bind(issuer)
        .bind(&subject.id)
        .fetch_optional(&self.pool)
        .await?;
        raced.ok_or_else(|| {
            anyhow::anyhow!(
                "devspace id {id} already exists without this owner; attach it with `rroflow-ingest claim`"
            )
        })
    }

    /// Make an existing devspace (one registered by ingest key) owned by a Nomen account, so
    /// that account's sign-in opens it. Refuses when the devspace has another owner, or the
    /// account already owns a devspace with turns in it (an empty one is removed).
    pub async fn claim_devspace(
        &self,
        devspace: &str,
        issuer: &str,
        subject: &str,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let owner: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT owner_issuer, owner_subject FROM devspaces WHERE id = $1 FOR UPDATE",
        )
        .bind(devspace)
        .fetch_optional(&mut *tx)
        .await?;
        match owner {
            None => anyhow::bail!("no devspace {devspace}"),
            Some((Some(i), Some(s))) if i == issuer && s == subject => return Ok(()),
            Some((Some(_), Some(_))) => {
                anyhow::bail!("devspace {devspace} is owned by another account")
            }
            Some(_) => {}
        }
        let other: Option<String> = sqlx::query_scalar(
            "SELECT id FROM devspaces WHERE owner_issuer = $1 AND owner_subject = $2",
        )
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(other) = other {
            // A devspace made by signing in before claiming is empty: it gives way. One
            // that holds turns does not; merging data is not done here.
            let turns: i64 =
                sqlx::query_scalar("SELECT (SELECT count(*) FROM turns WHERE devspace_id = $1) + (SELECT count(*) FROM capture_events WHERE devspace_id = $1) + (SELECT count(*) FROM signal_reviews WHERE devspace_id = $1)")
                    .bind(&other)
                    .fetch_one(&mut *tx)
                    .await?;
            anyhow::ensure!(
                turns == 0,
                "account {subject} already owns devspace {other} with {turns} turns; one devspace per account"
            );
            sqlx::query("DELETE FROM devspaces WHERE id = $1")
                .bind(&other)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE devspaces SET owner_issuer = $2, owner_subject = $3 WHERE id = $1")
            .bind(devspace)
            .bind(issuer)
            .bind(subject)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert(&self, devspace: &str, recs: &[TurnRecord]) -> anyhow::Result<Upserted> {
        let recs = last_per_event(recs);
        if recs.is_empty() {
            return Ok(Upserted::default());
        }
        let n = recs.len();
        let mut event_id = Vec::with_capacity(n);
        let mut agent = Vec::with_capacity(n);
        let mut provider = Vec::with_capacity(n);
        let mut model = Vec::with_capacity(n);
        let mut project = Vec::with_capacity(n);
        let mut session_id = Vec::with_capacity(n);
        let mut started_at = Vec::with_capacity(n);
        let mut ended_at = Vec::with_capacity(n);
        let mut termination = Vec::with_capacity(n);
        let mut input_tokens = Vec::with_capacity(n);
        let mut output_tokens = Vec::with_capacity(n);
        let mut cache_read = Vec::with_capacity(n);
        let mut cache_write = Vec::with_capacity(n);
        let mut tool_calls = Vec::with_capacity(n);
        let mut tool_errors = Vec::with_capacity(n);
        let mut has_content = Vec::with_capacity(n);
        let mut record = Vec::with_capacity(n);
        // Signal hits, flattened.
        let (mut h_event, mut h_ord, mut h_signal, mut h_ver) = (vec![], vec![], vec![], vec![]);
        let (mut h_sev, mut h_score, mut h_summary) = (vec![], vec![], vec![]);
        for r in &recs {
            event_id.push(r.event_id.clone());
            agent.push(r.agent.clone());
            provider.push(r.provider.clone());
            model.push(r.model.clone());
            project.push(r.project.clone());
            session_id.push(r.session_id.clone());
            started_at.push(timestamp(&r.started_at));
            ended_at.push(timestamp(&r.ended_at));
            termination.push(
                serde_json::to_value(r.termination)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
            );
            input_tokens.push(tokens(r.usage.input_tokens));
            output_tokens.push(tokens(r.usage.output_tokens));
            cache_read.push(tokens(r.usage.cache_read_tokens));
            cache_write.push(tokens(r.usage.cache_write_tokens));
            tool_calls.push(i32::try_from(r.tools.len()).unwrap_or(i32::MAX));
            tool_errors.push(
                i32::try_from(
                    r.tools
                        .iter()
                        .filter(|t| t.status == ToolStatus::Error)
                        .count(),
                )
                .unwrap_or(i32::MAX),
            );
            has_content.push(r.content.is_some());
            record.push(serde_json::to_value(r)?);
            for (i, h) in r.signals.iter().enumerate() {
                h_event.push(r.event_id.clone());
                h_ord.push(i32::try_from(i).unwrap_or(i32::MAX));
                h_signal.push(h.signal.clone());
                h_ver.push(i32::try_from(h.rule_version).unwrap_or(i32::MAX));
                h_sev.push(
                    serde_json::to_value(h.severity)?
                        .as_str()
                        .unwrap_or("info")
                        .to_string(),
                );
                h_score.push(h.score);
                h_summary.push(h.summary.clone());
            }
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO devspaces (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(devspace)
            .execute(&mut *tx)
            .await?;
        let inserted: Vec<bool> = sqlx::query_scalar(
            r#"
            INSERT INTO turns (
                devspace_id, event_id, agent, provider, model, project, session_id,
                started_at, ended_at, termination,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                tool_calls, tool_errors, has_content, record)
            SELECT $1::text, * FROM UNNEST(
                $2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[],
                $8::timestamptz[], $9::timestamptz[], $10::text[],
                $11::int8[], $12::int8[], $13::int8[], $14::int8[],
                $15::int4[], $16::int4[], $17::bool[], $18::jsonb[])
            ON CONFLICT (devspace_id, event_id) DO UPDATE SET
                agent = EXCLUDED.agent, provider = EXCLUDED.provider, model = EXCLUDED.model,
                project = EXCLUDED.project, session_id = EXCLUDED.session_id,
                started_at = EXCLUDED.started_at, ended_at = EXCLUDED.ended_at,
                termination = EXCLUDED.termination,
                input_tokens = EXCLUDED.input_tokens, output_tokens = EXCLUDED.output_tokens,
                cache_read_tokens = EXCLUDED.cache_read_tokens,
                cache_write_tokens = EXCLUDED.cache_write_tokens,
                tool_calls = EXCLUDED.tool_calls, tool_errors = EXCLUDED.tool_errors,
                has_content = EXCLUDED.has_content, record = EXCLUDED.record,
                received_at = now()
            RETURNING (xmax = 0)
            "#,
        )
        .bind(devspace)
        .bind(&event_id)
        .bind(&agent)
        .bind(&provider)
        .bind(&model)
        .bind(&project)
        .bind(&session_id)
        .bind(&started_at)
        .bind(&ended_at)
        .bind(&termination)
        .bind(&input_tokens)
        .bind(&output_tokens)
        .bind(&cache_read)
        .bind(&cache_write)
        .bind(&tool_calls)
        .bind(&tool_errors)
        .bind(&has_content)
        .bind(&record)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM signal_hits WHERE devspace_id = $1 AND event_id = ANY($2)")
            .bind(devspace)
            .bind(&event_id)
            .execute(&mut *tx)
            .await?;
        if !h_event.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO signal_hits
                    (devspace_id, event_id, ordinal, signal, rule_version, severity, score, summary)
                SELECT $1::text, * FROM UNNEST(
                    $2::text[], $3::int4[], $4::text[], $5::int4[], $6::text[], $7::float8[], $8::text[])
                "#,
            )
            .bind(devspace)
            .bind(&h_event)
            .bind(&h_ord)
            .bind(&h_signal)
            .bind(&h_ver)
            .bind(&h_sev)
            .bind(&h_score)
            .bind(&h_summary)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        let new = inserted.iter().filter(|b| **b).count();
        Ok(Upserted {
            new,
            replaced: inserted.len() - new,
        })
    }

    pub async fn list(
        &self,
        devspace: &str,
        limit: usize,
        f: &TurnFilter<'_>,
    ) -> anyhow::Result<Vec<TurnRecord>> {
        let rows: Vec<Json<TurnRecord>> = sqlx::query_scalar(
            r#"
            SELECT t.record FROM turns t
            WHERE t.devspace_id = $1
              AND ($4::text IS NULL OR t.agent = $4)
              AND ($2::text IS NULL OR EXISTS (
                    SELECT 1 FROM signal_hits h
                    WHERE h.devspace_id = t.devspace_id AND h.event_id = t.event_id
                      AND h.signal = $2))
            ORDER BY t.started_at DESC NULLS LAST, t.event_id
            LIMIT $3
            "#,
        )
        .bind(devspace)
        .bind(f.signal)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(f.agent)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|j| j.0).collect())
    }

    pub async fn summary(&self, devspace: &str) -> anyhow::Result<Summary> {
        let (turns, tool_calls, tool_errors, input, output, cache_read, unreported): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            r#"
            SELECT
                count(*),
                coalesce(sum(tool_calls), 0)::int8,
                coalesce(sum(tool_errors), 0)::int8,
                coalesce(sum(input_tokens) FILTER (WHERE input_tokens IS NOT NULL AND output_tokens IS NOT NULL), 0)::int8,
                coalesce(sum(output_tokens) FILTER (WHERE input_tokens IS NOT NULL AND output_tokens IS NOT NULL), 0)::int8,
                coalesce(sum(cache_read_tokens) FILTER (WHERE input_tokens IS NOT NULL AND output_tokens IS NOT NULL), 0)::int8,
                count(*) FILTER (WHERE input_tokens IS NULL OR output_tokens IS NULL)
            FROM turns WHERE devspace_id = $1
            "#,
        )
        .bind(devspace)
        .fetch_one(&self.pool)
        .await?;
        let grouped = |sql: &'static str| {
            let pool = self.pool.clone();
            async move {
                let rows: Vec<(String, i64)> =
                    sqlx::query_as(sql).bind(devspace).fetch_all(&pool).await?;
                anyhow::Ok(
                    rows.into_iter()
                        .map(|(k, n)| (k, count(n)))
                        .collect::<BTreeMap<_, _>>(),
                )
            }
        };
        let (by_agent, by_project, signals) = tokio::try_join!(
            grouped("SELECT agent, count(*) FROM turns WHERE devspace_id = $1 GROUP BY 1"),
            grouped(
                "SELECT coalesce(project, '(none)'), count(*) FROM turns WHERE devspace_id = $1 GROUP BY 1"
            ),
            grouped("SELECT signal, count(*) FROM signal_hits WHERE devspace_id = $1 GROUP BY 1"),
        )?;
        let by_agent_signal: Vec<(String, String, i64)> = sqlx::query_as(
            r#"
            SELECT t.agent, h.signal, count(*)
            FROM signal_hits h JOIN turns t USING (devspace_id, event_id)
            WHERE h.devspace_id = $1
            GROUP BY 1, 2
            "#,
        )
        .bind(devspace)
        .fetch_all(&self.pool)
        .await?;
        let mut signals_by_agent: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for (agent, signal, n) in by_agent_signal {
            signals_by_agent
                .entry(agent)
                .or_default()
                .insert(signal, count(n));
        }
        Ok(Summary {
            turns: count(turns),
            signals_by_agent,
            by_agent,
            by_project,
            signals,
            tool_calls: count(tool_calls),
            tool_errors: count(tool_errors),
            input_tokens: u64::try_from(input).unwrap_or(0),
            output_tokens: u64::try_from(output).unwrap_or(0),
            cache_read_tokens: u64::try_from(cache_read).unwrap_or(0),
            turns_without_usage: count(unreported),
        })
    }

    pub async fn signal_config(&self, devspace: &str) -> anyhow::Result<SignalConfig> {
        let row: Option<Json<SignalConfig>> =
            sqlx::query_scalar("SELECT config FROM signal_settings WHERE devspace_id = $1")
                .bind(devspace)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|j| j.0).unwrap_or_default())
    }

    pub async fn set_signal_config(
        &self,
        devspace: &str,
        cfg: &SignalConfig,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO devspaces (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(devspace)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"
            INSERT INTO signal_settings (devspace_id, config) VALUES ($1, $2)
            ON CONFLICT (devspace_id) DO UPDATE SET config = EXCLUDED.config, updated_at = now()
            "#,
        )
        .bind(devspace)
        .bind(Json(cfg))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
