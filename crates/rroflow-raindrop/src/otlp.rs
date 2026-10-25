//! Raindrop event and OTLP span construction for one turn.

use crate::redact::{Redactor, truncate};
use crate::transcript::{Turn, ts_nanos};
use serde_json::{Value, json};
use uuid::Uuid;

const NS: Uuid = Uuid::from_u128(0x7f1c_2d3e_4a5b_46c7_8d9e_0f1a_2b3c_4d5e);

fn hex_id(seed: &str, bytes: usize) -> String {
    let u = Uuid::new_v5(&NS, seed.as_bytes());
    u.simple().to_string()[..bytes * 2].to_string()
}

fn attr(k: &str, v: Value) -> Value {
    let val = match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({"intValue": n.to_string()}),
        Value::Number(n) => json!({"doubleValue": n}),
        Value::Bool(b) => json!({"boolValue": b}),
        other => {
            json!({"stringValue": other.as_str().map(String::from).unwrap_or_else(|| other.to_string())})
        }
    };
    json!({"key": k, "value": val})
}

pub(crate) struct Built {
    pub(crate) event: Value,
    pub(crate) spans: Vec<Value>,
}

pub(crate) fn build(
    t: &Turn,
    user: &str,
    workspace: Option<&str>,
    payloads: bool,
    r: &Redactor,
) -> Built {
    let event_id = Uuid::new_v5(&NS, format!("{}:{}", t.session, t.key).as_bytes()).to_string();
    let trace_id = hex_id(&format!("trace:{event_id}"), 16);
    let root = hex_id(&format!("root:{event_id}"), 8);
    // Only finished tool calls are sent. Raindrop keeps every version of a re-sent span in
    // the event's tool_calls list, so sending an in-flight call (status UNSET) and later its
    // finished version double-counts it. An unfinished call is sent by the next run instead.
    let finished: Vec<&crate::transcript::Tool> =
        t.tools.iter().filter(|x| x.end.is_some()).collect();
    let errors = finished.iter().filter(|x| x.error).count();
    let mut tool_names: Vec<&str> = finished.iter().map(|x| x.name.as_str()).collect();
    tool_names.sort();
    tool_names.dedup();
    let output = r.apply(&truncate(&t.output.join("\n\n"), 20_000));
    let mut props = json!({
        "source": t.agent,
        "agent": t.agent,
        "agent_version": t.version,
        "session_id": t.session,
        "cwd": t.cwd,
        "git_branch": t.branch,
        "entrypoint": t.entrypoint,
        "compacted": t.compacted,
        "tool_calls": finished.len(),
        "tool_calls_unfinished": t.tools.len() - finished.len(),
        "tool_errors": errors,
        "tool_names": tool_names.join(","),
        "ingested_by": concat!("rroflow-raindrop/", env!("CARGO_PKG_VERSION")),
    });
    if t.agent == "claude-code" {
        // Kept for signals and filters written before `agent_version` existed.
        props["claude_code_version"] = json!(t.version);
    }
    if let Some(w) = workspace {
        props["workspace"] = json!(w);
    }
    let mut ai = json!({"input": r.apply(&truncate(&t.input, 20_000)), "convo_id": t.session});
    if !output.is_empty() {
        ai["output"] = json!(output);
    }
    if let Some(m) = &t.model {
        ai["model"] = json!(m);
    }
    let event_name = format!("{}.turn", t.agent.replace('-', "_"));
    let event = json!({"event_id": event_id, "user_id": user, "event": event_name, "timestamp": t.start, "ai_data": ai, "properties": props});

    let start = ts_nanos(&t.start).unwrap_or(0);
    let end = ts_nanos(&t.end).unwrap_or(start).max(start);
    let assoc = |k: &str, v: &str| attr(&format!("traceloop.association.properties.{k}"), json!(v));
    let model = t.model.clone().unwrap_or_else(|| "unknown".into());
    let mut spans = vec![json!({
        "traceId": trace_id, "spanId": root, "name": event_name, "kind": 1,
        "startTimeUnixNano": start.to_string(), "endTimeUnixNano": end.to_string(),
        "status": {"code": if errors > 0 { 2 } else { 1 }},
        "attributes": [
            assoc("event_id", &event_id), assoc("user_id", user), assoc("convo_id", &t.session),
            attr("ai.operationId", json!("ai.generateText")), attr("traceloop.span.kind", json!("llm")),
            attr("gen_ai.system", json!(t.provider)), attr("gen_ai.request.model", json!(model)),
            attr("gen_ai.response.model", json!(model)),
            attr("gen_ai.usage.input_tokens", json!(t.in_tokens)), attr("gen_ai.usage.output_tokens", json!(t.out_tokens)),
        ]
    })];
    for tool in &finished {
        let s = ts_nanos(&tool.start).unwrap_or(start);
        let e = tool.end.as_deref().and_then(ts_nanos).unwrap_or(s).max(s);
        let mut attrs = vec![
            assoc("event_id", &event_id),
            attr("ai.operationId", json!("ai.toolCall")),
            attr("traceloop.span.kind", json!("tool")),
            attr("traceloop.entity.name", json!(tool.name)),
            attr(
                "traceloop.entity.duration_ms",
                json!(((e - s) / 1_000_000) as i64),
            ),
        ];
        if payloads {
            attrs.push(attr(
                "traceloop.entity.input",
                json!(r.apply(&truncate(&tool.input, 2_000))),
            ));
            attrs.push(attr(
                "traceloop.entity.output",
                json!(r.apply(&truncate(&tool.output, 2_000))),
            ));
        }
        let status = if tool.error {
            json!({"code": 2, "message": r.apply(&truncate(&tool.output, 300))})
        } else {
            json!({"code": 1})
        };
        spans.push(json!({
            "traceId": trace_id, "spanId": hex_id(&format!("tool:{}", tool.id), 8), "parentSpanId": root,
            "name": tool.name, "kind": 1, "startTimeUnixNano": s.to_string(), "endTimeUnixNano": e.to_string(),
            "status": status, "attributes": attrs
        }));
    }
    Built { event, spans }
}

