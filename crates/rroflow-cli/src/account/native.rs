//! Native RROFLOW grants. A refresh intent is durable before any network request.
use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Grant {
    pub id: String,
    pub project_id: String,
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_request: Option<String>,
    #[serde(default)]
    pub invalidated: bool,
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_at: u64,
    grant_expires_at: u64,
    scope: String,
    subject: String,
    project_id: String,
    grant_id: String,
}
fn secret(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn request_id() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn unavailable() -> Refusal {
    Refusal::Unavailable(
        "native sign-in returned an invalid response; credentials were not replaced".into(),
    )
}

// Reuse only transport configuration; native calls never use Nomen's endpoints.
fn client(issuer: &str, client_id: &str) -> Result<Issuer, Refusal> {
    if client_id.is_empty()
        || client_id.len() > 128
        || !client_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
        return Err(unavailable());
    }
    Issuer::new(issuer, client_id, None).map_err(Refusal::Unavailable)
}
fn post<T: serde::de::DeserializeOwned>(
    issuer: &Issuer,
    path: &str,
    form: &[(&str, &str)],
) -> Result<T, Refusal> {
    let response = issuer
        .http
        .post(format!("{}/auth/native/device/{path}", issuer.issuer))
        .send_form(form.iter().copied())
        .map_err(|_| {
            Refusal::Unavailable("native sign-in service could not be reached; retry safely".into())
        })?;
    let status = response.status();
    if status.is_success() {
        return read_json(response).map_err(|_| unavailable());
    }
    if status.is_client_error() {
        let e: OAuthError = read_json(response).map_err(|_| unavailable())?;
        // Never print an arbitrary error description or server-supplied credential.
        if matches!(
            e.error.as_str(),
            "authorization_pending"
                | "slow_down"
                | "access_denied"
                | "expired_token"
                | "invalid_grant"
                | "invalid_client"
                | "unauthorized_client"
                | "invalid_scope"
                | "invalid_request"
        ) {
            return Err(Refusal::Refused(e.error));
        }
    }
    Err(Refusal::Unavailable(format!(
        "native sign-in service answered {status}; retry safely"
    )))
}
fn validate(account: &Account) -> Result<&Grant, Refusal> {
    let g = account.native.as_ref().ok_or_else(unavailable)?;
    if account.auth_method != AuthMethod::Native
        || account.project_id.is_some()
        || !secret(&g.id, "ndg_")
        || !secret(&account.access_token, "rrnc_")
        || !account
            .refresh_token
            .as_deref()
            .is_some_and(|t| secret(t, "rrnr_"))
        || valid_subject(&account.subject).is_err()
        || valid_subject(&g.project_id).is_err()
        || g.refresh_request.as_ref().is_some_and(|s| !secret(s, ""))
    {
        return Err(unavailable());
    }
    Ok(g)
}
fn apply(account: &Account, t: Tokens, first: bool) -> Result<Account, Refusal> {
    if t.token_type != "Bearer"
        || t.scope != "capture"
        || !secret(&t.access_token, "rrnc_")
        || !secret(&t.refresh_token, "rrnr_")
        || !secret(&t.grant_id, "ndg_")
        || valid_subject(&t.subject).is_err()
        || valid_subject(&t.project_id).is_err()
        || t.grant_expires_at <= unix_now()
        || t.expires_at > t.grant_expires_at
        || t.expires_at > unix_now().saturating_add(3600 + 60)
        || t.grant_expires_at > unix_now().saturating_add(31536000 + 60)
    {
        return Err(unavailable());
    }
    if !first {
        let g = validate(account)?;
        if t.subject != account.subject
            || t.project_id != g.project_id
            || t.grant_id != g.id
            || t.grant_expires_at != g.expires_at
        {
            return Err(unavailable());
        }
    }
    Ok(Account {
        subject: t.subject,
        access_token: t.access_token,
        refresh_token: Some(t.refresh_token),
        expires_at: t.expires_at,
        native: Some(Grant {
            id: t.grant_id,
            project_id: t.project_id,
            expires_at: t.grant_expires_at,
            refresh_request: None,
            invalidated: false,
        }),
        ..account.clone()
    })
}

