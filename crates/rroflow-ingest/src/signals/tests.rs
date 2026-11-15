//! Signal tests: the catalogue, settings, custom rules, preview and statistics.
use crate::tests::*;

#[tokio::test]
async fn signal_dashboard_counts_full_cohorts_and_preserves_evidence_scope() {
    for backend in backends() {
        let env = Env::new(backend, "signal_stats");
        let app = env.app().await;
        let start = pg::timestamp("2026-09-01T00:00:00Z").unwrap();
        let mut turns: Vec<_> = (0..125)
            .map(|i| {
                let mut r = rec(&format!("stat-{i}"), Some("blocked_handoff"));
                r.agent = format!("agent-{}", i % 15);
                r.project = Some(format!("project-{}", i % 15));
                r.started_at = (start + chrono::Duration::days(i % 40)).to_rfc3339();
                r.ended_at = r.started_at.clone();
                r
            })
            .collect();
        let duplicate = turns[0].signals[0].clone();
        turns[0].signals.push(duplicate);
        turns[0].model = Some("literal%_needle".into());
        let mut undated = rec("undated", Some("blocked_handoff"));
        undated.started_at.clear();
        undated.ended_at.clear();
        turns.push(undated);
        turns.push(rec("no-hit", None));
        assert_eq!(
            call(&app, post("key-jesse", &turns)).await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &app,
                post(
                    "key-other",
                    &[rec("private-other", Some("blocked_handoff"))]
                )
            )
            .await
            .0,
            StatusCode::OK
        );
        let path = "/v1/signals/stats?signal=blocked_handoff";
        let (code, body) = call(&app, get("key-jesse", path)).await;
        assert_eq!(code, StatusCode::OK);
        let stats: signals::stats::SignalStats =
            serde_json::from_value(body["stats"].clone()).unwrap();
        assert_eq!(
            (
                stats.received_turns,
                stats.matched_turns,
                stats.undated_matches
            ),
            (127, 126, 1)
        );
        assert_eq!(stats.by_day.len(), 30);
        assert_eq!(stats.by_agent.len(), 12);
        assert_eq!(stats.by_project.len(), 12);
        assert_eq!(
            stats
                .by_agent
                .iter()
                .map(|b| b.matched_turns)
                .sum::<usize>()
                + stats.other_agent_matches,
            126
        );
        assert_eq!(
            stats
                .by_project
                .iter()
                .map(|b| b.matched_turns)
                .sum::<usize>()
                + stats.other_project_matches,
            126
        );
        assert_eq!(
            stats.by_day.iter().map(|b| b.matched_turns).sum::<usize>()
                + stats.other_day_matches
                + stats.undated_matches,
            126
        );
        assert!(!body.to_string().contains("literal%_needle"));
        assert!(!body.to_string().contains("private-other"));
        let scoped = "signal=blocked_handoff&agent=agent-0&project=project-0&since=2026-09-01T00:00:00Z&until=2026-09-02T00:00:00Z";
        let (code, body) = call(
            &app,
            get("key-jesse", &format!("/v1/signals/stats?{scoped}")),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["stats"]["matched_turns"], 2);
        assert_eq!(body["stats"]["received_turns"], 2);
        let (_, evidence) = call(&app, get("key-jesse", &format!("/v1/turns?{scoped}"))).await;
        assert_eq!(evidence["turns"].as_array().unwrap().len(), 2);
        let (_, literal) = call(
            &app,
            get(
                "key-jesse",
                &format!("/v1/signals/stats?{scoped}&search=literal%25_needle"),
            ),
        )
        .await;
        assert_eq!(literal["stats"]["matched_turns"], 1);
        let (_, empty) = call(
            &app,
            get(
                "key-jesse",
                "/v1/signals/stats?signal=blocked_handoff&since=2030-01-01T00:00:00Z",
            ),
        )
        .await;
        assert_eq!(empty["stats"]["received_turns"], 0);
        assert!(empty["stats"]["by_day"].as_array().unwrap().is_empty());
        let (_, other) = call(&app, get("key-other", path)).await;
        assert_eq!(other["stats"]["matched_turns"], 1);
        assert_eq!(
            call(
                &app,
                post("key-other", &[rec("historic", Some("mood café"))])
            )
            .await
            .0,
            StatusCode::OK
        );
        let mut unicode = reqwest::Url::parse("http://fixture.invalid/v1/signals/stats").unwrap();
        unicode.query_pairs_mut().append_pair("signal", "mood café");
        let (code, old) = call(
            &app,
            get(
                "key-other",
                &format!("{}?{}", unicode.path(), unicode.query().unwrap()),
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(old["stats"]["matched_turns"], 1);
        assert_eq!(
            call(&app, get("wrong", path)).await.0,
            StatusCode::UNAUTHORIZED
        );
        turns[0].signals.clear();
        assert_eq!(
            call(&app, post("key-jesse", &turns[..1])).await.0,
            StatusCode::OK
        );
        drop(app);
        let app = env.app().await;
        let (_, after) = call(&app, get("key-jesse", path)).await;
        assert_eq!(after["stats"]["matched_turns"], 125);
        assert_eq!(after["stats"]["received_turns"], 127);
        for query in [
            "signal=blocked_handoff&devspace=other",
            "signal=blocked_handoff&cursor=abc",
            "signal=blocked_handoff&since=not-a-date",
            "signal=blocked_handoff&since=2026-10-28T00:00:00Z&until=2026-10-27T00:00:00Z",
            "signal=",
        ] {
            assert_eq!(
                call(
                    &app,
                    get("key-jesse", &format!("/v1/signals/stats?{query}"))
                )
                .await
                .0,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            call(&app, get("key-jesse", "/v1/turns?until=bad")).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn signal_settings_round_trip_and_reject_bad_values() {
    for b in backends() {
        let env = Env::new(b, "settings");
        let app = env.app().await;
        let (_, body) = call(&app, get("key-jesse", "/v1/signals/settings")).await;
        assert_eq!(
            body["settings"]["disabled"].as_array().unwrap().len(),
            0,
            "{b:?}"
        );
        let mut want = serde_json::json!({
            "disabled": ["mood"],
            "options": { "tool_error_streak": { "min_streak": 5.0 } }
        });
        let (code, _) = call(&app, put("key-jesse", "/v1/signals/settings", want.clone())).await;
        assert_eq!(code, StatusCode::OK, "{b:?}");
        want["revision"] = 1.into();
        want["custom"] = serde_json::json!([]);
        let (_, body) = call(&app, get("key-jesse", "/v1/signals/settings")).await;
        assert_eq!(body["settings"], want, "{b:?}");
        // Settings are per devspace.
        let (_, body) = call(&app, get("key-other", "/v1/signals/settings")).await;
        assert_eq!(
            body["settings"]["disabled"].as_array().unwrap().len(),
            0,
            "{b:?}"
        );
        // The summary carries them.
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["settings"], want, "{b:?}");
        for bad in [
            serde_json::json!({ "disabled": ["nope"] }),
            serde_json::json!({ "options": { "tool_error_streak": { "min_streak": 999 } } }),
        ] {
            let (code, _) = call(&app, put("key-jesse", "/v1/signals/settings", bad)).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{b:?}");
        }
        let (code, _) = call(
            &app,
            Request::put("/v1/signals/settings")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{b:?}");
    }
}

#[tokio::test]
async fn catalogue_is_public_and_grouped_by_category() {
    let env = Env::new(Backend::Jsonl, "catalogue");
    let app = env.app().await;
    let (code, body) = call(
        &app,
        Request::get("/v1/signals").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["schema"], SIGNALS_SCHEMA);
    let grouped: usize = body["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["signals"].as_array().unwrap().len())
        .sum();
    assert_eq!(grouped, CATALOGUE.len());
    let streak = body["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "tool_error_streak")
        .unwrap();
    assert_eq!(streak["category"], "failure");
    assert_eq!(streak["options"][0]["name"], "min_streak");
}

#[tokio::test]
async fn custom_signals_preview_ingest_pause_and_persist_per_owner() {
    for b in backends() {
        let env = Env::new(b, "custom");
        let app = env.app().await;
        let rule = serde_json::json!({"id":"custom_test_phrase","name":"Completion phrase","group":"Quality","enabled":true,"agent":"codex","project":"shippinai","condition":{"kind":"response_contains","text":"all done"}});
        let mut t = rec("first", None);
        t.content = Some(rroflow_core::Content {
            input: "Check this".into(),
            output: "It is ALL DONE now.".into(),
        });
        let missing = rec("missing", None);
        let mut other_agent = t.clone();
        other_agent.event_id = "claude".into();
        other_agent.agent = "claude-code".into();
        assert_eq!(
            call(&app, post("key-jesse", &[t.clone(), missing, other_agent]))
                .await
                .0,
            StatusCode::OK
        );
        let (status, preview) = call(
            &app,
            json_post("key-jesse", "/v1/signals/preview", rule.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(preview["evaluated"], 1);
        let (_, identity) = call(&app, get("key-jesse", "/v1/whoami")).await;
        assert_eq!(preview["devspace"], identity["devspace"]);
        assert_eq!(preview["unavailable"], 1);
        assert_eq!(preview["matches"].as_array().unwrap().len(), 1);
        assert_eq!(
            call(&app, json_post("", "/v1/signals/preview", rule.clone()))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(
                &app,
                json_post("key-other", "/v1/signals/preview", rule.clone())
            )
            .await
            .1["scanned"],
            0
        );
        let config = serde_json::json!({"revision":0,"custom":[rule.clone()]});
        let (status, saved) = call(
            &app,
            put("key-jesse", "/v1/signals/settings", config.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(saved["settings"]["revision"], 1);
        // Saving does not silently rewrite the already captured turn.
        assert_eq!(
            call(
                &app,
                get("key-jesse", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"],
            serde_json::json!([])
        );
        t.event_id = "new-arrival".into();
        call(&app, post("key-jesse", &[t.clone()])).await;
        let (_, hits) = call(
            &app,
            get("key-jesse", "/v1/turns?signal=custom_test_phrase"),
        )
        .await;
        assert_eq!(hits["turns"].as_array().unwrap().len(), 1);
        assert_eq!(hits["turns"][0]["signals"][0]["rule_version"], 1);
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", config))
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut changed = saved["settings"].clone();
        changed["custom"][0]["condition"]["text"] = "different".into();
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", changed))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        let mut paused = saved["settings"].clone();
        paused["custom"][0]["enabled"] = false.into();
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", paused))
                .await
                .0,
            StatusCode::OK
        );
        let restarted = env.app().await;
        assert_eq!(
            call(&restarted, get("key-jesse", "/v1/signals/settings"))
                .await
                .1["settings"]["custom"][0]["enabled"],
            false
        );
        t.event_id = "after-pause".into();
        call(&restarted, post("key-jesse", &[t])).await;
        assert_eq!(
            call(
                &restarted,
                get("key-jesse", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // Clients cannot inject another owner's/custom namespace findings.
        call(
            &restarted,
            post("key-other", &[rec("forged", Some("custom_test_phrase"))]),
        )
        .await;
        assert_eq!(
            call(
                &restarted,
                get("key-other", "/v1/turns?signal=custom_test_phrase")
            )
            .await
            .1["turns"],
            serde_json::json!([])
        );
        assert_eq!(
            call(&restarted, get("key-other", "/v1/signals/settings"))
                .await
                .1["settings"]["custom"],
            serde_json::json!([])
        );
    }
}

#[tokio::test]
async fn custom_rule_bounds_metadata_and_concurrent_settings_are_checked() {
    use rroflow_core::custom::{CustomSignal, Evaluation};
    for b in backends() {
        let env = Env::new(b, "custom-bounds");
        let app = env.app().await;
        let rule = serde_json::json!({"id":"custom_test_errors","name":"Tool failures","group":"Quality","enabled":true,"condition":{"kind":"tool_errors","at_least":2}});
        let mut t = rec("tools", None);
        t.tools = (0..3)
            .map(|i| rroflow_core::ToolRecord {
                id: format!("tool-{i}"),
                name: "TestRunner".into(),
                status: if i < 2 {
                    rroflow_core::ToolStatus::Error
                } else {
                    rroflow_core::ToolStatus::Ok
                },
                started_at: t.started_at.clone(),
                ended_at: t.ended_at.clone(),
                input: None,
                output: None,
            })
            .collect();
        let parsed: CustomSignal = serde_json::from_value(rule.clone()).unwrap();
        assert!(matches!(parsed.evaluate(&t), Evaluation::Match(_)));
        let mut named = rule.clone();
        named["condition"] = serde_json::json!({"kind":"tool_named","name":"testrunner"});
        assert!(matches!(
            serde_json::from_value::<CustomSignal>(named)
                .unwrap()
                .evaluate(&t),
            Evaluation::Match(_)
        ));
        let mut scoped = parsed.clone();
        scoped.project = Some("different".into());
        assert!(matches!(scoped.evaluate(&t), Evaluation::OutOfScope));
        let mut cfg = serde_json::json!({"revision":0,"custom":[rule.clone()]});
        let (a, b) = tokio::join!(
            call(&app, put("key-jesse", "/v1/signals/settings", cfg.clone())),
            call(&app, put("key-jesse", "/v1/signals/settings", cfg.clone()))
        );
        assert!(
            (a.0 == StatusCode::OK && b.0 == StatusCode::CONFLICT)
                || (b.0 == StatusCode::OK && a.0 == StatusCode::CONFLICT)
        );
        cfg["revision"] = 1.into();
        cfg["custom"] = serde_json::json!([]);
        assert_eq!(
            call(&app, put("key-jesse", "/v1/signals/settings", cfg))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        for condition in [
            serde_json::json!({"kind":"tool_errors","at_least":0}),
            serde_json::json!({"kind":"prompt_contains","text":""}),
            serde_json::json!({"kind":"response_contains","text":"x".repeat(161)}),
        ] {
            let mut invalid = rule.clone();
            invalid["condition"] = condition;
            assert_eq!(
                call(&app, json_post("key-jesse", "/v1/signals/preview", invalid))
                    .await
                    .0,
                StatusCode::BAD_REQUEST
            );
        }
    }
}

#[tokio::test]
async fn settings_migration_preserves_old_choices_and_refuses_old_writers() {
    if std::env::var(PG_ENV).is_err() {
        return;
    }
    let env = Env::new(Backend::Postgres, "settings-migration");
    let Store::Postgres(pg) = env.cfg.open_store().await.unwrap() else {
        unreachable!()
    };
    let mut tx = pg.pool.begin().await.unwrap();
    // pg_temp shadows the real table on this transaction's connection only.
    sqlx::query("CREATE TEMP TABLE signal_settings (config JSONB NOT NULL) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO signal_settings VALUES ($1)")
        .bind(sqlx::types::Json(
            serde_json::json!({"disabled":["mood"],"options":{}}),
        ))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!(
        "../../migrations/0004_signal_settings_revisions.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    let normalized: sqlx::types::Json<serde_json::Value> =
        sqlx::query_scalar("SELECT config FROM signal_settings")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(normalized.0["revision"], 0);
    assert_eq!(normalized.0["custom"], serde_json::json!([]));
    assert_eq!(normalized.0["disabled"], serde_json::json!(["mood"]));
    let error = sqlx::query(
        "INSERT INTO signal_settings VALUES ('{\"disabled\":[],\"options\":{}}'::jsonb)",
    )
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
}
