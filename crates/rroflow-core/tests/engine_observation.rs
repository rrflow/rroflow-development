//! Contract fixtures only; these are not claimed engine receipts or training records.
use rroflow_core::{activity, capture::CaptureEvent};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({
        "schema":"rroflow.capture/v1",
        "event_id":"11111111-1111-4111-8111-111111111111",
        "capture_id":"22222222-2222-4222-8222-222222222222",
        "sequence":0,"agent":"rrflow","session_id":"fixture-session",
        "operation_id":"fixture-read","parent_operation_id":null,
        "observed_at":"2026-10-31T00:00:00Z","kind":"engine.observed",
        "engine":{
            "version":1,"instance_id":"fixture-instance","scope":"fixture-scope",
            "operation":"task-attempt-read","response_sha256":"a".repeat(64),
            "task_id":"fixture-task","attempt_id":"fixture-attempt",
            "job_id":null,"plan_sha256":null,"source_sha256":null,
            "configuration_sha256":"b".repeat(64)
        }
    })
}

#[test]
fn references_round_trip_without_becoming_success_usage_or_findings() {
    let event: CaptureEvent = serde_json::from_value(fixture()).unwrap();
    event.validate().unwrap();
    let stored = serde_json::to_vec(&event).unwrap();
    assert_eq!(
        serde_json::from_slice::<CaptureEvent>(&stored).unwrap(),
        event
    );
    let projected = activity::project(&[event]).unwrap();
    assert_eq!(projected[0].outcome, activity::Outcome::Unknown);
    assert_eq!(projected[0].provider_outcome, None);
    assert_eq!(projected[0].usage, None);
    assert!(projected[0].findings.is_empty());
    assert!(projected[0].tools.is_empty());
}

#[test]
fn identity_and_digest_validation_rejects_missing_or_malformed_references() {
    let cases = [
        ("version", json!(2)),
        ("instance_id", json!("")),
        ("scope", json!("contains private text")),
        ("operation", json!("x".repeat(129))),
        ("task_id", Value::Null),
        ("attempt_id", json!("bad\nidentity")),
        ("response_sha256", json!("A".repeat(64))),
        ("source_sha256", json!("short")),
        ("plan_sha256", json!("z".repeat(64))),
        ("configuration_sha256", json!("")),
    ];
    for (field, value) in cases {
        let mut row = fixture();
        row["engine"][field] = value;
        let event: CaptureEvent = serde_json::from_value(row).unwrap();
        assert!(event.validate().is_err(), "accepted invalid {field}");
    }
    let mut job = fixture();
    job["engine"]["task_id"] = Value::Null;
    job["engine"]["attempt_id"] = Value::Null;
    assert!(
        serde_json::from_value::<CaptureEvent>(job.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    job["engine"]["job_id"] = json!("fixture-job");
    job["engine"]["plan_sha256"] = json!("c".repeat(64));
    job["engine"]["source_sha256"] = json!("d".repeat(64));
    serde_json::from_value::<CaptureEvent>(job)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn engine_envelopes_cannot_carry_content_provider_success_or_usage() {
    let cases = [
        ("agent", json!("codex")),
        ("kind", json!("call.completed")),
        ("engine", Value::Null),
        ("payload", json!({"private":"text"})),
        (
            "usage",
            json!({"input_tokens":0,"input_includes_cache":false}),
        ),
        ("model", json!("fixture-model")),
        ("native_status", json!("completed")),
        ("tool_name", json!("fixture-tool")),
        ("exit_code", json!(0)),
    ];
    for (field, value) in cases {
        let mut row = fixture();
        row[field] = value;
        assert!(
            serde_json::from_value::<CaptureEvent>(row)
                .unwrap()
                .validate()
                .is_err(),
            "accepted forbidden {field}"
        );
    }
    let mut unknown = fixture();
    unknown["engine"]["training_consent"] = json!(true);
    assert!(serde_json::from_value::<CaptureEvent>(unknown).is_err());
}

#[test]
fn older_capture_events_do_not_gain_engine_metadata() {
    let mut row = fixture();
    row.as_object_mut().unwrap().remove("engine");
    row["agent"] = json!("codex");
    row["kind"] = json!("call.started");
    let event: CaptureEvent = serde_json::from_value(row).unwrap();
    event.validate().unwrap();
    assert!(event.engine.is_none());
    assert!(
        serde_json::to_value(&event)
            .unwrap()
            .get("engine")
            .is_none()
    );
}
