//! Per-user startup. Manager definitions contain references, never account tokens.
use super::{Journal, lock};
use crate::{account, capture};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Subcommand, Clone, Copy)]
pub(crate) enum Action {
    /// Install and start this user's linked collector. Requires a saved rroflow login.
    Install,
    /// Read operating-system state; running is not proof of cloud delivery.
    Status,
    /// Start the previously installed collector.
    Start,
    /// Stop it now; leave automatic startup enabled.
    Stop,
    /// Disable and stop startup. Keep captured data, account and pinned files.
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installation {
    schema: String,
    source_id: String,
    store: PathBuf,
    account_file: PathBuf,
    sha256: String,
    binding_sha256: String,
}
impl Installation {
    fn name(&self) -> String {
        format!("rroflow-source-{}", self.source_id)
    }
    fn bundle(&self) -> PathBuf {
        self.store.join("startup").join(&self.sha256)
    }
    fn manifest(&self) -> PathBuf {
        self.bundle().join("installation.json")
    }
    fn executable(&self) -> PathBuf {
        self.bundle().join(if cfg!(windows) {
            "rroflow.exe"
        } else {
            "rroflow"
        })
    }
    fn unit(&self) -> PathBuf {
        self.bundle().join(format!("{}.service", self.name()))
    }
    fn validate(&self, store: &Path) -> Result<()> {
        ensure!(
            self.schema == "rroflow.startup/v1" && self.store == store,
            "startup identity mismatch"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.source_id)?.to_string() == self.source_id,
            "invalid startup source identity"
        );
        for hash in [&self.sha256, &self.binding_sha256] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "invalid startup digest"
            );
        }
        path_text(&self.store)?;
        path_text(&self.account_file)?;
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn path_text(path: &Path) -> Result<&str> {
    let s = path.to_str().context("startup needs a UTF-8 path")?;
    ensure!(
        path.is_absolute() && !s.chars().any(char::is_control),
        "startup needs an absolute path without control characters"
    );
    Ok(s)
}
fn absolute(path: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).context("startup path is unavailable")?;
    // Native Windows manager APIs take drive paths, not Rust's verbatim prefix.
    #[cfg(windows)]
    let path = {
        let s = path.to_str().context("startup needs a UTF-8 path")?;
        ensure!(
            !s.starts_with(r"\\?\UNC\"),
            "startup on a network share is not supported"
        );
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(s))
    };
    path_text(&path)?;
    Ok(path)
}
fn read_regular(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_file(),
        "startup files must be regular files"
    );
    Ok(fs::read(path)?)
}
fn pointer(store: &Path) -> PathBuf {
    store.join("startup").join("current.json")
}
fn load(store: &Path) -> Result<Option<Installation>> {
    if !pointer(store).try_exists()? {
        return Ok(None);
    }
    let install: Installation = serde_json::from_slice(&read_regular(&pointer(store))?)?;
    install.validate(store)?;
    let saved: Installation = serde_json::from_slice(&read_regular(&install.manifest())?)?;
    ensure!(
        install == saved,
        "startup manifest changed; refusing manager mutation"
    );
    Ok(Some(install))
}
fn verify_start(install: &Installation) -> Result<()> {
    ensure!(
        digest(&read_regular(&install.executable())?) == install.sha256,
        "pinned collector changed; startup refused"
    );
    ensure!(
        digest(&read_regular(&install.store.join("source.json"))?) == install.binding_sha256,
        "source binding changed; relink deliberately before reinstalling startup"
    );
    Ok(())
}

