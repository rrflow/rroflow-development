//! The account this machine sends as: `rroflow login`, `logout`, `whoami`.
//!
//! Ported from the Devagotchi CLI (`EonsofStupid/devagotchi`, `crates/hub/src/account.rs` at
//! b5d974a); the HTTP layer is blocking here because this CLI is. Keep the two in step until
//! they share a crate.
//!
//! Sign-in is OAuth device authorization (RFC 8628) against Nomen, Shippin's identity
//! provider: the terminal shows a short code and a URL, the person opens it on any device,
//! and the tokens arrive here. No browser on this machine, no local port, no secret in the
//! binary: the CLI is a public client. The tokens rest in one owner-only file under the
//! config directory (`account.json`, 0600, written whole and renamed into place). The
//! ingest checks the access token by introspection, so a stolen file is a stolen token and
//! nothing more, and `logout` revokes it.
//!
//! The access token is refreshed when within five minutes of expiry. When Nomen cannot be
//! reached, the account on file stays "signed in (offline)" for seven days past expiry,
//! then "expired"; nothing here deletes it but `logout`.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const DEFAULT_ISSUER: &str = "https://id.shippin.cloud";
/// `rroflow-cli`, the native application in Nomen's `rroflow` project (device
/// authorization and refresh tokens, no secret).
pub(crate) const DEFAULT_CLIENT_ID: &str = "396821568421953540";
/// The `rroflow` project in Nomen (Shippin Platform org): the audience the ingest accepts.
pub(crate) const DEFAULT_PROJECT_ID: &str = "396821566727389188";
pub(crate) const DEFAULT_INGEST_URL: &str = "https://api.rroflow.ai";
pub(crate) const FILE_NAME: &str = "account.json";
const SCOPE: &str = "openid profile email offline_access urn:zitadel:iam:user:resourceowner";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Refresh when the access token is this close to expiry.
const REFRESH_AHEAD: Duration = Duration::from_secs(5 * 60);
/// Past expiry, the account on file still counts as signed in (offline) for this long.
pub(crate) const OFFLINE_GRACE: Duration = Duration::from_secs(7 * 24 * 3600);
/// A device code the person never confirms is given up on after this long at most.
const MAX_WAIT: Duration = Duration::from_secs(15 * 60);
const MAX_ANSWER_BYTES: u64 = 64 * 1024;

/// The scopes asked for: who the person is, a refresh token, their org, and, when the
/// rroflow project id is given, that project in the token's audience.
fn scope(project_id: Option<&str>) -> String {
    match project_id {
        Some(p) => format!("{SCOPE} urn:zitadel:iam:org:project:id:{p}:aud"),
        None => SCOPE.to_string(),
    }
}

/// A signed-in account. The tokens are never printed; `Debug` shows the rest.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Account {
    pub issuer: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// The rroflow.ai ingest this account sends to.
    pub ingest_url: String,
    /// The Nomen account ID (`sub`), which never changes; the devspace is owned by it.
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    pub signed_in_at: u64,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("subject", &self.subject)
            .field("username", &self.username)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// The account's standing at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// The access token is good for a while.
    Fresh,
    /// Within `REFRESH_AHEAD` of expiry, or past it: refresh before use.
    Refresh,
    /// Past expiry and no refresh succeeded; still the account on file, for now.
    Offline { grace_ends_at: u64 },
    /// Past the grace: sign in again.
    Expired,
}

impl Account {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn standing(&self, now: u64) -> Standing {
        if now + REFRESH_AHEAD.as_secs() < self.expires_at {
            Standing::Fresh
        } else if now < self.expires_at {
            Standing::Refresh
        } else if now < self.expires_at + OFFLINE_GRACE.as_secs() {
            Standing::Offline {
                grace_ends_at: self.expires_at + OFFLINE_GRACE.as_secs(),
            }
        } else {
            Standing::Expired
        }
    }

    /// `username` when Nomen gave one, else the account ID.
    pub fn display_name(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.subject)
    }
}

