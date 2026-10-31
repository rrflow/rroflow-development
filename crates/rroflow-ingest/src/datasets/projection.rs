use super::*;
use std::collections::BTreeMap;

// Project inside Postgres: raw prompts, tool arguments/results, paths, model names
// and signal excerpts never enter a job snapshot. Bound strings before transport.
const PROJECTION: &str = r#"
SELECT jsonb_build_object(
 'source_id',left(q.id,1025),'event_id',left(q.event_id,1025),
 'source_digest',q.source_digest,'reviewer',left(q.reviewer,1025),
 'signal',left(q.signal,1025),'rule_version',q.rule_version,'verdict',q.verdict,
 'agent',left(q.s->>'agent',65),
 'features',jsonb_build_object(
   'usage',q.s->'usage','termination',q.s->'termination','compacted',q.s->'compacted',
   'tools_unfinished',q.s->'tools_unfinished',
   'tool_calls',jsonb_array_length(q.s->'tools'),
   'tool_errors',(SELECT count(*) FROM jsonb_array_elements(q.s->'tools') t WHERE t->>'status'='error')
 ))
FROM (
 SELECT event_id AS id,event_id,record AS s,NULL::text AS source_digest,NULL::text AS reviewer,
 NULL::text AS signal,NULL::bigint AS rule_version,NULL::text AS verdict
 FROM turns WHERE devspace_id=$1 AND $2='patterns'
 AND NOT EXISTS(SELECT 1 FROM signal_reviews r WHERE r.devspace_id=$1 AND r.record#>>'{request,event_id}'=turns.event_id)
 UNION ALL
 SELECT review_id,record#>>'{request,event_id}',record->'snapshot',record#>>'{request,turn_digest}',
 record->>'reviewer',record#>>'{request,signal}',(record#>>'{request,rule_version}')::bigint,record#>>'{request,verdict}'
 FROM signal_reviews WHERE devspace_id=$1 AND $2='signal_reviews'
) q
WHERE q.s->>'agent'=ANY($3)
AND ($4 OR coalesce(nullif(q.s->>'project',''),nullif(q.s->>'cwd',''),'(none)')=ANY($5))
ORDER BY q.id LIMIT $6
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Features {
    usage: rroflow_core::Usage,
    termination: rroflow_core::Termination,
    compacted: bool,
    tools_unfinished: u32,
    tool_calls: u64,
    tool_errors: u64,
}
#[derive(Deserialize)]
struct Source {
    source_id: String,
    event_id: String,
    source_digest: Option<String>,
    reviewer: Option<String>,
    signal: Option<String>,
    rule_version: Option<u32>,
    verdict: Option<String>,
    agent: String,
    features: Features,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Input {
    pub schema: String,
    pub purpose: Purpose,
    pub rows: Vec<Value>,
    pub exclusions: BTreeMap<String, usize>,
}

pub(super) async fn snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ds: &str,
    policy: &Policy,
    cfg: &DatasetConfig,
) -> anyhow::Result<Input> {
    let rows: Vec<SqlJson<Value>> = sqlx::query_scalar(PROJECTION)
        .bind(ds)
        .bind(policy.purpose.name())
        .bind(&policy.agents)
        .bind(policy.all_projects)
        .bind(&policy.projects)
        .bind(cfg.max_rows as i64 + 1)
        .fetch_all(&mut **tx)
        .await?;
    anyhow::ensure!(rows.len() <= cfg.max_rows, "row_capacity");
    let turn_source_count = rows.len();
    let mut exclusions = BTreeMap::new();
    let mut output = Vec::new();
    let mut reviewed: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for raw in rows {
        let Ok(source) = serde_json::from_value::<Source>(raw.0) else {
            *exclusions.entry("invalid_source".into()).or_default() += 1;
            continue;
        };
        if source.source_id.is_empty()
            || source.source_id.len() > 1024
            || source.event_id.is_empty()
            || source.event_id.len() > 1024
            || !crate::valid_devspace_id(&source.agent)
            || source.features.tool_errors > source.features.tool_calls
        {
            *exclusions.entry("invalid_source".into()).or_default() += 1;
            continue;
        }
        let features = serde_json::to_value(&source.features)?;
        let event_ref = reference(ds, "event", &source.event_id);
        let projection_digest = digest(&(&event_ref, &source.agent, &features));
        let mut row = json!({"event_ref":event_ref,"projection_digest":projection_digest,"agent":source.agent,"features":features,"label":null});
        if policy.purpose == Purpose::SignalReviews {
            let valid = source
                .source_digest
                .as_ref()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                && source
                    .signal
                    .as_ref()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 1024)
                && source
                    .reviewer
                    .as_ref()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 1024)
                && source.rule_version.is_some()
                && matches!(source.verdict.as_deref(), Some("match" | "not_match"));
            if !valid {
                *exclusions.entry("invalid_review".into()).or_default() += 1;
                continue;
            }
            row["source_turn_digest"] = json!(source.source_digest);
            row["label"] = json!({"signal_ref":reference(ds,"signal",source.signal.as_deref().unwrap()),"rule_version":source.rule_version,"verdict":source.verdict,"review_refs":[reference(ds,"review",&source.source_id)],"reviewer_refs":[reference(ds,"reviewer",source.reviewer.as_deref().unwrap())]});
            let group = digest(&(
                &row["event_ref"],
                &row["source_turn_digest"],
                &row["label"]["signal_ref"],
                &row["label"]["rule_version"],
            ));
            reviewed.entry(group).or_default().push(row);
        } else {
            output.push(row);
        }
    }
    for (_, rows) in reviewed {
        if rows
            .iter()
            .any(|r| r["label"]["verdict"] != rows[0]["label"]["verdict"])
        {
            *exclusions.entry("conflicting_reviews".into()).or_default() += rows.len();
            continue;
        }
        let mut first = rows[0].clone();
        for field in ["review_refs", "reviewer_refs"] {
            let mut refs: Vec<String> = rows
                .iter()
                .flat_map(|r| r["label"][field].as_array().unwrap())
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            refs.sort();
            refs.dedup();
            first["label"][field] = json!(refs);
        }
        *exclusions.entry("duplicate_reviews".into()).or_default() += rows.len() - 1;
        output.push(first);
    }
    if !policy.sdk_agents.is_empty() {
        let (sdk_rows, sdk_exclusions, source_count) =
            super::sdk::snapshot(tx, ds, policy, cfg).await?;
        anyhow::ensure!(
            turn_source_count + source_count <= cfg.max_rows,
            "row_capacity"
        );
        for row in &mut output {
            row["source_kind"] = json!("session_turn");
        }
        output.extend(sdk_rows);
        exclusions.extend(sdk_exclusions);
    }
    output.sort_by_key(digest);
    let input = Input {
        schema: if policy.sdk_agents.is_empty() {
            "rroflow.dataset-input/v1"
        } else {
            "rroflow.dataset-input/v2"
        }
        .into(),
        purpose: policy.purpose.clone(),
        rows: output,
        exclusions,
    };
    anyhow::ensure!(
        serde_json::to_vec(&input)?.len() <= cfg.max_snapshot_bytes,
        "snapshot_capacity"
    );
    Ok(input)
}

