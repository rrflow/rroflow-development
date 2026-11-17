//! Turn tests: receiving, replacing, paging, searching and refusing records.
use crate::tests::*;

#[tokio::test]
async fn project_and_session_filters_apply_before_limit_and_remain_tenant_scoped() {
    for backend in backends() {
        let env = Env::new(backend, "navigation");
        let app = env.app().await;
        let mut old = rec("old", None);
        old.project = None;
        old.cwd = "/project-a".into();
        old.session_id = "session-a".into();
        let mut newer = rec("newer", None);
        newer.project = Some("project-b".into());
        newer.session_id = "session-b".into();
        assert_eq!(
            call(&app, post("key-jesse", &[old.clone(), newer])).await.0,
            StatusCode::OK
        );
        let (status, body) = call(
            &app,
            get(
                "key-jesse",
                "/v1/turns?project=%2Fproject-a&session=session-a&limit=1",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["turns"].as_array().unwrap().len(), 1);
        assert_eq!(body["turns"][0]["event_id"], old.event_id);
        let (_, empty) = call(
            &app,
            get("key-jesse", "/v1/turns?project=project-b&session=session-a"),
        )
        .await;
        assert_eq!(empty["turns"], serde_json::json!([]));
        let (_, other) = call(&app, get("key-other", "/v1/turns?project=%2Fproject-a")).await;
        assert_eq!(other["turns"], serde_json::json!([]));
        let (_, sum) = call(&app, get("key-jesse", "/v1/summary")).await;
        assert_eq!(sum["summary"]["by_project"]["/project-a"], 1);
    }
}

#[tokio::test]
async fn key_decides_the_devspace_and_resends_replace() {
    for b in backends() {
        let env = Env::new(b, "devspace");
        let app = env.app().await;
        let (code, body) = call(
            &app,
            post(
                "key-jesse",
                &[rec("a", Some("blocked_handoff")), rec("bb", None)],
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{b:?}: {body}");
        assert_eq!(body["devspace"].as_str(), Some(env.jesse.as_str()), "{b:?}");
        assert_eq!(body["new"], 2, "{b:?}");
        // Re-send with a changed signal: replaced, and the old hit is gone.
        let (_, body) = call(&app, post("key-jesse", &[rec("a", Some("mood"))])).await;
        assert_eq!(
            (body["new"].as_u64(), body["replaced"].as_u64()),
            (Some(0), Some(1)),
            "{b:?}"
        );
        // Another devspace sees nothing of jesse's.
        let (_, body) = call(&app, get("key-other", "/v1/turns")).await;
        assert_eq!(body["devspace"].as_str(), Some(env.other.as_str()));
        assert_eq!(body["turns"].as_array().unwrap().len(), 0, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?signal=mood")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?signal=blocked_handoff")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 0, "{b:?}");
        // Agent filter.
        let mut claude = rec("ccc", None);
        claude.agent = "claude-code".into();
        call(&app, post("key-jesse", &[claude])).await;
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?agent=claude-code")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns?agent=codex&signal=mood")).await;
        assert_eq!(body["turns"].as_array().unwrap().len(), 1, "{b:?}");
        // Newest first.
        let (_, body) = call(&app, get("key-jesse", "/v1/turns")).await;
        assert_eq!(body["turns"][0]["event_id"], "ccc", "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        let s = &body["summary"];
        assert_eq!(s["turns"], 3, "{b:?}");
        assert_eq!(s["signals"]["mood"], 1, "{b:?}");
        assert!(s["signals"].get("blocked_handoff").is_none(), "{b:?}");
        assert_eq!(
            (s["input_tokens"].as_u64(), s["cache_read_tokens"].as_u64()),
            (Some(300), Some(150)),
            "{b:?}"
        );
        assert_eq!(s["by_agent"]["codex"], 2, "{b:?}");
        assert_eq!(s["signals_by_agent"]["codex"]["mood"], 1, "{b:?}");
        // Data survives a restart.
        let app2 = env.app().await;
        let (_, body) = call(&app2, get("key-jesse", "/v1/summary")).await;
        assert_eq!(body["summary"]["turns"], 3, "{b:?}");
    }
}

#[tokio::test]
async fn duplicate_event_ids_in_one_batch_keep_the_last() {
    for b in backends() {
        let env = Env::new(b, "dup");
        let app = env.app().await;
        let (code, body) = call(
            &app,
            post(
                "key-jesse",
                &[rec("a", None), rec("a", Some("aborted_turn"))],
            ),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{b:?}: {body}");
        assert_eq!(body["new"], 1, "{b:?}");
        let (_, body) = call(&app, get("key-jesse", "/v1/turns")).await;
        let turns = body["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 1, "{b:?}");
        assert_eq!(turns[0]["signals"][0]["signal"], "aborted_turn", "{b:?}");
    }
}

#[tokio::test]
async fn unreported_usage_is_counted_not_priced_as_zero() {
    for b in backends() {
        let env = Env::new(b, "usage");
        let app = env.app().await;
        let mut r = rec("a", None);
        r.usage.input_tokens = None;
        call(&app, post("key-jesse", &[r, rec("bb", None)])).await;
        let (_, body) = call(&app, get("key-jesse", "/v1/summary")).await;
        let s = &body["summary"];
        assert_eq!(
            (
                s["turns_without_usage"].as_u64(),
                s["input_tokens"].as_u64()
            ),
            (Some(1), Some(100)),
            "{b:?}"
        );
    }
}

#[tokio::test]
async fn turn_pages_keep_timestamp_ties_and_undated_records_without_duplicates() {
    for backend in backends() {
        let app = Env::new(backend, "pages").app().await;
        let mut turns: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|id| rec(id, None))
            .collect();
        turns[0].started_at = "2026-10-27T00:00:00Z".into();
        turns[1].started_at = "2026-10-26T20:00:00-04:00".into();
        turns[2].started_at = "2026-10-26T20:00:00Z".into();
        turns[3].started_at.clear();
        turns[4].started_at.clear();
        assert_eq!(
            call(&app, post("key-jesse", &turns)).await.0,
            StatusCode::OK
        );
        let mut url = "/v1/turns?limit=1".to_string();
        let mut found = Vec::new();
        for _ in 0..6 {
            let (status, body) = call(&app, get("key-jesse", &url)).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["turns"].as_array().unwrap().len(), 1);
            found.push(body["turns"][0]["event_id"].as_str().unwrap().to_string());
            if let Some(cursor) = body["next_cursor"].as_str() {
                url = format!("/v1/turns?limit=1&cursor={cursor}");
                let (_, other) = call(&app, get("key-other", &url)).await;
                assert_eq!(other["turns"], serde_json::json!([]));
            } else {
                break;
            }
        }
        assert_eq!(found, ["a", "b", "c", "d", "e"]);
    }
}

#[tokio::test]
async fn turn_search_is_literal_date_filtered_and_exact_lookup_is_tenant_scoped() {
    for backend in backends() {
        let app = Env::new(backend, "search").app().await;
        let mut a = rec("a", Some("blocked_handoff"));
        a.model = Some("Model-%_One".into());
        a.started_at = "2026-10-27T00:00:00Z".into();
        let mut b = rec("b", None);
        b.model = Some("other".into());
        b.started_at = "2026-10-20T00:00:00Z".into();
        call(&app, post("key-jesse", &[a, b])).await;
        for query in [
            "search=%25_",
            "search=MODEL-%25_ONE",
            "since=2026-10-26T00%3A00%3A00Z",
            "event=a",
        ] {
            let (status, body) = call(
                &app,
                get("key-jesse", &format!("/v1/turns?{query}&limit=1")),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["turns"][0]["event_id"], "a");
            assert!(body["next_cursor"].is_null());
            let (_, other) = call(&app, get("key-other", &format!("/v1/turns?{query}"))).await;
            assert_eq!(other["turns"], serde_json::json!([]));
        }
        let (_, body) = call(
            &app,
            get("key-jesse", "/v1/turns?search=other&signal=blocked_handoff"),
        )
        .await;
        assert_eq!(body["turns"], serde_json::json!([]));
    }
}

#[tokio::test]
async fn invalid_turn_navigation_is_rejected_instead_of_silently_widening_the_view() {
    let app = Env::new(Backend::Jsonl, "bad-nav").app().await;
    for query in ["cursor=bad", "since=yesterday", "seach=typo"] {
        assert_eq!(
            call(&app, get("key-jesse", &format!("/v1/turns?{query}")))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    let long = "x".repeat(201);
    assert_eq!(
        call(&app, get("key-jesse", &format!("/v1/turns?search={long}")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn unknown_key_wrong_schema_and_bad_timestamps_are_refused() {
    for b in backends() {
        let app = Env::new(b, "refuse").app().await;
        let (code, _) = call(&app, post("nope", &[rec("a", None)])).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let mut bad = rec("a", None);
        bad.schema = "something/v0".into();
        let (code, body) = call(&app, post("key-jesse", &[bad])).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("schema"));
        let mut bad = rec("a", None);
        bad.started_at = "yesterday".into();
        let (code, body) = call(&app, post("key-jesse", &[bad])).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("started_at"));
    }
}
