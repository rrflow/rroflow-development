//! `rroflow setup`: install once, sign in, and capture from then on
//! (docs/plan/sdk-install-and-linking.md increment 2, on the source collector).
//!
//! 1. Detect the agents' session folders on this machine and say what was found.
//! 2. Sign in with the person's Shippin account (Nomen device flow) unless already signed in.
//! 3. Link those folders to the devspace the sign-in opens (the credential decides it).
//! 4. Register the per-user collector so capture keeps running at login.
//!
//! Nothing leaves the machine before the person agrees (or passes `--yes`). Credentials
//! of the agents themselves are never read. Every step is skipped when already done.
use crate::{account, sources};
use anyhow::{Result, bail};
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

#[derive(clap::Args)]
pub(crate) struct Args {
    /// Nomen issuer.
    #[arg(long, env = "RROFLOW_ISSUER", default_value = account::DEFAULT_ISSUER)]
    issuer: String,
    /// The rroflow CLI's application in Nomen.
    #[arg(long, env = "RROFLOW_CLIENT_ID", default_value = account::DEFAULT_CLIENT_ID)]
    client_id: String,
    /// The rroflow project id in Nomen, asked for in the token's audience.
    #[arg(long, env = "RROFLOW_NOMEN_PROJECT", default_value = account::DEFAULT_PROJECT_ID)]
    project_id: String,
    /// The rroflow.ai ingest this account sends to.
    #[arg(long, default_value = account::DEFAULT_INGEST_URL)]
    ingest_url: String,
    /// Also send scrubbed prompt, response and tool text. Default: metrics and signals only.
    #[arg(long)]
    content: bool,
    /// Link and capture without asking (for scripted installs). Consent is still printed.
    #[arg(long, short = 'y')]
    yes: bool,
    /// Link only; do not register automatic startup.
    #[arg(long)]
    no_service: bool,
}

fn count_sessions(root: &Path) -> usize {
    fn walk(p: &Path, n: &mut usize) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                walk(&path, n);
            } else if ft.is_file() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.ends_with(".jsonl") || name.ends_with(".jsonl.zst") {
                    *n += 1;
                }
            }
        }
    }
    let mut n = 0;
    walk(root, &mut n);
    n
}

fn ask(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!("{question} needs an answer: re-run in a terminal, or pass --yes to agree");
    }
    print!("{question} [Y/n] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let a = line.trim().to_ascii_lowercase();
    Ok(a.is_empty() || a == "y" || a == "yes")
}

pub(crate) fn run(args: Args) -> Result<()> {
    // 1. Detect.
    let claude = sources::default_root("CLAUDE_CONFIG_DIR", ".claude", "projects")?;
    let codex = sources::default_root("CODEX_HOME", ".codex", "sessions")?;
    let mut found = 0;
    for (name, root) in [("Claude Code", &claude), ("Codex", &codex)] {
        if root.is_dir() {
            let n = count_sessions(root);
            found += 1;
            println!("found {name}: {} ({n} session file(s))", root.display());
        } else {
            println!("not found: {name} ({} does not exist yet)", root.display());
        }
    }
    if found == 0 {
        println!(
            "No agent sessions yet; linking anyway so sessions are captured once they appear."
        );
    }

    // 2. Sign in.
    if account::sender()?.is_none() {
        println!("Sign in with your Shippin account to choose where these sessions go.");
        account::login(
            &args.issuer,
            &args.client_id,
            Some(&args.project_id),
            &args.ingest_url,
        )?;
    } else {
        println!("signed in: using the saved rroflow account");
    }

    // 3. Link (consent first).
    let store = account::config_dir()
        .map(|p| p.join("sources"))
        .ok_or_else(|| anyhow::anyhow!("source store unavailable"))?;
    if store.join("source.json").exists() {
        println!("sources already linked");
    } else {
        let what = if args.content {
            "metrics, signal results and scrubbed prompt/response/tool text"
        } else {
            "metrics and signal results (no prompt or response text)"
        };
        println!(
            "Linking sends {what} from every session under these folders, past and future, to your devspace."
        );
        if !args.yes && !ask("Link these agents and capture their turns from now on?")? {
            println!("Nothing linked. Run `rroflow setup` again any time.");
            return Ok(());
        }
        sources::run(sources::Command::Link {
            store: None,
            claude_root: None,
            codex_root: None,
            content: args.content,
        })?;
    }

    // 4. Keep it running.
    if args.no_service {
        println!(
            "Automatic startup not registered (--no-service). Run `rroflow sources run` to collect."
        );
        return Ok(());
    }
    let installed = sources::run(sources::Command::Service {
        store: None,
        command: sources::service::Action::Install,
    });
    if let Err(e) = installed {
        let msg = format!("{e:#}");
        if msg.contains("already") {
            println!("automatic startup already registered");
        } else {
            return Err(e);
        }
    }
    sources::run(sources::Command::Service {
        store: None,
        command: sources::service::Action::Status,
    })?;
    println!(
        "Done. Your sessions now report to your rroflow devspace; see `rroflow whoami` and https://app.rroflow.ai."
    );
    Ok(())
}