/// `RROFLOW_CONFIG_DIR`, else `%APPDATA%\rroflow` on Windows, else
/// `$XDG_CONFIG_HOME/rroflow` or `~/.config/rroflow`.
pub(crate) fn config_dir() -> Option<PathBuf> {
    let abs = |k: &str| {
        std::env::var_os(k)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if let Some(d) = abs("RROFLOW_CONFIG_DIR") {
        return Some(d);
    }
    if cfg!(windows) {
        return abs("APPDATA").map(|d| d.join("rroflow"));
    }
    abs("XDG_CONFIG_HOME")
        .or_else(|| abs("HOME").map(|h| h.join(".config")))
        .map(|d| d.join("rroflow"))
}

pub(crate) fn account_path() -> anyhow::Result<PathBuf> {
    config_dir()
        .map(|d| d.join(FILE_NAME))
        .ok_or_else(|| anyhow::anyhow!("no config directory: set RROFLOW_CONFIG_DIR"))
}

/// Read what is on file; `None` when nothing is. A file this binary cannot read is an
/// error naming the file, never its contents.
pub(crate) fn read(path: &Path) -> Result<Option<Account>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            format!(
                "{} is not an account file; `rroflow logout` removes it",
                path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Write the file whole, owner-only, and rename it into place.
pub(crate) fn write(path: &Path, account: &Account) -> Result<(), String> {
    let text = serde_json::to_string_pretty(account).map_err(|e| e.to_string())? + "\n";
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", tmp.display()))?;
    }
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn remove(path: &Path) -> Result<bool, String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Why a sign-in or refresh did not produce an account.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The person declined, the code expired, or Nomen refused the client or the grant.
    Refused(String),
    /// Nomen could not be reached or did not answer as an issuer does.
    Unavailable(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused(why) => write!(f, "refused: {why}"),
            Refusal::Unavailable(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for Refusal {}

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    expires_in: u64,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Deserialize, Default)]
struct OAuthError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct UserInfo {
    sub: String,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

/// An account ID as Nomen states it: no spaces or control characters, not an email.
fn valid_subject(subject: &str) -> Result<(), String> {
    if subject.is_empty() || subject.len() > 256 {
        return Err(format!("{subject:?} must be 1..=256 characters"));
    }
    if subject.chars().any(|c| c.is_whitespace() || c.is_control()) || subject.contains('@') {
        return Err(format!("{subject:?} is not an account ID"));
    }
    Ok(())
}

/// The endpoints of one issuer, in Nomen's (Zitadel's) layout.
pub(crate) struct Issuer {
    issuer: String,
    client_id: String,
    project_id: Option<String>,
    http: ureq::Agent,
}

impl Issuer {
    pub fn new(issuer: &str, client_id: &str, project_id: Option<&str>) -> Result<Self, String> {
        let issuer = issuer.trim_end_matches('/');
        if !(issuer.starts_with("https://") || issuer.starts_with("http://127.0.0.1")) {
            return Err("issuer must be an https:// URL".into());
        }
        if client_id.trim().is_empty() {
            return Err("client id must not be empty".into());
        }
        let http: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .timeout_connect(Some(Duration::from_secs(5)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            issuer: issuer.to_string(),
            client_id: client_id.to_string(),
            project_id: project_id.map(str::to_string).filter(|p| !p.is_empty()),
            http,
        })
    }

    pub fn for_account(account: &Account) -> Result<Self, String> {
        Self::new(
            &account.issuer,
            &account.client_id,
            account.project_id.as_deref(),
        )
    }

    /// Sign in: show the code, wait for the person, land the tokens. `tell` gets the
    /// lines a person reads (the code and the URL) as they are known.
    pub fn login(&self, ingest_url: &str, tell: &mut dyn Write) -> Result<Account, Refusal> {
        let started = unix_now();
        let scope = scope(self.project_id.as_deref());
        let auth: DeviceAuthorization = self.post(
            "/oauth/v2/device_authorization",
            &[("client_id", self.client_id.as_str()), ("scope", &scope)],
        )?;
        let _ = writeln!(
            tell,
            "To sign in, open {} and enter the code {}",
            auth.verification_uri, auth.user_code
        );
        if let Some(complete) = &auth.verification_uri_complete {
            let _ = writeln!(tell, "  (or open {complete})");
        }
        let _ = writeln!(tell, "Waiting for you to confirm there…");
        let mut interval = Duration::from_secs(auth.interval.unwrap_or(5).max(1));
        let deadline = Duration::from_secs(auth.expires_in).min(MAX_WAIT);
        let expired = || {
            Refusal::Refused(
                "the code expired before it was confirmed; run `rroflow login` again".into(),
            )
        };
        loop {
            std::thread::sleep(interval);
            if Duration::from_secs(unix_now().saturating_sub(started)) > deadline {
                return Err(expired());
            }
            let answer: Result<TokenAnswer, Refusal> = self.post(
                "/oauth/v2/token",
                &[
                    ("grant_type", DEVICE_GRANT),
                    ("device_code", auth.device_code.as_str()),
                    ("client_id", self.client_id.as_str()),
                ],
            );
            match answer {
                Ok(tokens) => return self.account_from(tokens, ingest_url, unix_now()),
                Err(Refusal::Refused(code)) if code == "authorization_pending" => {}
                Err(Refusal::Refused(code)) if code == "slow_down" => {
                    interval += Duration::from_secs(5);
                }
                Err(Refusal::Refused(code)) if code == "access_denied" => {
                    return Err(Refusal::Refused("you declined the sign-in".into()));
                }
                Err(Refusal::Refused(code)) if code == "expired_token" => return Err(expired()),
                Err(other) => return Err(other),
            }
        }
    }

    /// A fresh access token from the refresh token; the account keeps its identity.
    pub fn refresh(&self, account: &Account) -> Result<Account, Refusal> {
        let Some(refresh_token) = account.refresh_token.as_deref() else {
            return Err(Refusal::Refused(
                "no refresh token on file; run `rroflow login` again".into(),
            ));
        };
        let scope = scope(self.project_id.as_deref());
        let tokens: TokenAnswer = self.post(
            "/oauth/v2/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", self.client_id.as_str()),
                ("scope", &scope),
            ],
        )?;
        let now = unix_now();
        Ok(Account {
            access_token: tokens.access_token,
            refresh_token: tokens
                .refresh_token
                .or_else(|| account.refresh_token.clone()),
            expires_at: now + tokens.expires_in.unwrap_or(3600),
            ..account.clone()
        })
    }

    /// Tell Nomen the tokens are done with (RFC 7009); best effort, the file goes either way.
    pub fn revoke(&self, account: &Account) -> Result<(), Refusal> {
        for token in [
            Some(account.access_token.as_str()),
            account.refresh_token.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let response = self
                .http
                .post(format!("{}/oauth/v2/revoke", self.issuer))
                .send_form([("token", token), ("client_id", self.client_id.as_str())])
                .map_err(|error| Refusal::Unavailable(format!("Nomen revoke: {error}")))?;
            if !response.status().is_success() {
                return Err(Refusal::Unavailable(format!(
                    "Nomen revoke answered {}",
                    response.status()
                )));
            }
        }
        Ok(())
    }

    fn account_from(
        &self,
        tokens: TokenAnswer,
        ingest_url: &str,
        now: u64,
    ) -> Result<Account, Refusal> {
        let response = self
            .http
            .get(format!("{}/oidc/v1/userinfo", self.issuer))
            .header("Authorization", &format!("Bearer {}", tokens.access_token))
            .call()
            .map_err(|error| Refusal::Unavailable(format!("Nomen userinfo: {error}")))?;
        if !response.status().is_success() {
            return Err(Refusal::Unavailable(format!(
                "Nomen userinfo answered {}",
                response.status()
            )));
        }
        let info: UserInfo = read_json(response)?;
        valid_subject(&info.sub)
            .map_err(|why| Refusal::Unavailable(format!("Nomen's account id {why}")))?;
        Ok(Account {
            issuer: self.issuer.clone(),
            client_id: self.client_id.clone(),
            project_id: self.project_id.clone(),
            ingest_url: ingest_url.trim_end_matches('/').to_string(),
            subject: info.sub,
            username: info.preferred_username.filter(|u| !u.is_empty()),
            email: info.email.filter(|e| !e.is_empty()),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: now + tokens.expires_in.unwrap_or(3600),
            signed_in_at: now,
        })
    }

    /// A form POST to the issuer; an OAuth error body becomes `Refused(error code)`.
    fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, &str)],
    ) -> Result<T, Refusal> {
        let response = self
            .http
            .post(format!("{}{path}", self.issuer))
            .send_form(form.iter().copied())
            .map_err(|error| Refusal::Unavailable(format!("Nomen {path}: {error}")))?;
        let status = response.status();
        if status.is_success() {
            return read_json(response);
        }
        if status.is_client_error() {
            let error: OAuthError = read_json(response).unwrap_or_default();
            if error.error.is_empty() {
                return Err(Refusal::Unavailable(format!(
                    "Nomen {path} answered {status}"
                )));
            }
            return Err(Refusal::Refused(match error.error_description {
                Some(why)
                    if !matches!(
                        error.error.as_str(),
                        "authorization_pending" | "slow_down" | "access_denied" | "expired_token"
                    ) =>
                {
                    format!("{} ({why})", error.error)
                }
                _ => error.error,
            }));
        }
        Err(Refusal::Unavailable(format!(
            "Nomen {path} answered {status}"
        )))
    }
}

