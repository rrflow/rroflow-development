//! Read-only, owner-scoped signal aggregates. Counts use current records, not a
//! client page or a training/evaluation population. No captured content is returned.
use crate::{
    ApiError, AppState, internal, pg,
    store::{Store, TurnFilter},
};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const BREAKDOWN_LIMIT: usize = 12;
const DAY_LIMIT: usize = 30;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignalQuery {
    signal: String,
    agent: Option<String>,
    project: Option<String>,
    session: Option<String>,
    search: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Bucket {
    pub key: String,
    pub received_turns: usize,
    pub matched_turns: usize,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct SignalStats {
    pub received_turns: usize,
    pub matched_turns: usize,
    pub undated_matches: usize,
    pub by_day: Vec<Bucket>,
    pub by_agent: Vec<Bucket>,
    pub by_project: Vec<Bucket>,
    pub other_day_matches: usize,
    pub other_agent_matches: usize,
    pub other_project_matches: usize,
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SignalQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = st.caller(&headers).await?.devspace;
    if q.signal.trim().is_empty() || q.signal.chars().count() > 128 {
        return Err(ApiError::BadRequest("Choose a valid signal".into()));
    }
    for (value, limit) in [
        (&q.agent, 128),
        (&q.project, 2048),
        (&q.session, 2048),
        (&q.search, 200),
    ] {
        if value.as_ref().is_some_and(|s| s.chars().count() > limit) {
            return Err(ApiError::BadRequest("A filter is too long".into()));
        }
    }
    let date = |v: &Option<String>| {
        v.as_deref()
            .map(|s| {
                pg::timestamp(s).ok_or_else(|| ApiError::BadRequest("Invalid date boundary".into()))
            })
            .transpose()
    };
    let (since, until) = (date(&q.since)?, date(&q.until)?);
    if since.zip(until).is_some_and(|(a, b)| a >= b) {
        return Err(ApiError::BadRequest("Start must be before end".into()));
    }
    fn nonempty(s: &Option<String>) -> Option<&str> {
        s.as_deref().filter(|s| !s.is_empty())
    }
    let filter = TurnFilter {
        signal: Some(&q.signal),
        agent: nonempty(&q.agent),
        project: nonempty(&q.project),
        session: nonempty(&q.session),
        search: q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        since,
        until,
        ..Default::default()
    };
    let stats = aggregate(&st.store, &owner, &filter)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({
        "schema":"rroflow.signal-stats/v1", "devspace":owner,
        "as_of":chrono::Utc::now().to_rfc3339(),
        "scope":{"signal":q.signal,"agent":filter.agent,"project":filter.project,"session":filter.session,"search":filter.search,"since":since.map(|v|v.to_rfc3339()),"until":until.map(|v|v.to_rfc3339())},
        "stats":stats,
        "limits":{"breakdowns":BREAKDOWN_LIMIT,"active_days":DAY_LIMIT}
    })))
}

type Row = (String, String, i64, i64);

