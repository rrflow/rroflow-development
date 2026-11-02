//! Tenancy against real Postgres, each test in a private schema. The unit tests at the end run
//! everywhere; the rest skip themselves without `RROFLOW_TEST_DATABASE_URL` (CI sets it).
use super::*;
use crate::tests::{
    FakeNomen, NOMEN_CLIENT, NOMEN_PROJECT, call, fake_nomen, rec, signed_in, source_health_report,
};
use crate::{Config, Devspace, NomenConfig, allowances, router};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;

const PG_ENV: &str = "RROFLOW_TEST_DATABASE_URL";
const OPERATOR_KEY: &str = "operator-registered-key";

struct Fixture {
    app: Router,
    store: Arc<Store>,
    fake: Arc<FakeNomen>,
    admin: sqlx::PgPool,
    schema: String,
    _dir: tempfile::TempDir,
}

fn private_schema_url(base: &str, schema: &str) -> String {
    let mut url = reqwest::Url::parse(base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    url.to_string()
}

async fn fixture(tenancy: TenancyConfig) -> Option<Fixture> {
    let Ok(base) = std::env::var(PG_ENV) else {
        eprintln!("{PG_ENV} not set: tenancy tests skipped");
        return None;
    };
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    // Identifier is a fixed prefix plus a locally generated u64, never external input.
    let schema = format!("tenancy_fixture_{}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_nomen().await;
    let key = crate::test_keys::rsa();
    let key_file = dir.path().join("nomen-key.json");
    std::fs::write(
        &key_file,
        json!({"type":"application","keyId":"k1","key":key.pem,"clientId":"rroflow-ingest@shippin"})
            .to_string(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let cfg = Config {
        database_url: Some(private_schema_url(&base, &schema)),
        db_max_connections: 8,
        nomen: Some(NomenConfig {
            issuer: fake.issuer.clone(),
            audiences: vec![NOMEN_PROJECT.into()],
            clients: vec![NOMEN_CLIENT.into()],
            key_file: key_file.to_string_lossy().into(),
        }),
        devspace: vec![Devspace {
            id: "operator".into(),
            key_sha256: vec![hash_key(OPERATOR_KEY)],
        }],
        tenancy,
        ..Config::default()
    };
    let store = Arc::new(cfg.open_store().await.unwrap());
    let app = router(
        AppState::new(&cfg, store.clone()).unwrap(),
        cfg.max_body_bytes,
    );
    Some(Fixture {
        app,
        store,
        fake,
        admin,
        schema,
        _dir: dir,
    })
}

fn request(
    method: &str,
    token: &str,
    uri: &str,
    body: Option<&Value>,
    project: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    if let Some(project) = project {
        builder = builder.header(PROJECT_HEADER, project);
    }
    match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

impl Fixture {
    fn pg(&self) -> &PgStore {
        let Store::Postgres(pg) = self.store.as_ref() else {
            unreachable!()
        };
        pg
    }
    /// A new person's access token. Their Nomen subject is the part after `tok-`.
    fn person(&self) -> String {
        let subject = (300_000_000_000_000_000u64 + u64::from(rand::random::<u32>())).to_string();
        let token = format!("tok-{subject}");
        self.fake
            .answers
            .lock()
            .unwrap()
            .insert(token.clone(), signed_in(&self.fake.issuer, &subject));
        token
    }
    async fn send(
        &self,
        method: &str,
        token: &str,
        uri: &str,
        body: Option<&Value>,
        project: Option<&str>,
    ) -> (StatusCode, Value) {
        call(&self.app, request(method, token, uri, body, project)).await
    }
    async fn get(&self, token: &str, uri: &str) -> (StatusCode, Value) {
        self.send("GET", token, uri, None, None).await
    }
    async fn post(&self, token: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        self.send("POST", token, uri, Some(&body), None).await
    }
    async fn user_id(&self, token: &str) -> String {
        let (code, me) = self.get(token, "/v1/me").await;
        assert_eq!(code, StatusCode::OK, "{me}");
        me["user"]["id"].as_str().unwrap().to_string()
    }
    /// Remove the private schema. Skipped when a test fails, which leaves it to inspect.
    async fn finish(self) {
        self.pg().pool.close().await;
        // Identifier is the locally generated name from `fixture`.
        let drop = format!("DROP SCHEMA {} CASCADE", self.schema);
        sqlx::query(sqlx::AssertSqlSafe(drop))
            .execute(&self.admin)
            .await
            .unwrap();
    }
}

struct Team {
    owner: String,
    organization: String,
    project: String,
}

async fn team(fx: &Fixture, name: &str) -> Team {
    let owner = fx.person();
    let (code, organization) = fx
        .post(&owner, "/v1/organizations", json!({"name":name}))
        .await;
    assert_eq!(code, StatusCode::OK, "{organization}");
    let organization = organization["id"].as_str().unwrap().to_string();
    let (code, project) = fx
        .post(
            &owner,
            &format!("/v1/organizations/{organization}/projects"),
            json!({"name":"Main"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{project}");
    Team {
        owner,
        organization,
        project: project["id"].as_str().unwrap().to_string(),
    }
}

/// A new person who joined `team` with `role`, through a real invitation.
async fn join(fx: &Fixture, team: &Team, role: &str) -> String {
    let (code, invitation) = fx
        .post(
            &team.owner,
            &format!("/v1/organizations/{}/invitations", team.organization),
            json!({"role":role}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{invitation}");
    let person = fx.person();
    let (code, joined) = fx
        .post(
            &person,
            "/v1/invitations/accept",
            json!({"token":invitation["token"]}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{joined}");
    assert_eq!(joined["role"], role);
    person
}

async fn project_key(fx: &Fixture, token: &str, project: &str) -> (String, String) {
    let (code, key) = fx
        .post(
            token,
            &format!("/v1/projects/{project}/keys"),
            json!({"label":"CI"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{key}");
    (
        key["key"].as_str().unwrap().to_string(),
        key["id"].as_str().unwrap().to_string(),
    )
}

fn turns_body(id: &str) -> Value {
    json!({"turns":[rec(id, None)]})
}

/// Every route that delivers records or changes a project, with a body that passes its
/// extractor, so a refusal below proves the authorization check and not a parsing failure.
fn delivery_routes() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("POST", "/v1/turns", turns_body("route-probe")),
        ("POST", "/v1/events", json!({"events":[]})),
        ("POST", "/v1/sources/health", source_health_report(1)),
    ]
}
fn change_routes() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "PUT",
            "/v1/signals/settings",
            json!({"revision":0,"custom":[],"disabled":[],"options":{}}),
        ),
        (
            "POST",
            "/v1/reviews",
            json!({"review_id":"11111111-1111-4111-8111-111111111111","event_id":"missing","turn_digest":"missing","signal":"blocked_handoff","rule_version":1,"verdict":"not_match"}),
        ),
        (
            "POST",
            "/v1/sdk/reviews",
            json!({"review_id":"22222222-2222-4222-8222-222222222222","activity_id":"missing","revision":"missing","signal":"missing","rule_version":1,"verdict":"match"}),
        ),
    ]
}
fn dataset_route() -> (&'static str, &'static str, Value) {
    (
        "PUT",
        "/v1/datasets/policy",
        json!({"expected_revision":0,"policy":{"purpose":"signal_reviews","enabled":true,"consent":true,"agents":["codex"],"sdk_agents":[],"all_projects":false,"projects":["p"],"retention_days":7}}),
    )
}
const READ_ROUTES: &[&str] = &[
    "/v1/turns",
    "/v1/summary",
    "/v1/events",
    "/v1/reviews",
    "/v1/sdk/activity",
    "/v1/sdk/reviews",
    "/v1/signals/stats?signal=mood",
    "/v1/sources/health",
    "/v1/allowances",
    "/v1/datasets",
    "/assets/research.json",
];

#[tokio::test]
async fn first_sign_in_creates_one_account_and_keeps_the_existing_devspace_id() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let token = fx.person();
    let subject = token.strip_prefix("tok-").unwrap().to_string();
    let (code, who) = fx.get(&token, "/v1/whoami").await;
    assert_eq!(code, StatusCode::OK);
    // Existing collectors bind queues to this id, so it must not change.
    assert_eq!(who["devspace"].as_str(), Some(subject.as_str()));
    assert_eq!(who["via"], "nomen");
    assert_eq!(who["role"], "owner");
    assert!(who.get("write_only").is_none());

    let (_, me) = fx.get(&token, "/v1/me").await;
    assert_eq!(me["user"]["name"], "jesse@example.test");
    assert_eq!(me["default_project"].as_str(), Some(subject.as_str()));
    let organizations = me["organizations"].as_array().unwrap();
    assert_eq!(organizations.len(), 1);
    assert_eq!(organizations[0]["personal"], true);
    assert_eq!(organizations[0]["role"], "owner");
    assert_eq!(
        organizations[0]["projects"],
        json!([{"id":subject,"slug":"default","name":"Default project"}])
    );

    // Concurrent first sign-ins of one account create one account.
    let racer = fx.person();
    let subject = racer.strip_prefix("tok-").unwrap().to_string();
    let mut attempts = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let (app, racer) = (fx.app.clone(), racer.clone());
        attempts.spawn(async move {
            call(&app, request("GET", &racer, "/v1/whoami", None, None)).await
        });
    }
    while let Some(result) = attempts.join_next().await {
        let (code, who) = result.unwrap();
        assert_eq!(code, StatusCode::OK, "{who}");
        assert_eq!(who["devspace"].as_str(), Some(subject.as_str()));
    }
    let (users, organizations): (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM user_identities WHERE subject=$1),(SELECT count(*) FROM organizations o JOIN user_identities i ON i.user_id=o.created_by WHERE i.subject=$1)")
        .bind(&subject).fetch_one(&fx.pg().pool).await.unwrap();
    assert_eq!((users, organizations), (1, 1));
    fx.finish().await;
}

#[tokio::test]
async fn project_keys_send_but_never_read_and_revocation_takes_effect_immediately() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Acme").await;
    // A custom rule to prove a write-only key cannot read it back.
    let rule = json!({"id":"custom_private","name":"Private rule","group":"Quality","enabled":true,"agent":"codex","project":"shippinai","condition":{"kind":"response_contains","text":"secret plan"}});
    let settings = json!({"revision":0,"custom":[rule],"disabled":[],"options":{}});
    let (code, saved) = fx
        .send(
            "PUT",
            &team.owner,
            "/v1/signals/settings",
            Some(&settings),
            Some(&team.project),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{saved}");

    let (key, key_id) = project_key(&fx, &team.owner, &team.project).await;
    assert!(key.starts_with(KEY_PREFIX));
    let stored: (String, String) =
        sqlx::query_as("SELECT key_sha256,prefix FROM project_keys WHERE id=$1")
            .bind(&key_id)
            .fetch_one(&fx.pg().pool)
            .await
            .unwrap();
    assert_eq!(stored.0, hash_key(&key), "only the digest is stored");
    assert_ne!(stored.0, key);
    assert_eq!(stored.1, key[..13]);
    let keys_uri = format!("/v1/projects/{}/keys", team.project);
    let (_, listed) = fx.get(&team.owner, &keys_uri).await;
    assert!(!listed.to_string().contains(&key), "never shown again");
    assert_eq!(listed["keys"][0]["last_used_at"], Value::Null);

    // What a key can do: identify itself, read built-in settings, and deliver.
    let (code, who) = fx.get(&key, "/v1/whoami").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(who["devspace"].as_str(), Some(team.project.as_str()));
    assert_eq!(who["via"], "project_key");
    assert_eq!(who["write_only"], true);
    assert_eq!(who["key"].as_str(), Some(key_id.as_str()));
    assert!(who.get("user").is_none());
    let (code, settings) = fx.get(&key, "/v1/signals/settings").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(settings["settings"]["custom"], json!([]));
    assert!(!settings.to_string().contains("secret plan"));
    for (method, uri, body) in delivery_routes() {
        let (code, answer) = fx.send(method, &key, uri, Some(&body), None).await;
        assert_eq!(code, StatusCode::OK, "{uri}: {answer}");
    }
    let (_, listed) = fx.get(&team.owner, &keys_uri).await;
    assert!(listed["keys"][0]["last_used_at"].is_string());

    // What it cannot do: read anything back, change anything, or manage anything.
    for uri in READ_ROUTES {
        assert_eq!(
            fx.get(&key, uri).await.0,
            StatusCode::FORBIDDEN,
            "write-only key read {uri}"
        );
    }
    for (method, uri, body) in change_routes().into_iter().chain([dataset_route()]) {
        assert_eq!(
            fx.send(method, &key, uri, Some(&body), None).await.0,
            StatusCode::FORBIDDEN,
            "write-only key changed {uri}"
        );
    }
    assert_eq!(fx.get(&key, "/v1/me").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        fx.post(&key, "/v1/organizations", json!({"name":"Nope"}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fx.post(&key, &keys_uri, json!({"label":"Nope"})).await.0,
        StatusCode::FORBIDDEN
    );

    // The delivered record is readable by a member of the project, and nowhere else.
    let (code, turns) = fx
        .send("GET", &team.owner, "/v1/turns", None, Some(&team.project))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(turns["turns"].as_array().unwrap().len(), 1);
    let (_, default_project) = fx.get(&team.owner, "/v1/turns").await;
    assert!(default_project["turns"].as_array().unwrap().is_empty());

    // Revocation: on the very next request, with no cache to wait out. Twice is fine.
    let revoke_uri = format!("{keys_uri}/{key_id}");
    let (code, _) = fx
        .send("DELETE", &team.owner, &revoke_uri, None, None)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        fx.send("POST", &key, "/v1/turns", Some(&turns_body("late")), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(fx.get(&key, "/v1/whoami").await.0, StatusCode::UNAUTHORIZED);
    let (code, _) = fx
        .send("DELETE", &team.owner, &revoke_uri, None, None)
        .await;
    assert_eq!(code, StatusCode::OK);
    let (_, listed) = fx.get(&team.owner, &keys_uri).await;
    assert!(listed["keys"][0]["revoked_at"].is_string());

    // An unknown project key is refused here, without asking the identity provider. If it
    // had been asked, this outage would answer 503 instead.
    fx.fake
        .status
        .store(503, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        fx.get("rrop_0000", "/v1/whoami").await.0,
        StatusCode::UNAUTHORIZED
    );
    fx.fake
        .status
        .store(200, std::sync::atomic::Ordering::SeqCst);
    fx.finish().await;
}

#[tokio::test]
async fn another_organization_looks_exactly_like_one_that_does_not_exist() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let alice = team(&fx, "Alice Co").await;
    let (alice_key, alice_key_id) = project_key(&fx, &alice.owner, &alice.project).await;
    let (code, _) = fx
        .send(
            "POST",
            &alice.owner,
            "/v1/turns",
            Some(&turns_body("alice-private")),
            Some(&alice.project),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let (_, invitation) = fx
        .post(
            &alice.owner,
            &format!("/v1/organizations/{}/invitations", alice.organization),
            json!({"role":"viewer"}),
        )
        .await;
    let invitation = invitation["id"].as_str().unwrap().to_string();
    let bob = team(&fx, "Bob Co").await;

    // Every route, with Alice's real identifier and with one that does not exist, must give
    // Bob the same answer: he cannot tell which of them is real.
    let absent_organization = format!("o_{}", "0".repeat(32));
    let absent_project = format!("p_{}", "0".repeat(32));
    let name = json!({"name":"Mine now"});
    let role = json!({"role":"viewer"});
    let label = json!({"label":"Mine now"});
    let by_organization = |organization: &str| -> Vec<(&'static str, String, Option<Value>)> {
        let base = format!("/v1/organizations/{organization}");
        vec![
            ("POST", format!("{base}/projects"), Some(name.clone())),
            ("GET", format!("{base}/members"), None),
            ("PUT", format!("{base}/members/u_x"), Some(role.clone())),
            ("DELETE", format!("{base}/members/u_x"), None),
            ("GET", format!("{base}/invitations"), None),
            ("POST", format!("{base}/invitations"), Some(role.clone())),
            ("DELETE", format!("{base}/invitations/{invitation}"), None),
            ("GET", format!("{base}/audit"), None),
        ]
    };
    let by_project = |project: &str| -> Vec<(&'static str, String, Option<Value>)> {
        let base = format!("/v1/projects/{project}/keys");
        vec![
            ("GET", base.clone(), None),
            ("POST", base.clone(), Some(label.clone())),
            ("DELETE", format!("{base}/{alice_key_id}"), None),
        ]
    };
    for (real, absent) in by_organization(&alice.organization)
        .into_iter()
        .zip(by_organization(&absent_organization))
        .chain(
            by_project(&alice.project)
                .into_iter()
                .zip(by_project(&absent_project)),
        )
    {
        let seen = fx
            .send(real.0, &bob.owner, &real.1, real.2.as_ref(), None)
            .await;
        let unseen = fx
            .send(absent.0, &bob.owner, &absent.1, absent.2.as_ref(), None)
            .await;
        assert_eq!(seen.0, StatusCode::NOT_FOUND, "{} {}", real.0, real.1);
        assert_eq!(seen, unseen, "{} {} reveals existence", real.0, real.1);
    }

    // Naming Alice's project in a request is no different.
    for uri in READ_ROUTES {
        let seen = fx
            .send("GET", &bob.owner, uri, None, Some(&alice.project))
            .await;
        let unseen = fx
            .send("GET", &bob.owner, uri, None, Some(&absent_project))
            .await;
        assert_eq!(seen.0, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(seen, unseen, "{uri} reveals existence");
    }
    let (code, _) = fx
        .send(
            "POST",
            &bob.owner,
            "/v1/turns",
            Some(&turns_body("planted")),
            Some(&alice.project),
        )
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    // Keys open only their own project; asking for another is refused, not ignored.
    let (bob_key, _) = project_key(&fx, &bob.owner, &bob.project).await;
    for (credential, target) in [
        (bob_key.as_str(), alice.project.as_str()),
        (alice_key.as_str(), bob.project.as_str()),
        (OPERATOR_KEY, alice.project.as_str()),
    ] {
        assert_eq!(
            fx.send(
                "POST",
                credential,
                "/v1/turns",
                Some(&turns_body("planted")),
                Some(target),
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    // Naming your own project is fine.
    assert_eq!(
        fx.send(
            "POST",
            &bob_key,
            "/v1/turns",
            Some(&turns_body("bob-own")),
            Some(&bob.project),
        )
        .await
        .0,
        StatusCode::OK
    );

    // No credential is always 401, whatever else the request says.
    let anonymous = Request::builder()
        .uri("/v1/turns")
        .header(PROJECT_HEADER, "../secrets")
        .body(Body::empty())
        .unwrap();
    assert_eq!(call(&fx.app, anonymous).await.0, StatusCode::UNAUTHORIZED);

    // Nor is a bad name checked for someone who has not said who they are.
    let anonymous = Request::post("/v1/organizations")
        .header("content-type", "application/json")
        .body(Body::from(json!({"name":""}).to_string()))
        .unwrap();
    assert_eq!(call(&fx.app, anonymous).await.0, StatusCode::UNAUTHORIZED);

    // A malformed or repeated header cannot widen a request.
    for bad in ["../secrets", "has space", ""] {
        assert_eq!(
            fx.send("GET", &bob.owner, "/v1/turns", None, Some(bad))
                .await
                .0,
            StatusCode::NOT_FOUND,
            "{bad:?}"
        );
    }
    let mut twice = request("GET", &bob.owner, "/v1/turns", None, Some(&bob.project));
    twice
        .headers_mut()
        .append(PROJECT_HEADER, alice.project.parse().unwrap());
    assert_eq!(call(&fx.app, twice).await.0, StatusCode::NOT_FOUND);

    // Nothing Bob did reached Alice's project.
    let (_, turns) = fx
        .send("GET", &alice.owner, "/v1/turns", None, Some(&alice.project))
        .await;
    let ids: Vec<_> = turns["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["event_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["alice-private"]);
    let (_, projects) = fx.get(&bob.owner, "/v1/me").await;
    assert!(!projects.to_string().contains(&alice.project));
    fx.finish().await;
}

#[tokio::test]
async fn roles_decide_what_each_member_can_do() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Roles").await;
    let viewer = join(&fx, &team, "viewer").await;
    let member = join(&fx, &team, "member").await;
    let admin = join(&fx, &team, "admin").await;
    let project = Some(team.project.as_str());
    let organization = format!("/v1/organizations/{}", team.organization);
    let keys = format!("/v1/projects/{}/keys", team.project);
    let dataset = dataset_route();

    // Viewer: reads, changes nothing.
    for uri in READ_ROUTES {
        assert_eq!(
            fx.send("GET", &viewer, uri, None, project).await.0,
            StatusCode::OK,
            "viewer read {uri}"
        );
    }
    for (method, uri, body) in delivery_routes()
        .into_iter()
        .chain(change_routes())
        .chain([dataset.clone()])
    {
        assert_eq!(
            fx.send(method, &viewer, uri, Some(&body), project).await.0,
            StatusCode::FORBIDDEN,
            "viewer changed {uri}"
        );
    }

    // Member: delivers and changes project settings, but manages nothing.
    for (method, uri, body) in delivery_routes() {
        let (code, answer) = fx.send(method, &member, uri, Some(&body), project).await;
        assert_eq!(code, StatusCode::OK, "member {uri}: {answer}");
    }
    for (method, uri, body) in change_routes() {
        let (code, answer) = fx.send(method, &member, uri, Some(&body), project).await;
        assert_ne!(code, StatusCode::FORBIDDEN, "member {uri}: {answer}");
    }
    assert_eq!(
        fx.send(dataset.0, &member, dataset.1, Some(&dataset.2), project)
            .await
            .0,
        StatusCode::FORBIDDEN,
        "a member does not give consent for everyone"
    );

    for person in [&viewer, &member] {
        for (method, uri, body) in [
            (
                "POST",
                format!("{organization}/projects"),
                Some(json!({"name":"Nope"})),
            ),
            (
                "POST",
                format!("{organization}/invitations"),
                Some(json!({"role":"viewer"})),
            ),
            ("GET", format!("{organization}/invitations"), None),
            ("GET", format!("{organization}/audit"), None),
            ("GET", keys.clone(), None),
            ("POST", keys.clone(), Some(json!({"label":"Nope"}))),
        ] {
            assert_eq!(
                fx.send(method, person, &uri, body.as_ref(), None).await.0,
                StatusCode::FORBIDDEN,
                "{method} {uri}"
            );
        }
        // Everyone in the organization can see who else is in it.
        assert_eq!(
            fx.get(person, &format!("{organization}/members")).await.0,
            StatusCode::OK
        );
    }

    // Admin: manages projects, keys and invitations; gets past the consent role check.
    let (code, answer) = fx
        .send(dataset.0, &admin, dataset.1, Some(&dataset.2), project)
        .await;
    assert_ne!(code, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(
        fx.post(
            &admin,
            &format!("{organization}/projects"),
            json!({"name":"Second"})
        )
        .await
        .0,
        StatusCode::OK
    );
    project_key(&fx, &admin, &team.project).await;
    assert_eq!(
        fx.post(
            &admin,
            &format!("{organization}/invitations"),
            json!({"role":"admin"})
        )
        .await
        .0,
        StatusCode::OK
    );

    // No invitation grants ownership, whoever sends it.
    for person in [&admin, &team.owner] {
        assert_eq!(
            fx.post(
                person,
                &format!("{organization}/invitations"),
                json!({"role":"owner"})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }

    // Admins cannot reach above themselves.
    let owner_id = fx.user_id(&team.owner).await;
    let member_id = fx.user_id(&member).await;
    let admin_id = fx.user_id(&admin).await;
    let owner_uri = format!("{organization}/members/{owner_id}");
    let member_uri = format!("{organization}/members/{member_id}");
    let as_viewer = json!({"role":"viewer"});
    let as_owner = json!({"role":"owner"});
    for (method, uri, body) in [
        ("PUT", &owner_uri, Some(&as_viewer)),
        ("DELETE", &owner_uri, None),
        ("PUT", &member_uri, Some(&as_owner)),
    ] {
        assert_eq!(
            fx.send(method, &admin, uri, body, None).await.0,
            StatusCode::FORBIDDEN,
            "admin {method} {uri}"
        );
    }
    let (code, _) = fx
        .send(
            "PUT",
            &team.owner,
            &format!("{organization}/members/{admin_id}"),
            Some(&as_owner),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "an owner can make an owner");

    // A role change applies on the next request. A demoted member can no longer deliver.
    let (code, _) = fx
        .send("PUT", &admin, &member_uri, Some(&as_viewer), None)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        fx.send(
            "POST",
            &member,
            "/v1/turns",
            Some(&turns_body("demoted")),
            project
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    fx.finish().await;
}

#[tokio::test]
async fn removal_is_immediate_and_an_organization_always_keeps_an_owner() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Owners").await;
    let organization = format!("/v1/organizations/{}", team.organization);
    let project = Some(team.project.as_str());
    let member = join(&fx, &team, "member").await;
    let member_id = fx.user_id(&member).await;

    assert_eq!(
        fx.send("GET", &member, "/v1/turns", None, project).await.0,
        StatusCode::OK
    );
    let (code, _) = fx
        .send(
            "DELETE",
            &team.owner,
            &format!("{organization}/members/{member_id}"),
            None,
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        fx.send("GET", &member, "/v1/turns", None, project).await.0,
        StatusCode::NOT_FOUND,
        "their still-valid sign-in no longer opens the project"
    );
    // Their own personal project is unaffected.
    assert_eq!(fx.get(&member, "/v1/turns").await.0, StatusCode::OK);

    // Anyone may leave; the last owner may not leave, step down or be removed.
    let viewer = join(&fx, &team, "viewer").await;
    let viewer_id = fx.user_id(&viewer).await;
    let (code, _) = fx
        .send(
            "DELETE",
            &viewer,
            &format!("{organization}/members/{viewer_id}"),
            None,
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let owner_id = fx.user_id(&team.owner).await;
    let owner_uri = format!("{organization}/members/{owner_id}");
    assert_eq!(
        fx.send("DELETE", &team.owner, &owner_uri, None, None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        fx.send(
            "PUT",
            &team.owner,
            &owner_uri,
            Some(&json!({"role":"admin"})),
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    // Two owners stepping down at once cannot leave an organization with none.
    let second = join(&fx, &team, "admin").await;
    let second_id = fx.user_id(&second).await;
    let second_uri = format!("{organization}/members/{second_id}");
    let promote = json!({"role":"owner"});
    assert_eq!(
        fx.send("PUT", &team.owner, &second_uri, Some(&promote), None)
            .await
            .0,
        StatusCode::OK
    );
    let step_down = json!({"role":"viewer"});
    let (first, other) = tokio::join!(
        fx.send("PUT", &team.owner, &owner_uri, Some(&step_down), None),
        fx.send("PUT", &second, &second_uri, Some(&step_down), None),
    );
    let mut outcomes = [first.0, other.0];
    outcomes.sort();
    assert_eq!(outcomes, [StatusCode::OK, StatusCode::CONFLICT]);
    let owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_members WHERE organization_id=$1 AND role='owner'",
    )
    .bind(&team.organization)
    .fetch_one(&fx.pg().pool)
    .await
    .unwrap();
    assert_eq!(owners, 1);

    // A personal organization never changes hands, so a sign-in always has somewhere to open.
    let (_, me) = fx.get(&team.owner, "/v1/me").await;
    let personal = me["organizations"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(me["organizations"][0]["personal"], true);
    let personal_uri = format!("/v1/organizations/{personal}");
    for (method, uri, body) in [
        (
            "POST",
            format!("{personal_uri}/invitations"),
            Some(json!({"role":"viewer"})),
        ),
        ("DELETE", format!("{personal_uri}/members/{owner_id}"), None),
        (
            "PUT",
            format!("{personal_uri}/members/{owner_id}"),
            Some(json!({"role":"viewer"})),
        ),
    ] {
        assert_eq!(
            fx.send(method, &team.owner, &uri, body.as_ref(), None)
                .await
                .0,
            StatusCode::CONFLICT,
            "{method} {uri}"
        );
    }
    fx.finish().await;
}

#[tokio::test]
async fn invitations_are_single_use_bounded_in_time_and_stored_only_as_digests() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Invites").await;
    let invitations = format!("/v1/organizations/{}/invitations", team.organization);
    let accept = "/v1/invitations/accept";
    async fn invite(fx: &Fixture, owner: &str, uri: &str) -> (String, String) {
        let (code, invitation) = fx.post(owner, uri, json!({"role":"member"})).await;
        assert_eq!(code, StatusCode::OK, "{invitation}");
        (
            invitation["token"].as_str().unwrap().to_string(),
            invitation["id"].as_str().unwrap().to_string(),
        )
    }

    let (token, id) = invite(&fx, &team.owner, &invitations).await;
    assert!(token.starts_with(INVITATION_PREFIX));
    let stored: String =
        sqlx::query_scalar("SELECT token_sha256 FROM organization_invitations WHERE id=$1")
            .bind(&id)
            .fetch_one(&fx.pg().pool)
            .await
            .unwrap();
    assert_eq!(stored, hash_key(&token));
    let (_, listed) = fx.get(&team.owner, &invitations).await;
    assert_eq!(listed["invitations"][0]["id"].as_str(), Some(id.as_str()));
    assert!(!listed.to_string().contains(&token));

    // Only people can accept, not keys.
    let (key, _) = project_key(&fx, &team.owner, &team.project).await;
    assert_eq!(
        fx.post(&key, accept, json!({"token":token})).await.0,
        StatusCode::FORBIDDEN
    );

    // An existing member does not use it up, so it still reaches the person it was for.
    assert_eq!(
        fx.post(&team.owner, accept, json!({"token":token})).await.0,
        StatusCode::CONFLICT
    );
    let first = fx.person();
    assert_eq!(
        fx.post(&first, accept, json!({"token":token})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        fx.send("GET", &first, "/v1/turns", None, Some(&team.project))
            .await
            .0,
        StatusCode::OK
    );

    // Used, unknown, revoked and expired all look the same.
    let second = fx.person();
    let used = fx.post(&second, accept, json!({"token":token})).await;
    assert_eq!(used.0, StatusCode::NOT_FOUND);
    let unknown = fx.post(&second, accept, json!({"token":"rroi_nope"})).await;
    assert_eq!(used, unknown);

    let (revoked, revoked_id) = invite(&fx, &team.owner, &invitations).await;
    let revoke = format!("{invitations}/{revoked_id}");
    assert_eq!(
        fx.send("DELETE", &team.owner, &revoke, None, None).await.0,
        StatusCode::OK
    );
    assert_eq!(
        fx.send("DELETE", &team.owner, &revoke, None, None).await.0,
        StatusCode::NOT_FOUND,
        "nothing left to revoke"
    );
    assert_eq!(
        fx.post(&second, accept, json!({"token":revoked})).await,
        used
    );

    let (expired, expired_id) = invite(&fx, &team.owner, &invitations).await;
    sqlx::query(
        "UPDATE organization_invitations SET expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(&expired_id)
    .execute(&fx.pg().pool)
    .await
    .unwrap();
    assert_eq!(
        fx.post(&second, accept, json!({"token":expired})).await,
        used
    );
    let (_, listed) = fx.get(&team.owner, &invitations).await;
    assert!(listed["invitations"].as_array().unwrap().is_empty());
    fx.finish().await;
}

#[tokio::test]
async fn allowances_pool_across_an_organization_so_new_projects_buy_nothing() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    fx.pg()
        .set_allowance_policy(&allowances::Policy {
            schema: "rroflow.allowance-policy/v1".into(),
            revision: 1,
            default: allowances::Allowance {
                plan: "fixture".into(),
                label: "Fixture".into(),
                turns_per_month: 2,
                events_per_month: 100,
            },
        })
        .await
        .unwrap();
    let pooled = team(&fx, "Pooled").await;
    let projects = format!("/v1/organizations/{}/projects", pooled.organization);
    let (_, second) = fx
        .post(&pooled.owner, &projects, json!({"name":"Second"}))
        .await;
    let second = second["id"].as_str().unwrap().to_string();
    let owner = pooled.owner.clone();
    let main = pooled.project.clone();
    let deliver = async |token: &str, project: &str, id: &str| {
        fx.send(
            "POST",
            token,
            "/v1/turns",
            Some(&turns_body(id)),
            Some(project),
        )
        .await
    };

    assert_eq!(deliver(&owner, &main, "a").await.0, StatusCode::OK);
    assert_eq!(deliver(&owner, &second, "b").await.0, StatusCode::OK);
    // The allowance is spent across both projects, so a third project cannot add room.
    let (code, refused) = deliver(&owner, &second, "c").await;
    assert_eq!(code, StatusCode::TOO_MANY_REQUESTS, "{refused}");
    assert_eq!(refused["quota"]["used"], 2);
    let (_, third) = fx.post(&owner, &projects, json!({"name":"Third"})).await;
    let third = third["id"].as_str().unwrap().to_string();
    assert_eq!(
        deliver(&owner, &third, "d").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Replaying what was already accepted never costs anything.
    assert_eq!(deliver(&owner, &main, "a").await.0, StatusCode::OK);
    // Status tells the same pooled truth from every project.
    for project in [&main, &second, &third] {
        let (_, status) = fx
            .send("GET", &owner, "/v1/allowances", None, Some(project))
            .await;
        assert_eq!(status["used"]["turns"], 2, "{status}");
        assert_eq!(
            status["organization"].as_str(),
            Some(pooled.organization.as_str())
        );
    }
    // Other organizations, and devspaces outside any organization, keep their own count.
    let other = team(&fx, "Unrelated").await;
    assert_eq!(
        deliver(&other.owner, &other.project, "e").await.0,
        StatusCode::OK
    );
    assert_eq!(
        fx.post(OPERATOR_KEY, "/v1/turns", turns_body("f")).await.0,
        StatusCode::OK
    );

    // Concurrent delivery to different projects cannot overspend the last of an allowance.
    let racers = team(&fx, "Racers").await;
    let racer_projects = format!("/v1/organizations/{}/projects", racers.organization);
    let mut targets = vec![racers.project.clone()];
    for name in ["One", "Two", "Three"] {
        let (_, project) = fx
            .post(&racers.owner, &racer_projects, json!({"name":name}))
            .await;
        targets.push(project["id"].as_str().unwrap().to_string());
    }
    let mut attempts = tokio::task::JoinSet::new();
    for (index, project) in targets.into_iter().enumerate() {
        let (app, token) = (fx.app.clone(), racers.owner.clone());
        attempts.spawn(async move {
            let body = turns_body(&format!("race-{index}"));
            call(
                &app,
                request("POST", &token, "/v1/turns", Some(&body), Some(&project)),
            )
            .await
            .0
        });
    }
    let mut accepted = 0;
    while let Some(code) = attempts.join_next().await {
        match code.unwrap() {
            StatusCode::OK => accepted += 1,
            StatusCode::TOO_MANY_REQUESTS => {}
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(accepted, 2, "exactly the allowance, across four projects");
    fx.finish().await;
}

#[tokio::test]
async fn creation_limits_are_enforced_and_personal_organizations_do_not_count() {
    let Some(fx) = fixture(TenancyConfig {
        max_organizations_per_user: 1,
        max_projects_per_organization: 2,
    })
    .await
    else {
        return;
    };
    let person = fx.person();
    let (code, organization) = fx
        .post(&person, "/v1/organizations", json!({"name":"Only"}))
        .await;
    assert_eq!(code, StatusCode::OK, "{organization}");
    assert_eq!(
        fx.post(&person, "/v1/organizations", json!({"name":"Second"}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let projects = format!(
        "/v1/organizations/{}/projects",
        organization["id"].as_str().unwrap()
    );
    for name in ["One", "Two"] {
        assert_eq!(
            fx.post(&person, &projects, json!({"name":name})).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        fx.post(&person, &projects, json!({"name":"Three"})).await.0,
        StatusCode::CONFLICT
    );
    // The default project counts toward its organization's limit.
    let (_, me) = fx.get(&person, "/v1/me").await;
    let personal = me["organizations"][0]["id"].as_str().unwrap();
    let personal = format!("/v1/organizations/{personal}/projects");
    assert_eq!(
        fx.post(&person, &personal, json!({"name":"Extra"})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        fx.post(&person, &personal, json!({"name":"Too many"}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Names are checked before anything is created.
    for bad in ["", "   ", "line\nbreak", &"x".repeat(101)] {
        assert_eq!(
            fx.post(&person, "/v1/organizations", json!({"name":bad}))
                .await
                .0,
            StatusCode::BAD_REQUEST,
            "{bad:?}"
        );
    }
    fx.finish().await;
}

#[tokio::test]
async fn audit_records_who_changed_what_for_admins_without_any_secret() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Audited").await;
    let organization = format!("/v1/organizations/{}", team.organization);
    let (key, key_id) = project_key(&fx, &team.owner, &team.project).await;
    let (_, invitation) = fx
        .post(
            &team.owner,
            &format!("{organization}/invitations"),
            json!({"role":"viewer"}),
        )
        .await;
    let token = invitation["token"].as_str().unwrap().to_string();
    let viewer = fx.person();
    fx.post(&viewer, "/v1/invitations/accept", json!({"token":token}))
        .await;
    let viewer_id = fx.user_id(&viewer).await;
    fx.send(
        "PUT",
        &team.owner,
        &format!("{organization}/members/{viewer_id}"),
        Some(&json!({"role":"member"})),
        None,
    )
    .await;
    fx.send(
        "DELETE",
        &team.owner,
        &format!("/v1/projects/{}/keys/{key_id}", team.project),
        None,
        None,
    )
    .await;
    fx.send(
        "DELETE",
        &team.owner,
        &format!("{organization}/members/{viewer_id}"),
        None,
        None,
    )
    .await;

    let (code, log) = fx.get(&team.owner, &format!("{organization}/audit")).await;
    assert_eq!(code, StatusCode::OK);
    let actions: Vec<_> = log["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        [
            "member.removed",
            "key.revoked",
            "member.role_changed",
            "member.joined",
            "invitation.created",
            "key.created",
            "project.created",
            "organization.created",
        ]
    );
    let owner_id = fx.user_id(&team.owner).await;
    assert_eq!(log["events"][0]["actor"].as_str(), Some(owner_id.as_str()));
    assert_eq!(
        log["events"][0]["target"].as_str(),
        Some(viewer_id.as_str())
    );
    assert_eq!(log["events"][2]["detail"]["from"], "viewer");
    assert_eq!(log["events"][2]["detail"]["to"], "member");
    let text = log.to_string();
    assert!(!text.contains(&key) && !text.contains(&token), "{text}");
    assert_eq!(log["next_before"], Value::Null);

    // Pages walk back through history without repeats.
    let (_, page) = fx
        .get(&team.owner, &format!("{organization}/audit?limit=3"))
        .await;
    assert_eq!(page["events"].as_array().unwrap().len(), 3);
    let before = page["next_before"].as_i64().unwrap();
    let (_, rest) = fx
        .get(
            &team.owner,
            &format!("{organization}/audit?limit=100&before={before}"),
        )
        .await;
    assert_eq!(rest["events"][0]["action"], "member.joined");
    assert_eq!(rest["events"].as_array().unwrap().len(), 5);
    fx.finish().await;
}

#[tokio::test]
async fn claiming_attaches_an_organization_and_refuses_organization_projects() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let issuer = fx.fake.issuer.clone();
    let person = fx.person();
    let subject = person.strip_prefix("tok-").unwrap().to_string();
    assert_eq!(
        fx.post(OPERATOR_KEY, "/v1/turns", turns_body("before-claim"))
            .await
            .0,
        StatusCode::OK
    );
    fx.get(&person, "/v1/whoami").await; // An empty default project, which gives way.
    fx.pg()
        .claim_devspace("operator", &issuer, &subject)
        .await
        .unwrap();

    let (_, who) = fx.get(&person, "/v1/whoami").await;
    assert_eq!(who["devspace"], "operator");
    let (_, me) = fx.get(&person, "/v1/me").await;
    assert_eq!(me["default_project"], "operator");
    assert_eq!(me["organizations"].as_array().unwrap().len(), 1);
    assert_eq!(
        me["organizations"][0]["projects"],
        json!([{"id":"operator","slug":"default","name":"Default project"}])
    );
    let (_, turns) = fx.get(&person, "/v1/turns").await;
    assert_eq!(turns["turns"][0]["event_id"], "before-claim");
    // The operator's key keeps working, and it is recorded as an operator action.
    assert_eq!(fx.get(OPERATOR_KEY, "/v1/turns").await.0, StatusCode::OK);
    let organization = me["organizations"][0]["id"].as_str().unwrap();
    let (_, log) = fx
        .get(&person, &format!("/v1/organizations/{organization}/audit"))
        .await;
    assert_eq!(log["events"][0]["action"], "project.claimed");
    assert_eq!(log["events"][0]["actor"], Value::Null);

    // A project someone created in an organization is not free to claim.
    let team = team(&fx, "Not yours").await;
    let error = fx
        .pg()
        .claim_devspace(&team.project, &issuer, &subject)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("belongs to an organization"));
    fx.finish().await;
}

#[tokio::test]
async fn a_removed_or_demoted_admin_cannot_return_with_an_invitation_they_kept() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Keepers").await;
    let organization = format!("/v1/organizations/{}", team.organization);
    let invitations = format!("{organization}/invitations");
    let accept = "/v1/invitations/accept";
    let mint = async |admin: &str| {
        let (code, invitation) = fx.post(admin, &invitations, json!({"role":"admin"})).await;
        assert_eq!(code, StatusCode::OK, "{invitation}");
        invitation["token"].as_str().unwrap().to_string()
    };

    // Removed, holding a token they minted while they were an admin.
    let removed = join(&fx, &team, "admin").await;
    let removed_id = fx.user_id(&removed).await;
    let kept = mint(&removed).await;
    let (code, answer) = fx
        .send(
            "DELETE",
            &team.owner,
            &format!("{organization}/members/{removed_id}"),
            None,
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{answer}");
    assert_eq!(
        fx.post(&removed, accept, json!({"token":kept})).await.0,
        StatusCode::NOT_FOUND,
        "a removed admin came back as an admin"
    );

    // Demoted, then leaving so the token could be accepted.
    let demoted = join(&fx, &team, "admin").await;
    let demoted_id = fx.user_id(&demoted).await;
    let demoted_uri = format!("{organization}/members/{demoted_id}");
    let kept = mint(&demoted).await;
    let (code, _) = fx
        .send(
            "PUT",
            &team.owner,
            &demoted_uri,
            Some(&json!({"role":"viewer"})),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        fx.send("DELETE", &demoted, &demoted_uri, None, None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fx.post(&demoted, accept, json!({"token":kept})).await.0,
        StatusCode::NOT_FOUND,
        "a demoted admin came back as an admin"
    );

    // Someone who keeps an admin's authority keeps their invitations working.
    let staying = join(&fx, &team, "admin").await;
    let staying_id = fx.user_id(&staying).await;
    let token = mint(&staying).await;
    let (code, _) = fx
        .send(
            "PUT",
            &team.owner,
            &format!("{organization}/members/{staying_id}"),
            Some(&json!({"role":"owner"})),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    let joining = fx.person();
    assert_eq!(
        fx.post(&joining, accept, json!({"token":token})).await.0,
        StatusCode::OK
    );

    // Keys belong to the project, so removal does not stop delivery; it says what to review.
    let leaving = join(&fx, &team, "admin").await;
    let leaving_id = fx.user_id(&leaving).await;
    let (key, _) = project_key(&fx, &leaving, &team.project).await;
    let (code, removal) = fx
        .send(
            "DELETE",
            &team.owner,
            &format!("{organization}/members/{leaving_id}"),
            None,
            None,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(removal["active_keys_created"], 1, "{removal}");
    assert_eq!(fx.get(&key, "/v1/whoami").await.0, StatusCode::OK);
    fx.finish().await;
}

/// Delivery locks the project and then the organization. Anything else that needs both must
/// take them in that order, or the two wait on each other until Postgres aborts one.
#[tokio::test]
async fn minting_a_key_during_delivery_does_not_deadlock() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Locks").await;
    let mut delivery = fx.pg().pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR UPDATE")
        .bind(&team.project)
        .fetch_one(&mut *delivery)
        .await
        .unwrap();
    let mint = {
        let app = fx.app.clone();
        let body = json!({"label":"During delivery"});
        let request = request(
            "POST",
            &team.owner,
            &format!("/v1/projects/{}/keys", team.project),
            Some(&body),
            None,
        );
        tokio::spawn(async move { call(&app, request).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    sqlx::query("SELECT id FROM organizations WHERE id=$1 FOR NO KEY UPDATE")
        .bind(&team.organization)
        .fetch_one(&mut *delivery)
        .await
        .unwrap();
    delivery.commit().await.unwrap();
    let (code, answer) = mint.await.unwrap();
    assert_eq!(code, StatusCode::OK, "{answer}");
    fx.finish().await;
}

#[tokio::test]
async fn claiming_refuses_to_destroy_a_project_with_active_keys() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let issuer = fx.fake.issuer.clone();
    let person = fx.person();
    let subject = person.strip_prefix("tok-").unwrap().to_string();
    let (_, key) = project_key(&fx, &person, &subject).await;
    let error = fx
        .pg()
        .claim_devspace("operator", &issuer, &subject)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("active key"), "{error}");
    let (code, who) = fx.get(&person, "/v1/whoami").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(who["devspace"].as_str(), Some(subject.as_str()));

    // Once revoked it can give way, and the trail names what was replaced.
    assert_eq!(
        fx.send(
            "DELETE",
            &person,
            &format!("/v1/projects/{subject}/keys/{key}"),
            None,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    fx.pg()
        .claim_devspace("operator", &issuer, &subject)
        .await
        .unwrap();
    let (_, me) = fx.get(&person, "/v1/me").await;
    let organization = me["organizations"][0]["id"].as_str().unwrap();
    let (_, log) = fx
        .get(&person, &format!("/v1/organizations/{organization}/audit"))
        .await;
    assert_eq!(log["events"][0]["action"], "project.claimed");
    assert_eq!(
        log["events"][0]["detail"]["replaced"].as_str(),
        Some(subject.as_str())
    );
    fx.finish().await;
}

/// The migration against the state it will really meet: an owned devspace and a key-registered
/// one, both with records, at the schema the previous release left.
#[tokio::test]
async fn migration_gives_existing_accounts_an_organization_and_moves_no_record() {
    let Ok(base) = std::env::var(PG_ENV) else {
        return;
    };
    let admin = sqlx::PgPool::connect(&base).await.unwrap();
    let schema = format!("tenancy_migration_{}", rand::random::<u64>());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&private_schema_url(&base, &schema))
        .await
        .unwrap();
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    let (earlier, this): (Vec<_>, Vec<_>) = files
        .iter()
        .partition(|path| path.file_name().unwrap().to_str().unwrap() < "0013");
    assert_eq!(earlier.len(), 12);
    for path in earlier {
        let sql = std::fs::read_to_string(path).unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO devspaces (id,owner_issuer,owner_subject,owner_org) VALUES ('owned','https://id.test','123','zitadel-org'),('owned2','https://id.test','456',NULL)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO devspaces (id) VALUES ('by_key')")
        .execute(&pool)
        .await
        .unwrap();
    for (devspace, event) in [("owned", "t1"), ("owned", "t2"), ("by_key", "t3")] {
        sqlx::query("INSERT INTO turns (devspace_id,event_id,agent,provider,session_id,termination,tool_calls,tool_errors,has_content,record) VALUES ($1,$2,'codex','openai','s','completed',0,0,false,'{}')")
            .bind(devspace).bind(event).execute(&pool).await.unwrap();
    }

    let sql = std::fs::read_to_string(this[0]).unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(&pool)
        .await
        .unwrap();

    let row: (String, String, bool, String, String, Option<String>, String) = sqlx::query_as("SELECT d.slug,d.name,o.personal,m.role,o.slug,d.owner_org,i.subject FROM devspaces d JOIN organizations o ON o.id=d.organization_id JOIN organization_members m ON m.organization_id=o.id JOIN user_identities i ON i.user_id=m.user_id AND i.issuer=d.owner_issuer AND i.subject=d.owner_subject WHERE d.id='owned'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "default");
    assert_eq!(row.1, "Default project");
    assert!(row.2);
    assert_eq!(row.3, "owner");
    assert!(row.4.starts_with("personal-"));
    assert_eq!(row.5.as_deref(), Some("zitadel-org"), "kept as it was");
    assert_eq!(row.6, "123");
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM users),(SELECT count(*) FROM user_identities),(SELECT count(*) FROM organizations),(SELECT count(*) FROM organization_members),(SELECT count(*) FROM turns)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        counts,
        (2, 2, 2, 2, 3),
        "one each per account; no turn lost"
    );
    let turns: i64 = sqlx::query_scalar("SELECT count(*) FROM turns WHERE devspace_id='owned'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(turns, 2, "records stay in their partition");
    // A key-registered devspace stays operator-managed until claimed.
    let organization: Option<String> =
        sqlx::query_scalar("SELECT organization_id FROM devspaces WHERE id='by_key'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(organization, None);
    // From now on an account cannot own a devspace outside an organization.
    let error = sqlx::query("INSERT INTO devspaces (id,owner_issuer,owner_subject) VALUES ('stray','https://id.test','789')")
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    pool.close().await;
    let drop = format!("DROP SCHEMA {schema} CASCADE");
    sqlx::query(sqlx::AssertSqlSafe(drop))
        .execute(&admin)
        .await
        .unwrap();
}

#[test]
fn slugs_are_url_safe_and_avoid_console_routes() {
    assert_eq!(slug_base("Acme Robotics, Inc.", "x"), "acme-robotics-inc");
    assert_eq!(slug_base("  --Déjà Vu--  ", "x"), "d-j-vu");
    assert_eq!(slug_base("日本語", "project"), "project");
    assert_eq!(slug_base("!!!", "organization"), "organization");
    let long = slug_base(&format!("{}-{}", "a".repeat(39), "b".repeat(9)), "x");
    assert_eq!(long, "a".repeat(39), "no trailing hyphen after truncating");
    assert_eq!(slug_candidate("acme", 0), "acme");
    let retried = slug_candidate("acme", 1);
    assert!(retried.starts_with("acme-") && retried.len() == 11);
    for reserved in RESERVED_SLUGS {
        assert_ne!(&slug_candidate(reserved, 0), reserved);
    }
}

#[test]
fn nobody_manages_above_themselves() {
    use Role::{Admin, Member, Owner, Viewer};
    assert!(Viewer < Member && Member < Admin && Admin < Owner);
    assert!(may_change(Owner, Owner, Some(Viewer)));
    assert!(may_change(Owner, Viewer, Some(Owner)));
    assert!(may_change(Owner, Owner, None));
    assert!(may_change(Admin, Member, Some(Admin)));
    assert!(may_change(Admin, Admin, None));
    assert!(!may_change(Admin, Owner, Some(Viewer)));
    assert!(!may_change(Admin, Owner, None));
    assert!(!may_change(Admin, Member, Some(Owner)));
    assert!(!may_change(Member, Viewer, Some(Viewer)));
    assert!(!may_change(Member, Viewer, None));
    assert!(!may_change(Viewer, Viewer, None));
}

#[test]
fn limits_have_safe_defaults_and_reject_unknown_settings() {
    let config: Config = toml::from_str("").unwrap();
    assert_eq!(config.tenancy.max_organizations_per_user, 3);
    assert_eq!(config.tenancy.max_projects_per_organization, 10);
    let config: Config = toml::from_str("[tenancy]\nmax_organizations_per_user = 0\n").unwrap();
    assert_eq!(config.tenancy.max_organizations_per_user, 0);
    assert_eq!(config.tenancy.max_projects_per_organization, 10);
    assert!(toml::from_str::<Config>("[tenancy]\nmax_orgs = 5\n").is_err());
}

#[tokio::test]
async fn management_authorization_is_refreshed_after_waiting_for_organization_lock() {
    let Some(fx) = fixture(TenancyConfig::default()).await else {
        return;
    };
    let team = team(&fx, "Concurrent permission changes").await;
    for project_check in [false, true] {
        for remove in [false, true] {
            let actor = join(&fx, &team, "admin").await;
            let user = fx.user_id(&actor).await;
            let mut blocker = fx.pg().pool.begin().await.unwrap();
            sqlx::query("SELECT id FROM organizations WHERE id=$1 FOR NO KEY UPDATE")
                .bind(&team.organization)
                .execute(&mut *blocker)
                .await
                .unwrap();
            let pool = fx.pg().pool.clone();
            let organization = team.organization.clone();
            let project = team.project.clone();
            let waiting_user = user.clone();
            let (started, pending) = tokio::sync::oneshot::channel();
            let waiting = tokio::spawn(async move {
                let mut tx = pool.begin().await.unwrap();
                let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
                started.send(pid).unwrap();
                let role = if project_check {
                    locked_project(&mut tx, &project, &waiting_user)
                        .await
                        .map(|(_, role)| role)
                } else {
                    locked_organization(&mut tx, &organization, &waiting_user)
                        .await
                        .map(|(role, _)| role)
                };
                tx.rollback().await.unwrap();
                role
            });
            let pid = pending.await.unwrap();
            // Observe the real database lock wait; no timing assumption establishes the race.
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let blocked: bool =
                        sqlx::query_scalar("SELECT cardinality(pg_blocking_pids($1)) > 0")
                            .bind(pid)
                            .fetch_one(&fx.admin)
                            .await
                            .unwrap();
                    if blocked {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("management check must be waiting on the organization lock");
            let change = if remove {
                "DELETE FROM organization_members WHERE organization_id=$1 AND user_id=$2"
            } else {
                "UPDATE organization_members SET role='viewer' WHERE organization_id=$1 AND user_id=$2"
            };
            sqlx::query(change)
                .bind(&team.organization)
                .bind(&user)
                .execute(&mut *blocker)
                .await
                .unwrap();
            blocker.commit().await.unwrap();
            let role = tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
                .await
                .unwrap()
                .unwrap();
            if remove {
                assert!(
                    matches!(role, Err(ApiError::Missing)),
                    "removed member must not retain management access: {role:?}"
                );
            } else {
                assert_eq!(
                    role.unwrap(),
                    Role::Viewer,
                    "demoted member must not retain admin authorization"
                );
            }
        }
    }
    fx.finish().await;
}
