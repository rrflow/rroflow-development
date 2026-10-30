//! Public deployment settings. Credentials remain in the separate account file.
use crate::{account, capture};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(clap::Args, Default)]
#[group(id = "deployment_options")]
pub(crate) struct Args {
    /// Public deployment JSON; otherwise deployment.json in RROFLOW_CONFIG_DIR.
    #[arg(long, env = "RROFLOW_DEPLOYMENT")]
    deployment: Option<PathBuf>,
    /// Nomen issuer origin (HTTPS, or loopback HTTP for local development).
    #[arg(long, env = "RROFLOW_ISSUER")]
    issuer: Option<String>,
    /// Public device-authorization client registered with your issuer.
    #[arg(long, env = "RROFLOW_CLIENT_ID")]
    client_id: Option<String>,
    /// Nomen project audience accepted by your ingest service.
    #[arg(long, env = "RROFLOW_NOMEN_PROJECT")]
    project_id: Option<String>,
    /// Ingest origin. No destination is compiled into the collector.
    #[arg(long, env = "RROFLOW_INGEST_URL")]
    ingest_url: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub project_id: Option<String>,
    pub ingest_url: Option<String>,
}

pub(crate) struct Deployment {
    pub issuer: String,
    pub client_id: String,
    pub project_id: String,
    pub ingest_url: String,
}

pub(crate) fn read(path: &Path, required: bool) -> Result<Profile> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if !required && e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Profile::default());
        }
        Err(_) => anyhow::bail!("cannot read deployment file {}", path.display()),
    };
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("cannot read deployment file {}", path.display()))?;
    ensure!(bytes.len() <= 64 * 1024, "deployment file exceeds 64 KiB");
    // Do not echo unrecognized values: a misplaced credential is still private.
    serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!(
            "invalid deployment JSON: expected only issuer, client_id, project_id and ingest_url"
        )
    })
}

pub(crate) fn profile(explicit: Option<&Path>) -> Result<Profile> {
    match explicit {
        Some(path) => read(path, true),
        None => match account::config_dir() {
            Some(dir) => read(&dir.join("deployment.json"), false),
            None => Ok(Profile::default()),
        },
    }
}

pub(crate) fn environment_profile() -> Result<Profile> {
    profile(
        std::env::var_os("RROFLOW_DEPLOYMENT")
            .as_deref()
            .map(Path::new),
    )
}

impl Args {
    pub fn resolve(&self, saved: Option<&account::Account>) -> Result<Deployment> {
        self.resolve_with(profile(self.deployment.as_deref())?, saved)
    }

    fn resolve_with(&self, file: Profile, saved: Option<&account::Account>) -> Result<Deployment> {
        let required = |value: Option<String>, field: &str| -> Result<String> {
            let value = value.with_context(|| format!("missing {field}: supply --deployment FILE, deployment.json, environment or command options"))?;
            ensure!(
                !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c.is_control()),
                "invalid deployment {field}"
            );
            Ok(value)
        };
        let issuer = required(
            self.issuer
                .clone()
                .or(file.issuer)
                .or_else(|| saved.map(|a| a.issuer.clone())),
            "issuer",
        )?;
        let ingest = required(
            self.ingest_url
                .clone()
                .or(file.ingest_url)
                .or_else(|| saved.map(|a| a.ingest_url.clone())),
            "ingest_url",
        )?;
        Ok(Deployment {
            issuer: capture::endpoint(&issuer).context("invalid deployment issuer")?,
            ingest_url: capture::endpoint(&ingest).context("invalid deployment ingest origin")?,
            client_id: required(
                self.client_id
                    .clone()
                    .or(file.client_id)
                    .or_else(|| saved.map(|a| a.client_id.clone())),
                "client_id",
            )?,
            project_id: required(
                self.project_id
                    .clone()
                    .or(file.project_id)
                    .or_else(|| saved.and_then(|a| a.project_id.clone())),
                "project_id",
            )?,
        })
    }
}

impl Deployment {
    pub fn matches(&self, saved: &account::Account) -> Result<()> {
        ensure!(
            self.issuer == capture::endpoint(&saved.issuer)?
                && self.ingest_url == capture::endpoint(&saved.ingest_url)?
                && self.client_id == saved.client_id
                && Some(self.project_id.as_str()) == saved.project_id.as_deref(),
            "saved account belongs to another deployment; use a separate RROFLOW_CONFIG_DIR for each deployment (queues are never reassigned)"
        );
        Ok(())
    }
}