fn read_json<T: serde::de::DeserializeOwned>(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T, Refusal> {
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_ANSWER_BYTES)
        .read_to_vec()
        .map_err(|error| Refusal::Unavailable(format!("Nomen: {error}")))?;
    serde_json::from_slice(&body)
        .map_err(|_| Refusal::Unavailable("Nomen's answer is not the JSON an issuer sends".into()))
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// The account on file, refreshed when due and Nomen answers; otherwise as it stands.
/// What comes back is written to the file only when the refresh succeeded.
pub(crate) fn current(path: &Path) -> Result<Option<Account>, String> {
    let stored = read(path)?;
    let Some(account) = &stored else {
        return Ok(None);
    };
    if matches!(
        account.standing(unix_now()),
        Standing::Fresh | Standing::Expired
    ) {
        return Ok(stored);
    }
    let Ok(nomen) = Issuer::for_account(account) else {
        return Ok(stored);
    };
    match nomen.refresh(account) {
        Ok(refreshed) => {
            write(path, &refreshed)?;
            Ok(Some(refreshed))
        }
        Err(_) => Ok(stored),
    }
}

pub(crate) fn standing_name(standing: Standing) -> String {
    match standing {
        Standing::Fresh | Standing::Refresh => "signed in".into(),
        Standing::Offline { grace_ends_at } => format!(
            "signed in (offline: Nomen unreachable; sign in again within {} h)",
            grace_ends_at.saturating_sub(unix_now()) / 3600
        ),
        Standing::Expired => "expired: run `rroflow login`".into(),
    }
}

/// Where and as whom the collector sends: an ingest key from `RROFLOW_INGEST_KEY` (headless
/// machines, CI), else the signed-in account, refreshed when due.
pub(crate) struct Sender {
    pub base: String,
    pub token: String,
    /// `key` or `account <name>`, for messages.
    pub via: String,
}

/// The sender for this run; `None` when there is neither a key nor an account on file.
pub(crate) fn sender() -> anyhow::Result<Option<Sender>> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let account = current(&account_path()?).map_err(anyhow::Error::msg)?;
    let base = env("RROFLOW_INGEST_URL")
        .or_else(|| account.as_ref().map(|a| a.ingest_url.clone()))
        .unwrap_or_else(|| DEFAULT_INGEST_URL.to_string());
    if let Some(key) = env("RROFLOW_INGEST_KEY") {
        return Ok(Some(Sender {
            base,
            token: key,
            via: "key".into(),
        }));
    }
    let Some(account) = account else {
        return Ok(None);
    };
    if account.standing(unix_now()) == Standing::Expired {
        anyhow::bail!(
            "the sign-in for {} has expired: run `rroflow login`",
            account.display_name()
        );
    }
    Ok(Some(Sender {
        base,
        token: account.access_token().to_string(),
        via: format!("account {}", account.display_name()),
    }))
}

