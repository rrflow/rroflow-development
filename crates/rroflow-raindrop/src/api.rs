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
                bail!("{path_and_query} rejected with HTTP {code}")
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
    }
    bail!("{path_and_query} failed after 3 attempts: {last}")
}
