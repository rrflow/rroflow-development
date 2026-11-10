//! Fake secrets of every kind the scrubber must remove, for the leak test.
//!
//! Each template takes fresh random characters and returns the text to plant and the part of it
//! that must not survive. Nothing here is a real credential.

use rand::distr::{Alphanumeric, SampleString};
use sha2::{Digest, Sha256};

pub(crate) struct Plant {
    pub(crate) kind: &'static str,
    pub(crate) text: String,
    pub(crate) secret: String,
}

type Template = fn(&str) -> (String, String);

fn same(text: String, secret: &str) -> (String, String) {
    (text, secret.to_string())
}

/// `random` with digits turned into letters, for values that hold no digit.
fn letters(random: &str) -> String {
    random
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(d) => char::from(b'a' + d as u8),
            None => c,
        })
        .collect()
}

pub(crate) const TEMPLATES: &[(&str, Template)] = &[
    ("private_key", |s| {
        same(
            format!("-----BEGIN RSA PRIVATE KEY-----\n{s}\n{s}\n-----END RSA PRIVATE KEY-----"),
            s,
        )
    }),
    ("private_key_cut_off", |s| {
        same(format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{s}\n{s}"), s)
    }),
    ("rroflow_ingest_key", |s| same(format!("rrok_{s}"), s)),
    ("rroflow_project_key", |s| same(format!("rrop_{s}"), s)),
    ("rroflow_session", |s| same(format!("rrna_{s}"), s)),
    ("rroflow_setup_password", |s| {
        let s = s.to_ascii_uppercase();
        let (a, b, c) = (&s[..5], &s[5..10], &s[10..15]);
        (format!("rrsetup-{a}-{b}-{c}"), s[..15].to_string())
    }),
    ("anthropic_key", |s| same(format!("sk-ant-api03-{s}"), s)),
    ("openai_key", |s| same(format!("sk-proj-{s}"), s)),
    ("xai_key", |s| same(format!("xai-{s}"), s)),
    ("stripe_key", |s| same(format!("sk_live_{s}"), s)),
    ("stripe_webhook", |s| same(format!("whsec_{s}"), s)),
    ("github_token", |s| same(format!("ghp_{s}"), s)),
    ("github_fine_grained", |s| {
        same(format!("github_pat_{s}"), s)
    }),
    ("gitlab_token", |s| same(format!("glpat-{s}"), s)),
    ("slack_token", |s| same(format!("xoxb-{s}"), s)),
    ("slack_webhook", |s| {
        same(format!("https://hooks.slack.com/services/T0/B0/{s}"), s)
    }),
    ("aws_key", |s| {
        let s = s[..16].to_ascii_uppercase();
        (format!("AKIA{s}"), s)
    }),
    ("google_api_key", |s| same(format!("AIza{s}"), s)),
    ("google_oauth_secret", |s| same(format!("GOCSPX-{s}"), s)),
    ("google_access_token", |s| same(format!("ya29.{s}"), s)),
    ("npm_token", |s| same(format!("npm_{s}"), s)),
    ("pypi_token", |s| same(format!("pypi-{s}{s}"), s)),
    ("huggingface_token", |s| same(format!("hf_{s}"), s)),
    ("sendgrid_key", |s| same(format!("SG.{s}.{s}"), s)),
    ("jwt", |s| {
        same(format!("eyJhbGciOiJSUzI1.eyJzdWIiOiIx.{s}"), s)
    }),
    ("postgres_url_password", |s| {
        same(format!("postgres://app:{s}@db.internal:5432/main"), s)
    }),
    ("redis_url_password", |s| {
        same(format!("redis://:{s}@cache.internal:6379/0"), s)
    }),
    ("bearer_header", |s| {
        same(format!("Authorization: Bearer {s}"), s)
    }),
    ("basic_header", |s| {
        same(format!("Authorization: Basic {s}"), s)
    }),
    ("bare_bearer", |s| same(format!("curl with bearer {s}"), s)),
    ("cookie_header", |s| {
        same(format!("Cookie: session={s}; theme=dark"), s)
    }),
    ("env_password_without_digits", |s| {
        let s = letters(s);
        (format!("DB_PASSWORD={s}\nDB_HOST=localhost"), s)
    }),
    ("env_secret_quoted", |s| {
        same(format!("API_SECRET=\"{s}\""), s)
    }),
    ("exported_token", |s| {
        same(format!("export GITHUB_TOKEN={s}"), s)
    }),
    ("yaml_field", |s| same(format!("client_secret: {s}"), s)),
    ("query_parameter", |s| {
        same(format!("https://example.test/cb?api_key={s}&x=1"), s)
    }),
    ("json_field", |s| {
        let s = letters(s);
        (format!("{{\"password\":\"{s}\"}}"), s)
    }),
    ("nested_json_field", |s| {
        same(
            format!("{{\"body\":\"{{\\\"client_secret\\\":\\\"{s}\\\"}}\"}}"),
            s,
        )
    }),
    ("camel_case_json_field", |s| {
        same(format!("{{\"accessToken\":\"{s}\"}}"), s)
    }),
    ("command_flag", |s| {
        same(format!("mysql --password {s} -h db"), s)
    }),
    ("long_hex", |s| {
        let hex = hex_digest(s);
        same(hex.clone(), &hex)
    }),
];

fn hex_digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Template `index` (wrapping around), filled with fresh random characters.
pub(crate) fn plant(index: usize) -> Plant {
    let (kind, template) = TEMPLATES[index % TEMPLATES.len()];
    let random = Alphanumeric.sample_string(&mut rand::rng(), 40);
    let (text, secret) = template(&random);
    Plant { kind, text, secret }
}

/// True if any recognizable part of `secret` is still in `text`.
pub(crate) fn survives(text: &str, secret: &str) -> bool {
    const PART: usize = 12;
    text.contains(secret)
        || (secret.len() > PART
            && (text.contains(&secret[..PART]) || text.contains(&secret[secret.len() - PART..])))
}