fn wait_for_stop(store: &Path) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match lock(store) {
            Ok(_released) => return Ok(()),
            Err(error) if std::time::Instant::now() >= deadline => {
                return Err(
                    error.context("startup stop has not released the collector; retry status/stop")
                );
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

// systemd unquotes arguments, expands % specifiers; ':' disables $ expansion.
fn unit_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    )
}
fn unit_text(install: &Installation) -> Result<String> {
    // systemd rejects quote characters in the executable itself even when escaped.
    // env executes the absolute pinned path as an argument, without a shell or PATH lookup.
    Ok(format!(
        "# rroflow.startup/v1 {}\n[Unit]\nDescription=RROFLOW session collector\nStartLimitIntervalSec=300\nStartLimitBurst=3\n[Service]\nType=exec\nExecStart=:/usr/bin/env -- {} sources managed-run --manifest {}\nRestart=on-failure\nRestartSec=15\nUMask=0077\nNoNewPrivileges=yes\n[Install]\nWantedBy=default.target\n",
        install.source_id,
        unit_quote(path_text(&install.executable())?),
        unit_quote(path_text(&install.manifest())?)
    ))
}
fn checked(command: &mut Command) -> Result<String> {
    let result = command
        .output()
        .context("startup manager could not be executed")?;
    ensure!(
        result.status.success(),
        "startup manager failed: {}",
        String::from_utf8_lossy(&result.stderr)
            .chars()
            .take(2000)
            .collect::<String>()
    );
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
#[cfg(target_os = "linux")]
fn systemctl(args: &[&str]) -> Result<String> {
    checked(Command::new("systemctl").arg("--user").args(args))
}

#[cfg(target_os = "linux")]
fn manage(install: &Installation, action: Action) -> Result<serde_json::Value> {
    let name = format!("{}.service", install.name());
    let unit = install.unit();
    ensure!(
        read_regular(&unit)? == unit_text(install)?.as_bytes(),
        "owned unit file changed; refusing manager mutation"
    );
    let fragment = systemctl(&["show", &name, "--property=FragmentPath", "--value"])?;
    ensure!(
        systemctl(&["show", &name, "--property=DropInPaths", "--value"])?.is_empty(),
        "startup unit has external overrides; nothing changed"
    );
    ensure!(
        fragment.is_empty() || absolute(Path::new(&fragment))? == absolute(&unit)?,
        "another unit owns this source name; nothing changed"
    );
    match action {
        Action::Install => {
            systemctl(&["enable", path_text(&unit)?])?;
            systemctl(&["daemon-reload"])?;
            systemctl(&["start", &name])?;
        }
        Action::Start => {
            ensure!(
                !fragment.is_empty(),
                "startup is not registered; reinstall it"
            );
            systemctl(&["reset-failed", &name])?;
            systemctl(&["start", &name])?;
        }
        Action::Stop if !fragment.is_empty() => {
            systemctl(&["stop", &name])?;
        }
        Action::Remove if !fragment.is_empty() => {
            systemctl(&["disable", "--now", &name])?;
            systemctl(&["daemon-reload"])?;
        }
        _ => {}
    }
    Ok(
        json!({"manager":"systemd-user","unit":name,"state":systemctl(&["show", &name,"--property=LoadState,ActiveState,SubState,UnitFileState,ExecMainStatus"] )?,"note":"OS state only; inspect source reports for delivery. Login startup; logout/boot persistence requires an explicitly configured user manager (linger)."}),
    )
}

#[cfg(windows)]
fn manage(install: &Installation, action: Action) -> Result<serde_json::Value> {
    use std::os::windows::process::CommandExt;
    let helper = install.bundle().join("manager.ps1");
    ensure!(
        read_regular(&helper)? == include_bytes!("service-windows.ps1"),
        "startup manager script changed"
    );
    ensure!(
        read_regular(&install.bundle().join("worker.ps1"))? == include_bytes!("service-worker.ps1"),
        "startup worker script changed"
    );
    let action = match action {
        Action::Install => "install",
        Action::Status => "status",
        Action::Start => "start",
        Action::Stop => "stop",
        Action::Remove => "remove",
    };
    let powershell = PathBuf::from(
        std::env::var_os("SystemRoot").context("Windows system directory unavailable")?,
    )
    .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let result = checked(
        Command::new(powershell)
            .creation_flags(0x08000000)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(helper)
            .args(["-Action", action, "-Manifest"])
            .arg(install.manifest()),
    )?;
    serde_json::from_str(&result).context("invalid startup manager response")
}
#[cfg(not(any(windows, target_os = "linux")))]
fn manage(_: &Installation, _: Action) -> Result<serde_json::Value> {
    anyhow::bail!("automatic startup currently supports Windows and systemd Linux");
}

pub(super) fn worker(manifest: &Path) -> Result<()> {
    let install: Installation = serde_json::from_slice(&read_regular(manifest)?)?;
    let store = absolute(&install.store)?;
    install.validate(&store)?;
    ensure!(
        absolute(manifest)? == absolute(&install.manifest())?,
        "unexpected startup manifest location"
    );
    ensure!(
        load(&store)?.as_ref() == Some(&install),
        "startup is no longer registered locally"
    );
    verify_start(&install)?;
    super::run(super::Command::Run {
        store: Some(store),
        interval_secs: 5,
        once: false,
        offline: false,
        account_file: Some(install.account_file),
        quiet: true,
    })
}

pub(super) fn run(store: &Path, action: Action) -> Result<()> {
    let store = absolute(store)?;
    let startup = store.join("startup");
    capture::private_dir(&startup)?;
    let control = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(startup.join("control.lock"))?;
    control
        .try_lock()
        .context("another startup operation is in progress")?;
    let mut install = load(&store)?;
    if matches!(action, Action::Install) {
        for key in ["RROFLOW_INGEST_KEY", "RROFLOW_INGEST_URL", "RROFLOW_REDACT"] {
            ensure!(
                std::env::var_os(key).is_none_or(|v| v.is_empty()),
                "automatic startup cannot persist {key}; use a saved rroflow login and persisted settings, or keep foreground collection"
            );
        }
        let account_file = absolute(&account::account_path()?)?;
        let account = account::read(&account_file)
            .map_err(anyhow::Error::msg)?
            .context("run rroflow login before installing startup")?;
        let journal = Journal::open(&store)?;
        ensure!(
            capture::endpoint(&account.ingest_url)? == journal.binding.endpoint,
            "saved sign-in endpoint differs from the linked source"
        );
        let executable = fs::read(std::env::current_exe()?)?;
        let desired = Installation {
            schema: "rroflow.startup/v1".into(),
            source_id: journal.binding.source_id.clone(),
            store: store.clone(),
            account_file,
            sha256: digest(&executable),
            binding_sha256: digest(&read_regular(&store.join("source.json"))?),
        };
        desired.validate(&store)?;
        if let Some(old) = &install {
            ensure!(
                *old == desired,
                "a different collector is installed; remove startup before upgrading (captured data is retained)"
            );
        } else {
            // Refuse takeover of an already running foreground collector.
            let _source_lock = lock(&store)?;
            capture::private_dir(&desired.bundle())?;
            capture::write_once(&desired.executable(), &executable)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(desired.executable(), fs::Permissions::from_mode(0o700))?;
            }
            capture::write_once(&desired.manifest(), &serde_json::to_vec_pretty(&desired)?)?;
            capture::write_once(&desired.unit(), unit_text(&desired)?.as_bytes())?;
            capture::write_once(
                &desired.bundle().join("worker.ps1"),
                include_bytes!("service-worker.ps1"),
            )?;
            capture::write_once(
                &desired.bundle().join("manager.ps1"),
                include_bytes!("service-windows.ps1"),
            )?;
            capture::write_once(&pointer(&store), &serde_json::to_vec_pretty(&desired)?)?;
        }
        install = Some(desired);
    }
    let Some(install) = install else {
        ensure!(
            matches!(action, Action::Status | Action::Remove),
            "startup is not installed; run sources service install"
        );
        println!("{}", json!({"installed":false,"captured_data":"retained"}));
        return Ok(());
    };
    if matches!(action, Action::Install | Action::Start) {
        verify_start(&install)?;
    }
    let state = manage(&install, action)?;
    if matches!(action, Action::Stop | Action::Remove) {
        // Task Scheduler acknowledges stop before its process tree has fully exited.
        // Do not report completion or remove local registration while capture still owns the journal.
        wait_for_stop(&store)?;
    }
    if matches!(action, Action::Remove) {
        fs::remove_file(pointer(&store))?;
        #[cfg(unix)]
        fs::File::open(&startup)?.sync_all()?;
    }
    println!(
        "{}",
        json!({"installed":!matches!(action,Action::Remove),"source_id":install.source_id,"binary_sha256":install.sha256,"manager_state":state,"captured_data":"retained"})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manager_arguments_preserve_paths_without_shell_or_specifier_expansion() {
        assert_eq!(
            unit_quote("/space $HOME/%n/quote\"/back\\"),
            "\"/space $HOME/%%n/quote\\\"/back\\\\\""
        );
        assert!(path_text(Path::new("relative")).is_err());
        #[cfg(unix)]
        assert!(path_text(Path::new("/bad\nExecStart=evil")).is_err());
    }
    #[test]
    fn manifests_reject_path_and_identity_substitution() {
        let dir = tempfile::tempdir().unwrap();
        let root = absolute(dir.path()).unwrap();
        let mut install = Installation {
            schema: "rroflow.startup/v1".into(),
            source_id: uuid::Uuid::new_v4().to_string(),
            store: root.clone(),
            account_file: root.join("account.json"),
            sha256: "a".repeat(64),
            binding_sha256: "b".repeat(64),
        };
        install.validate(&root).unwrap();
        install.sha256 = "../escape".into();
        assert!(install.validate(&root).is_err());
        install.sha256 = "a".repeat(64);
        install.source_id = "../../other".into();
        assert!(install.validate(&root).is_err());
    }
}
