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
    /// Rebuild SDK monitoring projections from the immutable event journal.
    ActivityWorker {
        #[arg(long, env = "RROFLOW_INGEST_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "RROFLOW_DATABASE_URL", hide_env_values = true)]
        database_url: Option<String>,
        #[arg(long)]
        once: bool,
        /// Explicitly retry failed/capacity-limited captures after correcting the cause.
        #[arg(long)]
        retry_errors: bool,
    },
    /// Build consented metadata datasets outside the HTTP request process.
    DatasetWorker {
        #[arg(long, env = "RROFLOW_INGEST_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "RROFLOW_DATABASE_URL", hide_env_values = true)]
        database_url: Option<String>,
        #[arg(long)]
        once: bool,
    },
    /// Install revisioned server-owned allowances (operator access only; no billing action).
    Allowance {
        #[arg(long, env = "RROFLOW_INGEST_CONFIG")]
        config: PathBuf,
        #[arg(long, env = "RROFLOW_DATABASE_URL", hide_env_values = true)]
        database_url: Option<String>,
        /// JSON allowance-policy/v1, or allowance-grant/v1 when --devspace is supplied.
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        devspace: Option<String>,
    },
    /// Print immutable release identity without reading configuration or opening a store.
    BuildInfo,
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

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::ActivityWorker {
            config,
            database_url,
            once,
            retry_errors,
        } => {
            let cfg = load_config(&config, database_url)?;
            let Store::Postgres(pg) = cfg.open_store().await? else {
                anyhow::bail!("SDK activity worker needs Postgres");
            };
            if retry_errors {
                rroflow_ingest::sdk_activity::retry_errors(&pg).await?;
            }
            loop {
                let status = rroflow_ingest::sdk_activity::run_once(&pg, &cfg.sdk_activity).await?;
                println!("{status}");
                if once {
                    break;
                }
                tokio::select! {_=shutdown_signal()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(cfg.sdk_activity.poll_seconds))=>{}}
            }
        }
        Cmd::DatasetWorker {
            config,
            database_url,
            once,
        } => {
            let cfg = load_config(&config, database_url)?;
            let options = cfg
                .datasets
                .as_ref()
                .context("dataset worker needs a [datasets] configuration")?;
            let Store::Postgres(pg) = cfg.open_store().await? else {
                anyhow::bail!("dataset worker needs Postgres");
            };
            loop {
                match rroflow_ingest::datasets::run_once(&pg, options).await {
                    Ok(status) => println!("{status}"),
                    Err(_) if !once => {
                        eprintln!("dataset worker unavailable; retrying without publishing")
                    }
                    Err(_) => {
                        anyhow::bail!("dataset worker unavailable; no successful build is claimed")
                    }
                }
                if once {
                    break;
                }
                tokio::select! {
                    _=shutdown_signal()=>break,
                    _=tokio::time::sleep(std::time::Duration::from_secs(options.poll_seconds))=>{}
                }
            }
        }
        Cmd::Allowance {
            config,
            database_url,
            file,
            devspace,
        } => {
            let mut cfg = load_config(&config, database_url)?;
            // This command installs the policy required by subsequent Serve startup.
            cfg.require_allowance_policy = false;
            let Store::Postgres(pg) = cfg.open_store().await? else {
                anyhow::bail!("account allowances need Postgres");
            };
            let bytes = std::fs::read(file)?;
            if let Some(ds) = devspace {
                let grant: rroflow_ingest::allowances::Grant = serde_json::from_slice(&bytes)?;
                pg.set_allowance_grant(&ds, &grant).await?;
            } else {
                let policy: rroflow_ingest::allowances::Policy = serde_json::from_slice(&bytes)?;
                pg.set_allowance_policy(&policy).await?;
            }
            println!("Allowance revision saved. No billing or consent was changed.");
        }
        Cmd::BuildInfo => println!("{}", serde_json::to_string(&rroflow_ingest::build_info())?),
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
            cfg.validate_authentication()?;
            let store = cfg.open_store().await.context("open store")?;
            let where_ = match &store {
                Store::Postgres(_) => "postgres".to_string(),
                Store::Jsonl(_) => format!("jsonl in {}", cfg.data_dir.display()),
            };
            let store = Arc::new(store);
            let app = router(AppState::new(&cfg, store.clone())?, cfg.max_body_bytes);
            let listener = tokio::net::TcpListener::bind(&cfg.listen)
                .await
                .with_context(|| format!("bind {}", cfg.listen))?;
            eprintln!(
                "rroflow-ingest listening on {} ({} devspace(s), store: {where_})",
                cfg.listen,
                cfg.devspace.len()
            );
            let worker =
                rroflow_ingest::sdk_activity::start_worker(store, cfg.sdk_activity.clone());
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await;
            if let Some(worker) = worker {
                worker.abort();
            }
            result?;
        }
    }
    Ok(())
}
