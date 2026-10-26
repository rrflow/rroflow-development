//! Raindrop HTTP: the write API (events, traces) and the Query API (read-back).
//!
//! Both select the project with the `X-Raindrop-Project-Id` header. The Query API silently
//! ignores a `?project=` parameter, so the header is the only selector used here.

use anyhow::{Result, bail};
use serde_json::Value;

/// POST to the write API with retry on 429 and 5xx.
pub(crate) fn post(base: &str, path: &str, key: &str, project: &str, body: &Value) -> Result<u16> {
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::post(&format!("{base}/{path}"))
            .header("Authorization", &format!("Bearer {key}"))
            .header("X-Raindrop-Project-Id", project)
            .send_json(body)
        {
            Ok(resp) => return Ok(resp.status().as_u16()),
            Err(ureq::Error::StatusCode(code)) if code != 429 && code < 500 => {
                bail!("{path} rejected with HTTP {code}")
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("{path} failed after 3 attempts: {last}")
}

/// GET from the Query API; returns the parsed JSON body.
pub(crate) fn get(base: &str, path_and_query: &str, key: &str, project: &str) -> Result<Value> {
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::get(&format!("{base}/{path_and_query}"))
            .header("Authorization", &format!("Bearer {key}"))
            .header("X-Raindrop-Project-Id", project)
            .call()
        {
            Ok(mut resp) => return Ok(resp.body_mut().read_json::<Value>()?),
            Err(ureq::Error::StatusCode(code)) if code != 429 && code < 500 => {
                bail!(
                    "{path_and_query} rejected with HTTP {code}{}",
                    read_hint(code, project)
                )
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("{path_and_query} failed after 3 attempts: {last}")
}

/// Plain-language cause for a refused read, from what was observed on the live API.
fn read_hint(code: u16, project: &str) -> String {
    match code {
        401 => ". The Query API key was not accepted: use a Query API key (Settings → API keys), not the write key".into(),
        403 => format!(
            ". Project `{project}` may have data but not be created yet: open Raindrop → Settings → Projects → \"Projects with data, not yet created\" and click Create project. Otherwise the key has no access to it"
        ),
        _ => String::new(),
    }
}

/// What a refused rroflow credential means, in plain words.
const REFUSED_CREDENTIAL: &str = "rroflow ingest refused the credential (HTTP 401): run `rroflow whoami`, then `rroflow login` (or check RROFLOW_INGEST_KEY)";

/// POST `rroflow.turn/v1` records to a devspace's rroflow ingest. The credential decides the devspace.
pub(crate) fn post_rroflow(base: &str, key: &str, body: &Value) -> Result<u16> {
    let url = format!("{}/v1/turns", base.trim_end_matches('/'));
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::post(&url)
            .header("Authorization", &format!("Bearer {key}"))
            .send_json(body)
        {
            Ok(resp) => return Ok(resp.status().as_u16()),
            Err(ureq::Error::StatusCode(401)) => {
                bail!(REFUSED_CREDENTIAL)
            }
            Err(ureq::Error::StatusCode(code)) if code != 429 && code < 500 => {
                bail!("rroflow ingest rejected the batch with HTTP {code}")
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("rroflow ingest at {url} failed after 3 attempts: {last}")
}

/// GET a devspace resource from the rroflow ingest (for example `/v1/signals/settings`).
pub(crate) fn get_rroflow(base: &str, key: &str, path: &str) -> Result<Value> {
    let url = format!("{}{path}", base.trim_end_matches('/'));
    let mut last = String::new();
    for attempt in 0..3 {
        match ureq::get(&url)
            .header("Authorization", &format!("Bearer {key}"))
            .call()
        {
            Ok(mut resp) => return Ok(resp.body_mut().read_json::<Value>()?),
            Err(ureq::Error::StatusCode(401)) => {
                bail!(REFUSED_CREDENTIAL)
            }
            Err(ureq::Error::StatusCode(code)) if code != 429 && code < 500 => {
                bail!("rroflow ingest answered {path} with HTTP {code}")
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("rroflow ingest at {url} failed after 3 attempts: {last}")
}