pub(super) fn login(
    selected: &crate::deployment::Deployment,
    tell: &mut dyn Write,
) -> Result<Account, Refusal> {
    let issuer = client(&selected.issuer, &selected.client_id)?;
    let auth: DeviceAuthorization = post(
        &issuer,
        "start",
        &[
            ("client_id", &selected.client_id),
            ("resource", &selected.ingest_url),
            ("scope", "capture"),
            ("device_name", &selected.device_name),
        ],
    )?;
    // Browser and API origins may differ, but neither may carry URL credentials.
    let browser = auth
        .verification_uri
        .strip_suffix("/app/device")
        .ok_or_else(unavailable)?;
    crate::capture::endpoint(browser).map_err(|_| unavailable())?;
    let valid_code = auth.user_code.len() == 11
        && auth.user_code.as_bytes()[5] == b'-'
        && auth
            .user_code
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 5 || b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ".contains(&b));
    if !valid_code
        || !secret(&auth.device_code, "rrnd_")
        || !(1..=900).contains(&auth.expires_in)
        || auth.verification_uri_complete.is_some()
    {
        return Err(unavailable());
    }
    let mut interval = Duration::from_secs(auth.interval.unwrap_or(5).max(5));
    if interval > MAX_WAIT {
        return Err(unavailable());
    }
    let started = std::time::Instant::now();
    let deadline = Duration::from_secs(auth.expires_in).min(MAX_WAIT);
    let request = request_id();
    writeln!(
        tell,
        "To connect this collector, open {} and enter the code {}",
        auth.verification_uri, auth.user_code
    )
    .map_err(|_| unavailable())?;
    tell.flush().map_err(|_| unavailable())?;
    let pending = Account {
        auth_method: AuthMethod::Native,
        native: None,
        issuer: selected.issuer.clone(),
        client_id: selected.client_id.clone(),
        project_id: None,
        ingest_url: selected.ingest_url.clone(),
        subject: String::new(),
        username: None,
        email: None,
        access_token: String::new(),
        refresh_token: None,
        expires_at: 0,
        signed_in_at: unix_now(),
    };
    loop {
        std::thread::sleep(interval.min(deadline.saturating_sub(started.elapsed())));
        if started.elapsed() >= deadline {
            return Err(Refusal::Refused(
                "the connection code expired; run rroflow login again".into(),
            ));
        }
        let result = post(
            &issuer,
            "token",
            &[
                ("client_id", &selected.client_id),
                ("grant_type", DEVICE_GRANT),
                ("device_code", &auth.device_code),
                ("request_id", &request),
            ],
        );
        match result {
            Ok(t) => return apply(&pending, t, true),
            Err(Refusal::Refused(e)) if e == "authorization_pending" => {}
            Err(Refusal::Refused(e)) if e == "slow_down" => {
                interval = (interval + Duration::from_secs(5)).min(MAX_WAIT)
            }
            Err(Refusal::Unavailable(_)) => interval = (interval * 2).min(MAX_WAIT),
            Err(e) => return Err(e),
        }
    }
}

pub(super) fn current(
    locked: &storage::LockedAccount,
    mut account: Account,
) -> Result<Account, String> {
    validate(&account).map_err(|e| e.to_string())?;
    if matches!(
        account.standing(unix_now()),
        Standing::Fresh | Standing::Expired
    ) {
        return Ok(account);
    }
    let issuer = client(&account.issuer, &account.client_id).map_err(|e| e.to_string())?;
    // At most two exchanges: recover a committed response, then rotate if its
    // access token expired during the outage. Save recovery BEFORE rotating again.
    for _ in 0..2 {
        let grant = account.native.as_mut().unwrap();
        if grant.refresh_request.is_none() {
            grant.refresh_request = Some(request_id());
            locked.write(&account)?;
        }
        let result = post(
            &issuer,
            "token",
            &[
                ("client_id", &account.client_id),
                ("grant_type", "refresh_token"),
                ("refresh_token", account.refresh_token.as_deref().unwrap()),
                (
                    "request_id",
                    account
                        .native
                        .as_ref()
                        .unwrap()
                        .refresh_request
                        .as_deref()
                        .unwrap(),
                ),
            ],
        );
        let tokens = match result {
            Ok(t) => t,
            Err(Refusal::Refused(e)) if matches!(e.as_str(), "invalid_grant" | "access_denied") => {
                account.native.as_mut().unwrap().invalidated = true;
                locked.write(&account)?;
                return Err(
                    "collector authorization ended; run rroflow login (queued captures retained)"
                        .into(),
                );
            }
            Err(e) => {
                return Err(format!(
                    "{e}; saved refresh intent and queued captures retained"
                ));
            }
        };
        account = apply(&account, tokens, false).map_err(|e| e.to_string())?;
        locked.write(&account)?;
        if account.expires_at > unix_now() {
            return Ok(account);
        }
    }
    Err("recovered authorization has no usable access token; retry shortly (queue retained)".into())
}