/// `rroflow login`: device sign-in, the account written owner-only, then the devspace
/// the ingest opens for it.
pub(crate) fn login(
    issuer: &str,
    client_id: &str,
    project_id: Option<&str>,
    ingest_url: &str,
) -> anyhow::Result<()> {
    let nomen = Issuer::new(issuer, client_id, project_id).map_err(anyhow::Error::msg)?;
    let account = nomen.login(ingest_url, &mut std::io::stdout())?;
    let path = account_path()?;
    write(&path, &account).map_err(anyhow::Error::msg)?;
    println!(
        "Signed in as {} (account {}). Tokens kept in {}, readable by you alone.",
        account.display_name(),
        account.subject,
        path.display()
    );
    match crate::api::get_rroflow(&account.ingest_url, account.access_token(), "/v1/whoami") {
        Ok(who) => println!(
            "rroflow.ai devspace: {} at {}",
            who["devspace"].as_str().unwrap_or("?"),
            account.ingest_url
        ),
        Err(e) => println!(
            "Could not reach the ingest at {} yet: {e:#}",
            account.ingest_url
        ),
    }
    Ok(())
}

/// `rroflow logout`: revoke the tokens at Nomen (best effort) and remove the file.
pub(crate) fn logout() -> anyhow::Result<()> {
    let path = account_path()?;
    match read(&path).map_err(anyhow::Error::msg)? {
        Some(account) => {
            match Issuer::for_account(&account).map(|n| n.revoke(&account)) {
                Ok(Ok(())) => println!("Tokens revoked at {}.", account.issuer),
                Ok(Err(why)) => println!(
                    "Could not revoke the tokens at {} ({why}); they expire on their own.",
                    account.issuer
                ),
                Err(why) => println!("Not revoking: {why}"),
            }
            remove(&path).map_err(anyhow::Error::msg)?;
            println!("Signed out ({} removed).", path.display());
        }
        None => println!("Not signed in."),
    }
    Ok(())
}