/// Wrap spans in one OTLP/JSON `resourceSpans` body.
pub(crate) fn trace_body(service: &str, spans: &[Value]) -> Value {
    json!({"resourceSpans": [{
        "resource": {"attributes": [attr("service.name", json!(service))]},
        "scopeSpans": [{"scope": {"name": "rroflow-raindrop", "version": env!("CARGO_PKG_VERSION")}, "spans": spans}]
    }]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_ids_are_deterministic() {
        let t = Turn {
            session: "s".into(),
            key: "k".into(),
            start: "2026-10-24T00:00:00Z".into(),
            end: "2026-10-24T00:00:01Z".into(),
            input: "hi".into(),
            ..Default::default()
        };
        let r = Redactor::new();
        assert_eq!(
            build(&t, "u", None, false, &r).event["event_id"],
            build(&t, "u", None, false, &r).event["event_id"]
        );
    }
    #[test]
    fn unfinished_tool_calls_are_held_back() {
        use crate::transcript::Tool;
        let t = Turn {
            session: "s".into(),
            key: "k".into(),
            start: "2026-10-24T00:00:00Z".into(),
            end: "2026-10-24T00:00:05Z".into(),
            tools: vec![
                Tool {
                    id: "a".into(),
                    name: "Bash".into(),
                    start: "2026-10-24T00:00:01Z".into(),
                    end: Some("2026-10-24T00:00:02Z".into()),
                    ..Default::default()
                },
                Tool {
                    id: "b".into(),
                    name: "Bash".into(),
                    start: "2026-10-24T00:00:03Z".into(),
                    end: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let b = build(&t, "u", None, false, &Redactor::new());
        assert_eq!(b.spans.len(), 2, "root plus the one finished tool");
        assert_eq!(b.event["properties"]["tool_calls"], 1);
        assert_eq!(b.event["properties"]["tool_calls_unfinished"], 1);
    }
}
