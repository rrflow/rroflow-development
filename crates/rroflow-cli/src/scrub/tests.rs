use super::plants::{TEMPLATES, plant, survives};
use super::*;
use serde_json::{Value, json};

fn scrubber() -> Scrubber {
    Scrubber::with_literals(vec![])
}

#[test]
fn every_planted_secret_is_removed_from_plain_text() {
    let s = scrubber();
    for index in 0..TEMPLATES.len() {
        let planted = plant(index);
        let out = s.scrub(&format!("before {} after", planted.text));
        assert!(
            !survives(&out.text, &planted.secret),
            "{} survived: {}",
            planted.kind,
            out.text
        );
        assert!(!out.found.is_empty(), "{} was not counted", planted.kind);
    }
}

#[test]
fn every_planted_secret_is_removed_from_json_and_the_line_stays_valid() {
    let s = scrubber();
    for index in 0..TEMPLATES.len() {
        let planted = plant(index);
        // Once as a value in a session line, and again as text inside a tool's JSON output.
        let inner = json!({"content": planted.text}).to_string();
        let lines = [
            json!({"type":"user","message":{"content":planted.text}}).to_string(),
            json!({"type":"tool_result","content":inner}).to_string(),
        ];
        for line in lines {
            let out = s.scrub(&line);
            assert!(
                !survives(&out.text, &planted.secret),
                "{} survived: {}",
                planted.kind,
                out.text
            );
            serde_json::from_str::<Value>(&out.text)
                .unwrap_or_else(|e| panic!("{} broke the line: {e}: {}", planted.kind, out.text));
        }
    }
}

#[test]
fn scrubbing_twice_changes_nothing() {
    let s = scrubber();
    for index in 0..TEMPLATES.len() {
        let once = s.scrub(&plant(index).text);
        let twice = s.scrub(&once.text);
        assert_eq!(once.text, twice.text);
        assert!(twice.found.is_empty(), "{:?}", twice.found);
    }
}

#[test]
fn ordinary_session_content_is_left_alone() {
    let s = scrubber();
    for text in [
        r#"{"usage":{"input_tokens":1234,"cache_read_input_tokens":99,"output_tokens":5}}"#,
        r#"{"model":"claude-opus-5-5","sessionId":"25b1106c-d759-49c9-a1b2-0f84903cb26d"}"#,
        "commit 0f849034cb26d7f3aa87d93c1fe049765679fad3",
        "Basic authentication and bearer authentication are both described in the guide.",
        "fn login(password: &str, token: String) -> Result<Session>",
        "password: string; api_key: Option<String>,",
        "let token = get_token(); let password = read_password();",
        "MAX_TOKENS=4096",
        "PASSWORD=${DB_PASSWORD}",
        "API_KEY=your-api-key-here",
        "DB_PASSWORD=changeme",
        "http://localhost:8080/path and https://example.test:443/a@b",
        "risk-assessment-framework-overview and task-management-notes",
        "The cookie: it is used to remember the session.",
        "npm_config_registry and sk-short and xoxb-",
    ] {
        let out = s.scrub(text);
        assert_eq!(out.text, text, "changed: {:?}", out.found);
    }
}

#[test]
fn a_url_keeps_everything_but_its_password() {
    let out = scrubber().scrub("postgres://app:hunter2@db.internal:5432/main");
    assert_eq!(
        out.text,
        "postgres://app:[REDACTED:url_password]@db.internal:5432/main"
    );
}

#[test]
fn configured_values_are_removed_and_short_ones_ignored() {
    let s = Scrubber::with_literals(vec!["25b1106c-d759-49c9".into(), "short".into()]);
    let out = s.scrub("key=25b1106c-d759-49c9 short");
    assert_eq!(out.text, "key=[REDACTED:configured_secret] short");
}

#[test]
fn a_match_starting_on_a_json_escape_keeps_the_line_valid() {
    let hex = "a".repeat(63);
    for escape in ["\\f", "\\b"] {
        let line = format!(r#"{{"text":"before{escape}{hex} after"}}"#);
        let out = scrubber().scrub(&line);
        assert!(out.found.contains_key("long_hex"), "{}", out.text);
        serde_json::from_str::<Value>(&out.text).unwrap_or_else(|e| panic!("{e}: {}", out.text));
    }
}

#[test]
fn a_rejected_match_does_not_hide_a_secret_behind_it() {
    // The first field's "value" runs up to the second field's opening quote and is rejected as
    // code. The real field must still be found.
    let secret = "Zx9SecretValue1234567890";
    let line = json!({
        "text": format!("\"token\":\"abcdefg  {{\"accessToken\":\"{secret}\"}} ")
    })
    .to_string();
    let out = scrubber().scrub(&line);
    assert!(!out.text.contains(secret), "{}", out.text);
}

#[test]
fn a_cookie_is_removed_without_the_text_after_it() {
    let out = scrubber().scrub("Cookie: session=abc123456789; theme=dark then foo(bar) {x}");
    assert_eq!(out.text, "Cookie: [REDACTED:cookie] then foo(bar) {x}");
}

#[test]
fn a_token_with_no_signature_is_still_removed() {
    let out = scrubber().scrub("eyJhbGciOiJSUzI1.eyJzdWIiOiIxMjM0NTY3ODkw");
    assert_eq!(out.text, "[REDACTED:jwt]");
}

#[test]
fn markers_cannot_break_a_json_string() {
    for rule in RULES {
        assert!(!rule.kind.contains(['"', '\\']));
    }
}

#[test]
fn survival_catches_a_partly_removed_secret() {
    let secret = "abcdefghijklmnopqrstuvwxyz012345";
    assert!(survives("x abcdefghijklmnop x", secret));
    assert!(survives("x stuvwxyz012345 x", secret));
    assert!(!survives("x abcdefg x", secret));
}

#[test]
fn the_version_is_recorded_by_name() {
    assert_eq!(VERSION, "rroflow.scrub/1");
}
