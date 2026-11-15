//! Request types, and the checks a record passes before it is stored.
use crate::{pg, provenance};
use rroflow_core::{SCHEMA, TurnRecord};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnsBody {
    #[serde(deserialize_with = "provenance::deserialize_turns")]
    pub turns: Vec<TurnRecord>,
}

/// Check one record before it is stored. Returns the problem in plain words.
pub(super) fn check_record(r: &TurnRecord) -> Result<(), String> {
    if r.schema != SCHEMA {
        return Err(format!("schema {:?} is not {SCHEMA}", r.schema));
    }
    if r.event_id.is_empty() {
        return Err("event_id is empty".into());
    }
    for (field, v) in [("started_at", &r.started_at), ("ended_at", &r.ended_at)] {
        if !v.is_empty() && pg::timestamp(v).is_none() {
            return Err(format!("{field} {v:?} is not an RFC 3339 timestamp"));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    pub limit: Option<usize>,
    pub agent: Option<String>,
    pub signal: Option<String>,
    pub project: Option<String>,
    pub session: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub cursor: Option<String>,
    pub event: Option<String>,
}

/// Most records one read returns.
pub(super) const LIST_MAX: usize = 1000;
