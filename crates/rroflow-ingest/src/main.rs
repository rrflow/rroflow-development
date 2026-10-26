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
    /// Mint a new ingest key for a devspace. The key is printed once; store it in the
    /// user's secret store and put only the printed hash in the config.
    Keygen {
        #[arg(long)]
        devspace: String,
    },
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
        Cmd::Serve {
            config,
            database_url,
        } => {
            let text = std::fs::read_to_string(&config)
                .with_context(|| format!("read {}", config.display()))?;
            let mut cfg: Config = toml::from_str(&text).context("parse config")?;
            if database_url.is_some() {
                cfg.database_url = database_url;
            }
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
