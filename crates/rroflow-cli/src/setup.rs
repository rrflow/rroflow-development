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
use crate::{account, deployment, sources};
use anyhow::{Result, bail};
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

#[derive(clap::Args)]
pub(crate) struct Args {
    #[command(flatten)]
    deployment: deployment::Args,
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

    // 2. Resolve before refreshing any saved credential. A profile never changes
    // where an existing account or source queue belongs.
    anyhow::ensure!(
        std::env::var("RROFLOW_INGEST_KEY")
            .ok()
            .is_none_or(|v| v.trim().is_empty()),
        "setup uses member sign-in; for headless keys use sources link with explicit RROFLOW_INGEST_URL"
    );
    let saved = account::read(&account::account_path()?).map_err(anyhow::Error::msg)?;
    let selected = args.deployment.resolve(saved.as_ref())?;
    if let Some(saved) = &saved {
        selected.matches(saved)?;
    }
    if saved
        .as_ref()
        .is_none_or(|a| a.standing(account::unix_now()) == account::Standing::Expired)
    {
        println!("Sign in to {} to link this computer.", selected.ingest_url);
        account::login(
            &selected.issuer,
            &selected.client_id,
            Some(&selected.project_id),
            &selected.ingest_url,
        )?;
    } else {
        println!(
            "signed in: using the saved account for {}",
            selected.ingest_url
        );
    }

    // 3. Link (consent first).
    let store = account::config_dir()
        .map(|p| p.join("sources"))
        .ok_or_else(|| anyhow::anyhow!("source store unavailable"))?;
    if store.join("source.json").exists() {
        let sender = account::sender()?.ok_or_else(|| anyhow::anyhow!("sign-in unavailable"))?;
        sources::check_setup_binding(&store, &sender)?;
        println!("sources already linked to this account");
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
        "Collector startup is configured. Run `rroflow sources status` to check capture and delivery; `rroflow whoami` shows your destination."
    );
    Ok(())
}
