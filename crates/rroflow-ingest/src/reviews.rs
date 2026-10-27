//! Human verdicts are immutable evidence, never an instruction to train or promote.
use crate::{ApiError, AppState, Store, internal};
use axum::{Json, extract::State, http::HeaderMap};
use rroflow_core::TurnRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Verdict {
    Match,
    NotMatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    review_id: String,
    event_id: String,
    turn_digest: String,
    signal: String,
    rule_version: u32,
    verdict: Verdict,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Review {
    request: Request,
    recorded_at: String,
    reviewer: String,
    snapshot: TurnRecord,
}
pub(crate) fn digest(t: &TurnRecord) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(t).expect("serialize turn"),
    ))
}
fn same_request(a: &Review, b: &Review) -> bool {
    a.request == b.request && a.reviewer == b.reviewer
}
fn validate_id(r: &Request) -> Result<(), ApiError> {
    if r.review_id.len() != 36
        || !r.review_id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(ApiError::BadRequest("review_id must be a UUID".into()));
    }
    Ok(())
}
fn validate(r: &Request, t: &TurnRecord) -> Result<(), ApiError> {
    validate_id(r)?;
    if r.event_id != t.event_id || r.turn_digest != digest(t) {
        return Err(ApiError::BadRequest(
            "Evidence changed. Refresh and review the new revision.".into(),
        ));
    }
    if !t
        .signals
        .iter()
        .any(|s| s.signal == r.signal && s.rule_version == r.rule_version)
    {
        return Err(ApiError::BadRequest(
            "Signal revision is absent from this evidence.".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn save(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(r): Json<Request>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let caller = st.caller(&headers).await?;
    let reviewer = caller.subject.unwrap_or_else(|| "devspace-key".into());
    // An acknowledged request can be retried even if newer capture has replaced the turn.
    validate_id(&r)?;
    let existing = read_one(&st.store, &caller.devspace, &r.review_id)
        .await
        .map_err(internal)?;
    if let Some(old) = existing {
        if old.request != r || old.reviewer != reviewer {
            return Err(ApiError::Conflict);
        }
        return Ok(Json(
            serde_json::json!({"review_id":r.review_id,"saved":true}),
        ));
    }
    let turn = match st.store.as_ref() {
        Store::Jsonl(s) => s.review_turn(&caller.devspace, &r.event_id),
        Store::Postgres(p) => {
            let row: Option<sqlx::types::Json<TurnRecord>> =
                sqlx::query_scalar("SELECT record FROM turns WHERE devspace_id=$1 AND event_id=$2")
                    .bind(&caller.devspace)
                    .bind(&r.event_id)
                    .fetch_optional(&p.pool)
                    .await
                    .map_err(|e| internal(e.into()))?;
            row.map(|x| x.0)
        }
    }
    .ok_or_else(|| ApiError::BadRequest("Turn was not found in this devspace.".into()))?;
    validate(&r, &turn)?;
    let review = Review {
        request: r,
        reviewer,
        recorded_at: chrono::Utc::now().to_rfc3339(),
        snapshot: turn,
    };
    let saved = write(&st.store, &caller.devspace, &review).await;
    match saved {
        Ok(()) => Ok(Json(
            serde_json::json!({"review_id":review.request.review_id,"saved":true}),
        )),
        Err(e) if e.downcast_ref::<crate::capture::Conflict>().is_some() => Err(ApiError::Conflict),
        Err(e) => Err(internal(e)),
    }
}
async fn read_one(store: &Store, ds: &str, id: &str) -> anyhow::Result<Option<Review>> {
    match store {
        Store::Postgres(p) => {
            let row: Option<sqlx::types::Json<Review>> = sqlx::query_scalar(
                "SELECT record FROM signal_reviews WHERE devspace_id=$1 AND review_id=$2",
            )
            .bind(ds)
            .bind(id)
            .fetch_optional(&p.pool)
            .await?;
            Ok(row.map(|x| x.0))
        }
        Store::Jsonl(s) => {
            let path = s.root.join(ds).join("reviews").join(format!("{id}.json"));
            match std::fs::read(path) {
                Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.into()),
            }
        }
    }
}
pub(crate) async fn list(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ds = st.caller(&headers).await?.devspace;
    let reviews = read(&st.store, &ds).await.map_err(internal)?;
    let items: Vec<_> = reviews.iter().map(|r| serde_json::json!({"request":r.request,"recorded_at":r.recorded_at,"reviewer":r.reviewer})).collect();
    Ok(Json(
        serde_json::json!({"devspace":ds,"reviews":items,"limit":1000}),
    ))
}
async fn read(store: &Store, ds: &str) -> anyhow::Result<Vec<Review>> {
    let mut reviews = match store {
        Store::Postgres(p) => {
            let rows:Vec<sqlx::types::Json<Review>> = sqlx::query_scalar("SELECT record FROM signal_reviews WHERE devspace_id=$1 ORDER BY recorded_at DESC,review_id DESC LIMIT 1000")
                .bind(ds).fetch_all(&p.pool).await?;
            rows.into_iter().map(|x| x.0).collect::<Vec<_>>()
        }
        Store::Jsonl(s) => {
            let dir = s.root.join(ds).join("reviews");
            let mut rows = vec![];
            if dir.exists() {
                for entry in std::fs::read_dir(dir)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|x| x == "json") {
                        rows.push(serde_json::from_slice::<Review>(&std::fs::read(path)?)?);
                    }
                }
            }
            rows
        }
    };
    reviews.sort_by(|a, b| {
        b.recorded_at
            .cmp(&a.recorded_at)
            .then(b.request.review_id.cmp(&a.request.review_id))
    });
    reviews.truncate(1000);
    Ok(reviews)
}
async fn write(store: &Store, ds: &str, r: &Review) -> anyhow::Result<()> {
    match store {
        Store::Postgres(p) => {
            sqlx::query("INSERT INTO signal_reviews(devspace_id,review_id,recorded_at,record) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(ds).bind(&r.request.review_id).bind(crate::pg::timestamp(&r.recorded_at)).bind(sqlx::types::Json(r)).execute(&p.pool).await?;
            let old: sqlx::types::Json<Review> = sqlx::query_scalar(
                "SELECT record FROM signal_reviews WHERE devspace_id=$1 AND review_id=$2",
            )
            .bind(ds)
            .bind(&r.request.review_id)
            .fetch_one(&p.pool)
            .await?;
            if !same_request(&old.0, r) {
                return Err(crate::capture::Conflict.into());
            }
        }
        Store::Jsonl(s) => {
            let dir = s.root.join(ds).join("reviews");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}.json", r.request.review_id));
            if path.exists() {
                let old: Review = serde_json::from_slice(&std::fs::read(path)?)?;
                if !same_request(&old, r) {
                    return Err(crate::capture::Conflict.into());
                }
                return Ok(());
            }
            let tmp = dir.join(format!(
                "{}.{}.tmp",
                r.request.review_id,
                rand::random::<u64>()
            ));
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp)?;
            f.write_all(&serde_json::to_vec(r)?)?;
            f.sync_all()?;
            drop(f);
            if let Err(e) = std::fs::hard_link(&tmp, &path) {
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(e.into());
                }
                let old: Review = serde_json::from_slice(&std::fs::read(&path)?)?;
                if !same_request(&old, r) {
                    return Err(crate::capture::Conflict.into());
                }
            }
            std::fs::remove_file(tmp)?;
            #[cfg(unix)]
            for d in [&dir, &s.root.join(ds), &s.root] {
                std::fs::File::open(d)?.sync_all()?;
            }
        }
    }
    Ok(())
}
