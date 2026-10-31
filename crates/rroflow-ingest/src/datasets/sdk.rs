//! SDK dataset projections contain structural metrics, never private event payloads.
use super::*;
use rroflow_core::{Usage, activity::Outcome};
use std::collections::BTreeMap;

const QUERY: &str = r#"
SELECT jsonb_build_object(
 'id',left(q.id,81),'activity_id',left(q.s->>'activity_id',81),'revision',left(q.revision,65),
 'parent',left(q.s->>'parent_activity_id',81),'agent',left(q.s->>'agent',65),
 'signal',left(q.signal,129),'rule_version',q.rule_version,'reviewer',left(q.reviewer,1025),'verdict',left(q.verdict,32),
 'features',jsonb_build_object(
   'projection_version',q.s->'projection_version','kind',q.s->'kind',
   'outcome',q.s->'outcome','outcome_basis',left(q.s->>'outcome_basis',129),
   'provider_outcome',q.s->'provider_outcome','usage',q.s->'usage',
   'event_count',jsonb_array_length(q.s->'events'),
   'unsupported_event_count',jsonb_array_length(q.s->'unsupported_event_ids'),
   'tool_calls',jsonb_array_length(q.s->'tools'),
   'tool_errors',(SELECT count(*) FROM jsonb_array_elements(q.s->'tools') t WHERE t->>'outcome'='failed')
 ))
FROM (
 SELECT a.activity_id AS id,a.record AS s,a.revision,NULL::text AS signal,
 NULL::bigint AS rule_version,NULL::text AS reviewer,NULL::text AS verdict
 FROM sdk_activities a JOIN sdk_activity_work w USING(devspace_id,capture_id)
 WHERE a.devspace_id=$1 AND $2='patterns' AND w.status='ready' AND a.source_revision=w.source_revision
 AND NOT EXISTS(SELECT 1 FROM sdk_activity_reviews r WHERE r.devspace_id=$1 AND r.activity_id=a.activity_id)
 UNION ALL
 SELECT review_id,record->'snapshot',record#>>'{request,revision}',record#>>'{request,signal}',
 (record#>>'{request,rule_version}')::bigint,record->>'reviewer',record#>>'{request,verdict}'
 FROM sdk_activity_reviews WHERE devspace_id=$1 AND $2='signal_reviews'
) q WHERE q.s->>'agent'=ANY($3) ORDER BY q.id LIMIT $4
"#;

#[derive(Deserialize, Serialize)]
struct Features {
    projection_version: u32,
    kind: String,
    outcome: Outcome,
    outcome_basis: String,
    provider_outcome: Option<Outcome>,
    usage: Option<Usage>,
    event_count: u64,
    unsupported_event_count: u64,
    tool_calls: u64,
    tool_errors: u64,
}
#[derive(Deserialize)]
struct Source {
    id: String,
    activity_id: String,
    revision: String,
    parent: Option<String>,
    agent: String,
    signal: Option<String>,
    rule_version: Option<u32>,
    reviewer: Option<String>,
    verdict: Option<String>,
    features: Features,
}
pub(super) async fn snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ds: &str,
    policy: &Policy,
    cfg: &DatasetConfig,
) -> anyhow::Result<(Vec<Value>, BTreeMap<String, usize>, usize)> {
    let rows: Vec<SqlJson<Value>> = sqlx::query_scalar(QUERY)
        .bind(ds)
        .bind(policy.purpose.name())
        .bind(&policy.sdk_agents)
        .bind(cfg.max_rows as i64 + 1)
        .fetch_all(&mut **tx)
        .await?;
    let count = rows.len();
    anyhow::ensure!(count <= cfg.max_rows, "row_capacity");
    let mut exclusions = BTreeMap::new();
    let mut output = Vec::new();
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for raw in rows {
        let Ok(s) = serde_json::from_value::<Source>(raw.0) else {
            *exclusions.entry("invalid_sdk_source".into()).or_default() += 1;
            continue;
        };
        let f = &s.features;
        if s.revision.len() != 64
            || !s.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || s.activity_id.len() > 80
            || s.id.len() > 80
            || s.parent.as_ref().is_some_and(|p| p.len() > 80)
            || !["claude", "codex", "grok"].contains(&s.agent.as_str())
            || f.projection_version != 1
            || !["call", "tool", "observation"].contains(&f.kind.as_str())
            || f.outcome_basis.len() > 128
            || f.tool_errors > f.tool_calls
            || f.unsupported_event_count > f.event_count
        {
            *exclusions.entry("invalid_sdk_source".into()).or_default() += 1;
            continue;
        }
        let activity_ref = reference(ds, "sdk_activity", &s.activity_id);
        let features = serde_json::to_value(f)?;
        let mut row = json!({"source_kind":"sdk_operation","activity_ref":activity_ref,
            "source_activity_digest":s.revision,"parent_ref":s.parent.map(|p|reference(ds,"sdk_activity",&p)),
            "projection_digest":digest(&(&activity_ref,&s.agent,&features)),"agent":s.agent,"features":features,"label":null});
        if policy.purpose == Purpose::SignalReviews {
            if !matches!(s.verdict.as_deref(), Some("match" | "not_match"))
                || s.rule_version.is_none()
                || s.signal
                    .as_ref()
                    .is_none_or(|s| s.is_empty() || s.len() > 128)
                || s.reviewer
                    .as_ref()
                    .is_none_or(|s| s.is_empty() || s.len() > 1024)
            {
                *exclusions.entry("invalid_sdk_review".into()).or_default() += 1;
                continue;
            }
            row["label"] = json!({"signal_ref":reference(ds,"sdk_signal",s.signal.as_deref().unwrap()),
                "rule_version":s.rule_version,"verdict":s.verdict,
                "review_refs":[reference(ds,"sdk_review",&s.id)],
                "reviewer_refs":[reference(ds,"reviewer",s.reviewer.as_deref().unwrap())]});
            let group = digest(&(
                &row["activity_ref"],
                &row["source_activity_digest"],
                &row["label"]["signal_ref"],
                &row["label"]["rule_version"],
            ));
            groups.entry(group).or_default().push(row);
        } else {
            output.push(row);
        }
    }
    for rows in groups.into_values() {
        if rows
            .iter()
            .any(|r| r["label"]["verdict"] != rows[0]["label"]["verdict"])
        {
            *exclusions
                .entry("conflicting_sdk_reviews".into())
                .or_default() += rows.len();
            continue;
        }
        let mut first = rows[0].clone();
        for field in ["review_refs", "reviewer_refs"] {
            let mut refs: Vec<_> = rows
                .iter()
                .flat_map(|r| r["label"][field].as_array().unwrap())
                .map(|v| v.as_str().unwrap())
                .collect();
            refs.sort();
            refs.dedup();
            first["label"][field] = json!(refs);
        }
        *exclusions
            .entry("duplicate_sdk_reviews".into())
            .or_default() += rows.len() - 1;
        output.push(first);
    }
    Ok((output, exclusions, count))
}