pub(super) fn build(
    input: &Input,
    ds: &str,
    id: &str,
    revision: i64,
    input_digest: &str,
    created: &str,
    expires: &str,
) -> anyhow::Result<Value> {
    anyhow::ensure!(
        ["rroflow.dataset-input/v1", "rroflow.dataset-input/v2"].contains(&input.schema.as_str())
            && digest(input) == input_digest,
        "input_integrity"
    );
    // Qualification is deliberately separate: no partition or training readiness is invented.
    Ok(json!({"manifest":{
        "schema":"rroflow.dataset/v1","id":id,"owner_ref":reference(ds,"owner",ds),"sharing":"private",
        "purpose":input.purpose,"policy_revision":revision,"consent_revision":revision,
        "projection":if input.schema=="rroflow.dataset-input/v2"{"metrics-only/v2"}else{"metrics-only/v1"},"input_digest":input_digest,"rows_digest":digest(&input.rows),
        "row_count":input.rows.len(),"exclusions":input.exclusions,"snapshot_at":created,"expires_at":expires,
        "builder_revision":crate::build_info().revision,"training_authorized":false,
        "qualification":"not_evaluated","partitions":{"unassigned":input.rows.len()},
        "contains_prompt_or_tool_content":false
    },"rows":input.rows}))
}