pub(crate) async fn aggregate(
    store: &Store,
    owner: &str,
    f: &TurnFilter<'_>,
) -> anyhow::Result<SignalStats> {
    match store {
        Store::Jsonl(store) => {
            let index = store.index.lock().expect("store lock");
            let mut groups: BTreeMap<(String, String), (i64, i64)> = BTreeMap::new();
            // Ignore the selected signal in the received-turn denominator.
            let cohort = TurnFilter { signal: None, ..*f };
            for turn in index
                .get(owner)
                .into_iter()
                .flat_map(|m| m.values())
                .filter(|r| crate::store::matches_filter(r, &cohort))
            {
                let hit = i64::from(
                    turn.signals
                        .iter()
                        .any(|h| Some(h.signal.as_str()) == f.signal),
                );
                let day =
                    pg::timestamp(&turn.started_at).map(|at| at.format("%Y-%m-%d").to_string());
                for pair in [
                    ("total", String::new()),
                    ("agent", turn.agent.clone()),
                    ("project", crate::store::project(turn).into()),
                    (
                        if day.is_some() { "day" } else { "undated" },
                        day.unwrap_or_default(),
                    ),
                ] {
                    let entry = groups.entry((pair.0.into(), pair.1)).or_default();
                    entry.0 += 1;
                    entry.1 += hit;
                }
            }
            Ok(from_rows(
                groups
                    .into_iter()
                    .map(|((d, k), (n, h))| (d, k, n, h))
                    .collect(),
            ))
        }
        Store::Postgres(store) => {
            let mut tx = store.pool.begin().await?;
            // Bound database work as well as the response. This is one consistent
            // statement snapshot; live ingestion can continue concurrently.
            sqlx::query("SET LOCAL statement_timeout = '5s'")
                .execute(&mut *tx)
                .await?;
            let rows: Vec<Row> = sqlx::query_as(r#"
                WITH cohort AS (
                    SELECT t.agent, coalesce(nullif(t.project,''),nullif(t.record->>'cwd',''),'(none)') AS project,
                      to_char(t.started_at AT TIME ZONE 'UTC','YYYY-MM-DD') AS day,
                      EXISTS (SELECT 1 FROM signal_hits h WHERE h.devspace_id=t.devspace_id AND h.event_id=t.event_id AND h.signal=$2) AS hit
                    FROM turns t WHERE t.devspace_id=$1
                      AND ($3::text IS NULL OR t.agent=$3)
                      AND ($4::text IS NULL OR coalesce(nullif(t.project,''),nullif(t.record->>'cwd',''),'(none)')=$4)
                      AND ($5::text IS NULL OR t.record->>'session_id'=$5)
                      AND ($6::text IS NULL OR position(lower($6) in lower(concat_ws(E'\n',t.record->>'session_id',t.record->>'model',t.record#>>'{content,input}',t.record#>>'{content,output}')))>0)
                      AND ($7::timestamptz IS NULL OR t.started_at >= $7)
                      AND ($8::timestamptz IS NULL OR t.started_at < $8)
                ), grouped AS (
                    SELECT CASE WHEN grouping(agent)=0 THEN 'agent' WHEN grouping(project)=0 THEN 'project'
                      WHEN grouping(day)=0 THEN CASE WHEN day IS NULL THEN 'undated' ELSE 'day' END ELSE 'total' END AS dimension,
                      coalesce(agent,project,day,'') AS key,
                      count(*) AS received, count(*) FILTER (WHERE hit) AS matched
                    FROM cohort GROUP BY GROUPING SETS ((),(agent),(project),(day))
                ), ranked AS (
                    SELECT *, row_number() OVER (PARTITION BY dimension ORDER BY
                      CASE WHEN dimension='day' THEN key END DESC,
                      matched DESC, received DESC, key COLLATE "C") AS rank
                    FROM grouped
                )
                SELECT dimension,key,received,matched FROM ranked
                  WHERE dimension IN ('total','undated') OR rank <= CASE WHEN dimension='day' THEN $9 ELSE $10 END
                UNION ALL
                SELECT 'other_' || dimension,'',sum(received)::int8,sum(matched)::int8 FROM ranked
                  WHERE dimension IN ('day','agent','project') AND rank > CASE WHEN dimension='day' THEN $9 ELSE $10 END
                  GROUP BY dimension
            "#).bind(owner).bind(f.signal).bind(f.agent).bind(f.project).bind(f.session).bind(f.search).bind(f.since).bind(f.until)
                .bind(DAY_LIMIT as i64).bind(BREAKDOWN_LIMIT as i64).fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok(from_rows(rows))
        }
    }
}

fn from_rows(rows: Vec<Row>) -> SignalStats {
    let mut s = SignalStats::default();
    for (dimension, key, n, h) in rows {
        let b = Bucket {
            key,
            received_turns: n.max(0) as usize,
            matched_turns: h.max(0) as usize,
        };
        match dimension.as_str() {
            "total" => {
                s.received_turns = b.received_turns;
                s.matched_turns = b.matched_turns;
            }
            "undated" => s.undated_matches = b.matched_turns,
            "day" => s.by_day.push(b),
            "agent" => s.by_agent.push(b),
            "project" => s.by_project.push(b),
            "other_day" => s.other_day_matches = b.matched_turns,
            "other_agent" => s.other_agent_matches = b.matched_turns,
            "other_project" => s.other_project_matches = b.matched_turns,
            _ => unreachable!("internal aggregate dimension"),
        }
    }
    for (v, other) in [
        (&mut s.by_agent, &mut s.other_agent_matches),
        (&mut s.by_project, &mut s.other_project_matches),
    ] {
        v.sort_by(|a, b| {
            b.matched_turns
                .cmp(&a.matched_turns)
                .then_with(|| b.received_turns.cmp(&a.received_turns))
                .then_with(|| a.key.cmp(&b.key))
        });
        *other += v
            .iter()
            .skip(BREAKDOWN_LIMIT)
            .map(|b| b.matched_turns)
            .sum::<usize>();
        v.truncate(BREAKDOWN_LIMIT);
    }
    s.by_day.sort_by(|a, b| b.key.cmp(&a.key));
    s.other_day_matches += s
        .by_day
        .iter()
        .skip(DAY_LIMIT)
        .map(|b| b.matched_turns)
        .sum::<usize>();
    s.by_day.truncate(DAY_LIMIT);
    s.by_day.reverse();
    s
}