/// Validate ambient settings before refreshing or sending any saved credential.
pub(crate) fn check_saved(
    saved: &account::Account,
    file: Profile,
    env: impl Fn(&str) -> Option<String>,
) -> Result<()> {
    let issuer = env("RROFLOW_ISSUER")
        .or(file.issuer)
        .unwrap_or_else(|| saved.issuer.clone());
    let ingest = env("RROFLOW_INGEST_URL")
        .or(file.ingest_url)
        .unwrap_or_else(|| saved.ingest_url.clone());
    let client = env("RROFLOW_CLIENT_ID")
        .or(file.client_id)
        .unwrap_or_else(|| saved.client_id.clone());
    let project = env("RROFLOW_NOMEN_PROJECT")
        .or(file.project_id)
        .or_else(|| saved.project_id.clone());
    ensure!(
        capture::endpoint(&issuer)? == capture::endpoint(&saved.issuer)?
            && capture::endpoint(&ingest)? == capture::endpoint(&saved.ingest_url)?
            && client == saved.client_id
            && project == saved.project_id,
        "saved account belongs to another deployment; use a separate RROFLOW_CONFIG_DIR (credentials and queues retained)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(host: &str) -> Profile {
        Profile {
            issuer: Some(format!("https://id.{host}")),
            ingest_url: Some(format!("https://api.{host}")),
            client_id: Some("cli-a".into()),
            project_id: Some("project-a".into()),
        }
    }

    fn account() -> account::Account {
        serde_json::from_value(serde_json::json!({"issuer":"https://id.one.example", "client_id":"cli-a", "project_id":"project-a", "ingest_url":"https://api.one.example", "subject":"member-a", "access_token":"fixture", "expires_at":0, "signed_in_at":0})).unwrap()
    }

    #[test]
    fn cli_options_compose_without_collisions() {
        use clap::CommandFactory;
        crate::Cli::command().debug_assert();
    }

    #[test]
    fn alternate_deployments_and_precedence() {
        for host in ["one.example", "two.example"] {
            let config = Args::default().resolve_with(file(host), None).unwrap();
            assert_eq!(config.ingest_url, format!("https://api.{host}"));
        }
        let args = Args {
            client_id: Some("override-client".into()),
            ..Default::default()
        };
        assert_eq!(
            args.resolve_with(file("one.example"), None)
                .unwrap()
                .client_id,
            "override-client"
        );
        assert!(
            Args::default()
                .resolve_with(Profile::default(), None)
                .is_err()
        );
        assert!(
            Args {
                client_id: Some(" ".into()),
                ..Default::default()
            }
            .resolve_with(file("one.example"), None)
            .is_err()
        );
    }

    #[test]
    fn refuses_cross_deployment_credentials_in_every_identity_dimension() {
        let saved = account();
        check_saved(&saved, Profile::default(), |_| None).unwrap();
        check_saved(&saved, file("one.example"), |_| None).unwrap();
        assert!(check_saved(&saved, file("two.example"), |_| None).is_err());
        for field in [
            "RROFLOW_ISSUER",
            "RROFLOW_INGEST_URL",
            "RROFLOW_CLIENT_ID",
            "RROFLOW_NOMEN_PROJECT",
        ] {
            assert!(
                check_saved(&saved, Profile::default(), |key| (key == field)
                    .then(|| "https://other.example".into()))
                .is_err()
            );
        }
    }

    #[test]
    fn rejects_credentials_and_unsafe_origins() {
        for origin in [
            "http://127.0.0.1.evil.example",
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com/?token=secret",
            "https://example.com/#secret",
            "http://example.com",
        ] {
            let args = Args {
                issuer: Some(origin.into()),
                ..Default::default()
            };
            assert!(args.resolve_with(file("one.example"), None).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deployment.json");
        assert!(read(&path, true).is_err());
        assert!(read(&path, false).is_ok());
        std::fs::write(&path, r#"{"token":"do-not-print"}"#).unwrap();
        let error = read(&path, true).err().unwrap().to_string();
        assert!(!error.contains("do-not-print"));
    }
}
