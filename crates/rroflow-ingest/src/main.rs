//! `rroflow-ingest serve` runs the rroflow.ai ingest; `rroflow-ingest keygen` mints a
//! devspace ingest key and prints the config line that registers its hash.

use anyhow::Context;
use clap::{Parser, Subcommand};
use rand::RngCore;
use rroflow_ingest::{AppState, Config, Store, hash_key, router};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rroflow-ingest", version, about = "rroflow.ai ingest service")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the ingest API.
    Serve {
        /// TOML config (listen, database_url or data_dir, max_batch, max_body_bytes, [[devspace]]).
        #[arg(long, env = "RROFLOW_INGEST_CONFIG")]
        config: PathBuf,
        /// Postgres URL; overrides `database_url` in the config.
        #[arg(long, env = "RROFLOW_DATABASE_URL", hide_env_values = true)]
        database_url: Option<String>,
    },
    /// Make an existing devspace owned by a Nomen account, so signing in with that account
    /// opens it (for devspaces registered by key before sign-in existed).
    Claim {
        #[arg(long, env = "RROFLOW_INGEST_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "RROFLOW_DATABASE_URL", hide_env_values = true)]
        database_url: Option<String>,
        /// Devspace id to claim.
        #[arg(long)]
        devspace: String,
        /// Nomen account ID (`sub`), as `GET /v1/whoami` shows it after signing in.
        #[arg(long)]
        subject: String,
    },
    /// Mint a new ingest key for a devspace. The key is printed once; store it in the
    /// user's secret store and put only the printed hash in the config.
    Keygen {
        #[arg(long)]
        devspace: String,
    },
}

/// Read the TOML config; a database URL from the flag or environment wins.
fn load_config(path: &PathBuf, database_url: Option<String>) -> anyhow::Result<Config> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut cfg: Config = toml::from_str(&text).context("parse config")?;
    if database_url.is_some() {
        cfg.database_url = database_url;
    }
    Ok(cfg)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { devspace } => {
            let mut bytes = [0u8; 32];
            rand::rng().fill_bytes(&mut bytes);
            let key = format!("rrok_{}", hex::encode(bytes));
            println!("ingest key (shown once): {key}");
            println!(
                "\n[[devspace]]\nid = \"{devspace}\"\nkey_sha256 = [\"{}\"]",
                hash_key(&key)
            );
        }
        Cmd::Claim {
            config,
            database_url,
            devspace,
            subject,
        } => {
            let cfg = load_config(&config, database_url)?;
            let issuer = cfg
                .nomen
                .as_ref()
                .context("config has no [nomen] section: claiming needs its issuer")?
                .issuer
                .trim_end_matches('/')
                .to_string();
            rroflow_ingest::nomen::valid_subject(&subject).map_err(anyhow::Error::msg)?;
            let Store::Postgres(pg) = cfg.open_store().await.context("open store")? else {
                anyhow::bail!("claiming needs database_url (Postgres)");
            };
            pg.claim_devspace(&devspace, &issuer, &subject).await?;
            println!("devspace {devspace} is now owned by Nomen account {subject} at {issuer}");
        }
        Cmd::Serve {
            config,
            database_url,
        } => {
            let cfg = load_config(&config, database_url)?;
            anyhow::ensure!(
                !cfg.devspace.is_empty(),
                "config has no [[devspace]] entries"
            );
            let store = cfg.open_store().await.context("open store")?;
            let where_ = match &store {
                Store::Postgres(_) => "postgres".to_string(),
                Store::Jsonl(_) => format!("jsonl in {}", cfg.data_dir.display()),
            };
            let app = router(AppState::new(&cfg, Arc::new(store))?, cfg.max_body_bytes);
            let listener = tokio::net::TcpListener::bind(&cfg.listen)
                .await
                .with_context(|| format!("bind {}", cfg.listen))?;
            eprintln!(
                "rroflow-ingest listening on {} ({} devspace(s), store: {where_})",
                cfg.listen,
                cfg.devspace.len()
            );
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
    }
    Ok(())
}
