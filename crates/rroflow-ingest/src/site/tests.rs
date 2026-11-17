//! Site tests: the public page, the original dashboard, branding and assets.
use crate::tests::*;

#[tokio::test]
async fn brand_configuration_is_instance_scoped_and_never_opens_member_data() {
    let mut env = Env::new(Backend::Jsonl, "brand");
    env.cfg.brand = BrandConfig {
        name: "Orbit AI".into(),
        accent: "#124b70".into(),
        highlight: "#b7e7ff".into(),
        ..Default::default()
    };
    let custom = env.app().await;
    let default = Env::new(Backend::Jsonl, "default_brand").app().await;
    for (app, name, color) in [
        (&custom, "Orbit AI", "#124b70"),
        (&default, "RROFLOW", "#36634e"),
    ] {
        for path in ["/", "/app", "/app/"] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let csp = response.headers()["content-security-policy"]
                .to_str()
                .unwrap();
            assert!(csp.contains("script-src 'self'; style-src 'self'"));
            assert!(!csp.contains("unsafe-inline"));
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let html = std::str::from_utf8(&body).unwrap();
            assert!(html.contains(name));
            assert!(html.contains("/assets/brand.css"));
        }
        let response = app
            .clone()
            .oneshot(
                Request::get("/assets/brand.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.headers()["content-type"],
            "text/css; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&body).unwrap().contains(color));
        let (code, _) = call(
            app,
            Request::get("/v1/summary").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
    }
    env.cfg.brand.accent = "#ffffff".into();
    let store = Arc::new(env.cfg.open_store().await.unwrap());
    assert!(AppState::new(&env.cfg, store).is_err());
}

#[tokio::test]
async fn public_product_entry_is_separate_from_member_data() {
    let app = Env::new(Backend::Jsonl, "public").app().await;
    for (path, needle) in [
        ("/", "See the whole story"),
        ("/assets/public.js", "/auth/config"),
        ("/assets/public.css", ".public-hero"),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(needle));
        assert!(!text.contains("/assets/dashboard.js"));
    }
    let (code, config) = call(
        &app,
        Request::get("/auth/config").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        config,
        serde_json::json!({"enabled":false,"member_origin":null,"session_storage":null})
    );
    let (code, _) = call(
        &app,
        Request::get("/v1/whoami").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_is_served_without_a_key_and_reads_only_with_one() {
    let app = Env::new(Backend::Jsonl, "dash").app().await;
    let resp = app
        .clone()
        .oneshot(Request::get("/app").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("connect-src 'self'")
    );
    let html = resp.into_body().collect().await.unwrap().to_bytes();
    assert!(
        std::str::from_utf8(&html)
            .unwrap()
            .contains("/assets/dashboard.js")
    );
    for (path, kind, needle) in [
        ("/assets/dashboard.js", "text/javascript", "/v1/summary"),
        ("/assets/dashboard.css", "text/css", "@media"),
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with(kind)
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&bytes).unwrap().contains(needle));
    }
    let (code, _) = call(
        &app,
        Request::get("/v1/summary").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, _) = call(
        &app,
        Request::get("/assets/research.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    let (code, notes) = call(&app, get("key-jesse", "/assets/research.json")).await;
    assert_eq!(code, StatusCode::OK);
    assert!(notes.as_array().unwrap().iter().any(|note| note["path"]
        == "docs/research/raindrop/ux-settings.md"
        && !note["text"].as_str().unwrap().is_empty()));
}

/// The embedded assets share one helper. Each keeps its content type, `nosniff`, the headers the
/// browser-session layer adds to every response, and its exact bytes, as before.
#[tokio::test]
async fn embedded_assets_keep_their_type_nosniff_and_exact_bytes() {
    let app = Env::new(Backend::Jsonl, "assets").app().await;
    let css = "text/css; charset=utf-8";
    let js = "text/javascript; charset=utf-8";
    for (path, content_type, body) in [
        ("/assets/public.css", css, include_str!("../public.css")),
        (
            "/assets/dashboard.css",
            css,
            include_str!("../dashboard.css"),
        ),
        ("/assets/public.js", js, include_str!("../public.js")),
        ("/assets/dashboard.js", js, include_str!("../dashboard.js")),
        (
            "/assets/dashboard-a2ui.js",
            js,
            include_str!("../dashboard-a2ui.js"),
        ),
        (
            "/assets/dashboard-datasets.js",
            js,
            include_str!("../dashboard-datasets.js"),
        ),
        (
            "/assets/dashboard-sdk.js",
            js,
            include_str!("../dashboard-sdk.js"),
        ),
    ] {
        let resp = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
        let headers = resp.headers().clone();
        assert_eq!(headers.get_all("content-type").iter().count(), 1, "{path}");
        assert_eq!(headers["content-type"], content_type, "{path}");
        assert_eq!(headers["x-content-type-options"], "nosniff", "{path}");
        assert_eq!(headers["cache-control"], "no-store", "{path}");
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(bytes, body.as_bytes(), "{path}");
    }
}