pub(super) fn revoke(account: &Account) -> Result<(), Refusal> {
    validate(account)?;
    let issuer = client(&account.issuer, &account.client_id)?;
    let response = issuer
        .http
        .post(format!("{}/auth/native/device/revoke", issuer.issuer))
        .send_form([
            ("client_id", account.client_id.as_str()),
            ("token", account.refresh_token.as_deref().unwrap()),
        ])
        .map_err(|_| {
            Refusal::Unavailable("native revocation service could not be reached".into())
        })?;
    if !response.status().is_success() {
        return Err(Refusal::Unavailable(
            "native revocation was not confirmed".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account() -> Account {
        serde_json::from_value(serde_json::json!({
            "auth_method":"native", "issuer":"https://api.example.test", "client_id":"cli",
            "ingest_url":"https://api.example.test", "subject":"member", "access_token":format!("rrnc_{}", "a".repeat(64)),
            "refresh_token":format!("rrnr_{}", "b".repeat(64)), "expires_at":unix_now()+300, "signed_in_at":unix_now(),
            "native":{"id":format!("ndg_{}", "c".repeat(64)), "project_id":"project", "expires_at":unix_now()+86400}
        })).unwrap()
    }
    fn response(a: &Account) -> Tokens {
        Tokens {
            access_token: format!("rrnc_{}", "d".repeat(64)),
            refresh_token: format!("rrnr_{}", "e".repeat(64)),
            token_type: "Bearer".into(),
            expires_at: unix_now() + 300,
            grant_expires_at: a.native.as_ref().unwrap().expires_at,
            scope: "capture".into(),
            subject: a.subject.clone(),
            project_id: "project".into(),
            grant_id: a.native.as_ref().unwrap().id.clone(),
        }
    }
    #[test]
    fn native_standing_respects_short_tokens_pending_intent_and_absolute_grant() {
        let mut a = account();
        assert_eq!(a.standing(unix_now()), Standing::Fresh);
        a.native.as_mut().unwrap().refresh_request = Some("f".repeat(64));
        assert_eq!(a.standing(unix_now()), Standing::Refresh);
        a.native.as_mut().unwrap().refresh_request = None;
        a.expires_at = unix_now() - 8 * 86400;
        assert!(matches!(a.standing(unix_now()), Standing::Offline { .. }));
        a.native.as_mut().unwrap().expires_at = unix_now() - 1;
        assert_eq!(a.standing(unix_now()), Standing::Expired);
    }
    #[test]
    fn refresh_cannot_rebind_identity_or_permissions_and_preserves_expired_receipts() {
        let a = account();
        for change in 0..6 {
            let mut t = response(&a);
            match change {
                0 => t.subject = "other".into(),
                1 => t.project_id = "other".into(),
                2 => t.grant_id = format!("ndg_{}", "f".repeat(64)),
                3 => t.scope = "capture admin".into(),
                4 => t.grant_expires_at += 1,
                _ => t.access_token = "server-controlled text".into(),
            }
            assert!(apply(&a, t, false).is_err());
        }
        let mut expired = response(&a);
        expired.expires_at = 1;
        let recovered = apply(&a, expired, false).unwrap();
        assert_eq!(recovered.expires_at, 1);
        assert_ne!(recovered.refresh_token, a.refresh_token);
        assert!(recovered.native.unwrap().refresh_request.is_none());
    }
}