/// `rroflow whoami`: the account on file and the devspace the ingest opens for this
/// machine's credential.
pub(crate) fn whoami(json: bool) -> anyhow::Result<()> {
    let account = current(&account_path()?).map_err(anyhow::Error::msg)?;
    let sender = sender()?;
    let devspace = sender
        .as_ref()
        .map(|s| crate::api::get_rroflow(&s.base, &s.token, "/v1/whoami"));
    if json {
        let out = serde_json::json!({
            "account": account.as_ref().map(|a| serde_json::json!({
                "issuer": a.issuer, "subject": a.subject, "username": a.username,
                "standing": standing_name(a.standing(unix_now())), "ingest_url": a.ingest_url,
            })),
            "sending_as": sender.as_ref().map(|s| &s.via),
            "ingest": sender.as_ref().map(|s| &s.base),
            "devspace": devspace.as_ref().and_then(|d| d.as_ref().ok()),
            "devspace_error": devspace.as_ref().and_then(|d| d.as_ref().err().map(|e| format!("{e:#}"))),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    match &account {
        Some(a) => println!(
            "{} (account {}) at {}: {}",
            a.display_name(),
            a.subject,
            a.issuer,
            standing_name(a.standing(unix_now()))
        ),
        None => println!("Not signed in: `rroflow login` signs in with your Shippin account."),
    }
    match (&sender, &devspace) {
        (Some(s), Some(Ok(who))) => println!(
            "Sending as {} to {}: devspace {}",
            s.via,
            s.base,
            who["devspace"].as_str().unwrap_or("?")
        ),
        (Some(s), Some(Err(e))) => println!("Sending as {} to {}: {e:#}", s.via, s.base),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests;
