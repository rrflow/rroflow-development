//! Server tests: configuration checks and Nomen sign-in.
use crate::tests::*;

#[tokio::test]
async fn config_rejects_unsafe_ids_and_shared_keys() {
    let store = Arc::new(
        Env::new(Backend::Jsonl, "cfg")
            .cfg
            .open_store()
            .await
            .unwrap(),
    );
    let bad = Config {
        devspace: vec![Devspace {
            id: "../etc".into(),
            key_sha256: vec![],
        }],
        ..Config::default()
    };
    assert!(AppState::new(&bad, store.clone()).is_err());
    let h = hash_key("k");
    let shared = Config {
        devspace: vec![
            Devspace {
                id: "a".into(),
                key_sha256: vec![h.clone()],
            },
            Devspace {
                id: "b".into(),
                key_sha256: vec![h],
            },
        ],
        ..Config::default()
    };
    assert!(AppState::new(&shared, store).is_err());
}

/// A Postgres-backed service with Nomen sign-in pointed at `fake`.
async fn nomen_app(env: &mut Env, fake: &FakeNomen, dir: &std::path::Path) -> Router {
    let key = crate::test_keys::rsa();
    let path = dir.join("nomen-key.json");
    std::fs::write(
        &path,
        serde_json::json!({"type": "application", "keyId": "k1", "key": key.pem, "clientId": "rroflow-ingest@shippin"})
            .to_string(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    env.cfg.nomen = Some(NomenConfig {
        issuer: fake.issuer.clone(),
        audiences: vec![NOMEN_PROJECT.into()],
        clients: vec![NOMEN_CLIENT.into()],
        key_file: path.to_string_lossy().into(),
    });
    env.app().await
}

/// A Nomen-style numeric account ID unique to this run.
fn sub_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(
        "{}",
        300_000_000_000_000_000u128 + (nanos % 99_999_999_999_999_999)
    )
}

#[tokio::test]
async fn nomen_sign_in_opens_the_accounts_own_devspace() {
    if std::env::var(PG_ENV).is_err() {
        eprintln!("{PG_ENV} not set: Nomen sign-in tests skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_nomen().await;
    let (alice, bob) = (sub_id(), sub_id());
    {
        let mut a = fake.answers.lock().unwrap();
        a.insert("tok-alice".into(), signed_in(&fake.issuer, &alice));
        a.insert("tok-bob".into(), signed_in(&fake.issuer, &bob));
        let mut other_client = signed_in(&fake.issuer, &alice);
        other_client["client_id"] = "some-tenant-app".into();
        a.insert("tok-other-client".into(), other_client);
    }
    let mut env = Env::new(Backend::Postgres, "nomen");
    let app = nomen_app(&mut env, &fake, dir.path()).await;

    // First sign-in creates the devspace, named by the account ID.
    let (code, who) = call(&app, get("tok-alice", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::OK, "{who}");
    assert_eq!(who["devspace"].as_str(), Some(alice.as_str()));
    assert_eq!(who["via"], "nomen");
    assert_eq!(who["org"], "300000000000000900");
    let (code, body) = call(&app, post("tok-alice", &[rec("a", Some("mood"))])).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["devspace"].as_str(), Some(alice.as_str()));

    // Each account sees only its own devspace; keys keep working beside sign-in.
    let (_, body) = call(&app, get("tok-bob", "/v1/turns")).await;
    assert_eq!(body["turns"].as_array().unwrap().len(), 0);
    let (_, who) = call(&app, get("key-jesse", "/v1/whoami")).await;
    assert_eq!(
        (who["devspace"].as_str(), who["via"].as_str()),
        (Some(env.jesse.as_str()), Some("key"))
    );

    // Refusals are 401; a token from an app rroflow does not accept is refused.
    for bad in ["tok-unknown", "tok-other-client"] {
        let (code, _) = call(&app, get(bad, "/v1/whoami")).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{bad}");
    }

    // Claim: a key-registered devspace becomes bob's, and bob's sign-in opens it. Bob's
    // devspace from signing in above is empty, so it gives way; alice's holds a turn.
    let Store::Postgres(pg) = env.cfg.open_store().await.unwrap() else {
        unreachable!()
    };
    let bob_space = env.other.clone();
    pg.claim_devspace(&bob_space, &fake.issuer, &bob)
        .await
        .unwrap();
    pg.claim_devspace(&bob_space, &fake.issuer, &bob)
        .await
        .unwrap(); // idempotent
    assert!(
        pg.claim_devspace(&bob_space, &fake.issuer, &alice)
            .await
            .is_err(),
        "already owned"
    );
    assert!(
        pg.claim_devspace(&env.jesse, &fake.issuer, &alice)
            .await
            .is_err(),
        "alice already owns a devspace with turns"
    );
    let fresh = nomen_app(&mut env, &fake, dir.path()).await; // new verifier cache
    let (_, who) = call(&fresh, get("tok-bob", "/v1/whoami")).await;
    assert_eq!(who["devspace"].as_str(), Some(bob_space.as_str()));

    // Nomen down: 503 issuer_unavailable, never 401; keys still work.
    fake.status.store(503, Ordering::SeqCst);
    let down = nomen_app(&mut env, &fake, dir.path()).await;
    let (code, body) = call(&down, get("tok-alice", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .starts_with("issuer_unavailable")
    );
    let (code, _) = call(&down, get("key-jesse", "/v1/whoami")).await;
    assert_eq!(code, StatusCode::OK);
}

#[tokio::test]
async fn nomen_needs_postgres() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_nomen().await;
    let mut env = Env::new(Backend::Jsonl, "nomen-jsonl");
    env.cfg.nomen = Some(NomenConfig {
        issuer: fake.issuer.clone(),
        audiences: vec![NOMEN_PROJECT.into()],
        clients: vec![NOMEN_CLIENT.into()],
        key_file: dir.path().join("missing.json").to_string_lossy().into(),
    });
    let store = Arc::new(env.cfg.open_store().await.unwrap());
    let err = AppState::new(&env.cfg, store).err().unwrap().to_string();
    assert!(err.contains("database_url"), "{err}");
}

#[test]
fn account_ids_that_are_not_valid_devspace_ids_get_a_digest() {
    assert_eq!(
        crate::pg::owner_devspace_id("https://id", "300000000000000001"),
        "300000000000000001"
    );
    let odd = crate::pg::owner_devspace_id("https://id", "user:with/odd.chars");
    assert!(
        odd.starts_with("n-") && crate::valid_devspace_id(&odd),
        "{odd}"
    );
    assert_ne!(
        odd,
        crate::pg::owner_devspace_id("https://other", "user:with/odd.chars")
    );
}
