//! Native collector authorization. Credentials are never browser-session tokens.
use super::*;
use sqlx::postgres::PgRow;

pub(crate) const ACCESS_PREFIX: &str = "rrnc_";
const REFRESH_PREFIX: &str = "rrnr_";
const DEVICE_PREFIX: &str = "rrnd_";
pub(crate) const SCOPE: &str = "capture";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceClient {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceConfig {
    pub resource: String,
    pub clients: Vec<DeviceClient>,
    pub authorization_ttl_seconds: i64,
    pub access_ttl_seconds: i64,
    pub grant_ttl_seconds: i64,
    pub max_pending: i64,
    pub max_grants_per_user: i64,
    pub global_attempts: i32,
    pub user_attempts: i32,
}
impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            resource: String::new(),
            clients: vec![],
            authorization_ttl_seconds: 600,
            access_ttl_seconds: 900,
            grant_ttl_seconds: 2592000,
            max_pending: 5000,
            max_grants_per_user: 20,
            global_attempts: 600,
            user_attempts: 20,
        }
    }
}
impl DeviceConfig {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            resource_origin(&self.resource),
            "device resource needs an HTTPS origin (loopback HTTP allowed)"
        );
        ensure!(
            !self.clients.is_empty() && self.clients.len() <= 32,
            "register 1..32 public device clients"
        );
        let mut ids = std::collections::HashSet::new();
        for c in &self.clients {
            ensure!(
                !c.id.is_empty()
                    && c.id.len() <= 128
                    && c.id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
                    && ids.insert(&c.id),
                "invalid or duplicate device client"
            );
            ensure!(valid_name(&c.name), "invalid device client display name");
        }
        ensure!(
            (60..=900).contains(&self.authorization_ttl_seconds)
                && (300..=3600).contains(&self.access_ttl_seconds)
                && (3600..=31536000).contains(&self.grant_ttl_seconds)
                && (1..=100000).contains(&self.max_pending)
                && (1..=100).contains(&self.max_grants_per_user)
                && (1..=100000).contains(&self.global_attempts)
                && (1..=1000).contains(&self.user_attempts),
            "device limits are out of range"
        );
        Ok(())
    }
    pub(crate) fn client(&self, id: &str) -> Option<&DeviceClient> {
        self.clients.iter().find(|c| c.id == id)
    }
}
fn resource_origin(s: &str) -> bool {
    reqwest::Url::parse(s).is_ok_and(|u| {
        (u.scheme() == "https"
            || (u.scheme() == "http"
                && matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))))
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path() == "/"
            && s == u.origin().ascii_serialization()
    })
}
pub(crate) fn valid_name(s: &str) -> bool {
    (1..=80).contains(&s.chars().count())
        && !s.trim().is_empty()
        && !s.chars().any(char::is_control)
}
pub(crate) fn valid_request_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
const ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
fn code() -> String {
    let bytes = rand::random::<[u8; 10]>();
    bytes
        .iter()
        .map(|b| ALPHABET[(b & 31) as usize] as char)
        .collect()
}
pub(crate) fn normalize_code(s: &str) -> Option<String> {
    if s.len() > 11 {
        return None;
    }
    let s = s.replace('-', "").to_ascii_uppercase();
    (s.len() == 10 && s.bytes().all(|b| ALPHABET.contains(&b))).then_some(s)
}
pub(crate) enum Outcome<T> {
    Value(T),
    Error(&'static str),
}
fn fail<T>(code: &'static str) -> Result<Outcome<T>> {
    Ok(Outcome::Error(code))
}

#[derive(Serialize)]
pub(crate) struct Authorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i32,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expires_at: i64,
    pub scope: String,
    pub subject: String,
    pub project_id: String,
    pub grant_id: String,
    pub grant_expires_at: i64,
}
#[derive(Serialize)]
pub(crate) struct Inspection {
    pub account_email: String,
    pub client_name: String,
    pub device_name: String,
    pub expires_at: String,
    pub scope: &'static str,
}
#[derive(Serialize)]
pub(crate) struct Grant {
    pub id: String,
    pub device_name: String,
    pub client_name: String,
    pub project_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_used_at: String,
}

impl Accounts {
    pub(crate) fn device_cfg(&self) -> Option<&DeviceConfig> {
        self.cfg.device.as_ref()
    }
    fn device_context(&self) -> Result<String> {
        Ok(hash_key(&format!(
            "rroflow.device/v1\n{}\n{}",
            self.context,
            self.device_cfg()
                .ok_or_else(|| anyhow::anyhow!("device authorization disabled"))?
                .resource
        )))
    }
    pub(crate) async fn device_allow(
        &self,
        operation: &str,
        subject: &str,
        public: bool,
    ) -> Result<bool> {
        let Some(cfg) = self.device_cfg() else {
            return Ok(false);
        };
        self.allow_limits(
            operation,
            subject,
            cfg.global_attempts,
            if public {
                cfg.global_attempts
            } else {
                cfg.user_attempts
            },
        )
        .await
    }
    fn device_aad(
        &self,
        kind: &str,
        token: &str,
        request: &str,
        grant: &str,
        key: &str,
    ) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&(
            "rroflow.device-response/v1",
            self.device_context()?,
            kind,
            token,
            request,
            grant,
            key,
        ))?)
    }
    fn seal_device(
        &self,
        kind: &str,
        token: &str,
        request: &str,
        value: &Tokens,
    ) -> Result<(String, Vec<u8>, Vec<u8>)> {
        let key = self.cipher.active().to_string();
        let aad = self.device_aad(kind, token, request, &value.grant_id, &key)?;
        let plain = Zeroizing::new(serde_json::to_vec(value)?);
        let (nonce, ciphertext) = self.cipher.seal(&plain, &aad)?;
        Ok((key, nonce, ciphertext))
    }
    fn open_device(
        &self,
        row: &PgRow,
        kind: &str,
        token: &str,
        request: &str,
        grant: &str,
    ) -> Result<Option<Tokens>> {
        let Some(key) = row.try_get::<Option<String>, _>("response_key")? else {
            return Ok(None);
        };
        let aad = self.device_aad(kind, token, request, grant, &key)?;
        let nonce: Vec<u8> = row.try_get("response_nonce")?;
        let ciphertext: Vec<u8> = row.try_get("response_ciphertext")?;
        let plain = Zeroizing::new(
            self.cipher
                .open(&key, &nonce, &ciphertext, &aad)
                .ok_or_else(|| anyhow::anyhow!("device response authentication failed"))?,
        );
        Ok(Some(serde_json::from_slice(&plain)?))
    }
    pub(crate) async fn device_start(
        &self,
        client: &str,
        resource: &str,
        name: &str,
    ) -> Result<Outcome<Authorization>> {
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        if cfg.client(client).is_none() {
            return fail("invalid_client");
        }
        if resource != cfg.resource || !valid_name(name) {
            return fail("invalid_request");
        }
        // Cleanup commits before the capacity lock or any request row lock.
        sqlx::query("DELETE FROM native_device_requests WHERE expires_at<=clock_timestamp()")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM native_device_grants WHERE expires_at<=clock_timestamp() OR revoked_at IS NOT NULL").execute(&self.pool).await?;
        let context = self.device_context()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("device-capacity/{context}"))
            .execute(&mut *tx)
            .await?;
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM native_device_requests WHERE context_sha256=$1",
        )
        .bind(&context)
        .fetch_one(&mut *tx)
        .await?;
        if n >= cfg.max_pending {
            return fail("temporarily_unavailable");
        }
        let device = secret(DEVICE_PREFIX);
        let user_code = code();
        sqlx::query("INSERT INTO native_device_requests(device_sha256,user_code_sha256,context_sha256,client_id,device_name,expires_at,interval_seconds,next_poll_at) VALUES($1,$2,$3,$4,$5,clock_timestamp()+make_interval(secs=>$6),5,clock_timestamp()+interval '5 seconds')")
            .bind(hash_key(&device)).bind(hash_key(&user_code)).bind(&context).bind(client).bind(name).bind(cfg.authorization_ttl_seconds as f64).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Outcome::Value(Authorization {
            device_code: device,
            user_code: format!("{}-{}", &user_code[..5], &user_code[5..]),
            verification_uri: format!("{}/app/device", self.cfg.origin),
            expires_in: cfg.authorization_ttl_seconds,
            interval: 5,
        }))
    }
    // Approval requires a live native BROWSER session, never a bearer device token.
    async fn device_browser(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        session: &str,
    ) -> Result<Option<(String, i64)>> {
        if !valid_secret(session, SESSION_PREFIX) {
            return Ok(None);
        }
        let row:Option<(String,i64,DateTime<Utc>)>=sqlx::query_as("SELECT c.user_id,c.credential_epoch,s.expires_at FROM native_sessions s JOIN native_credentials c ON c.user_id=s.user_id AND c.credential_epoch=s.credential_epoch WHERE s.session_sha256=$1 AND s.context_sha256=$2 FOR SHARE OF c,s")
            .bind(hash_key(session)).bind(&self.context).fetch_optional(&mut **tx).await?;
        let now = clock(tx).await?;
        Ok(row
            .filter(|r| r.2 > now)
            .map(|(user, epoch, _)| (user, epoch)))
    }
    pub(crate) async fn device_inspect(
        &self,
        session: &str,
        user_code: &str,
    ) -> Result<Outcome<Inspection>> {
        let Some(code) = normalize_code(user_code) else {
            return fail("invalid_request");
        };
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        let mut tx = self.pool.begin().await?;
        let Some((user, _)) = self.device_browser(&mut tx, session).await? else {
            return fail("access_denied");
        };
        let account_email: String =
            sqlx::query_scalar("SELECT email FROM native_credentials WHERE user_id=$1")
                .bind(&user)
                .fetch_one(&mut *tx)
                .await?;
        let row=sqlx::query("SELECT client_id,device_name,expires_at FROM native_device_requests WHERE user_code_sha256=$1 AND context_sha256=$2 AND status='pending' AND expires_at>clock_timestamp()")
            .bind(hash_key(&code)).bind(self.device_context()?).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return fail("invalid_request");
        };
        let client: String = row.try_get("client_id")?;
        let Some(client) = cfg.client(&client) else {
            return fail("invalid_client");
        };
        Ok(Outcome::Value(Inspection {
            account_email,
            client_name: client.name.clone(),
            device_name: row.try_get("device_name")?,
            expires_at: row.try_get::<DateTime<Utc>, _>("expires_at")?.to_rfc3339(),
            scope: SCOPE,
        }))
    }
    pub(crate) async fn device_decide(
        &self,
        session: &str,
        user_code: &str,
        project: &str,
        approve: bool,
    ) -> Result<Outcome<()>> {
        let Some(code) = normalize_code(user_code) else {
            return fail("invalid_request");
        };
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT device_sha256,client_id,expires_at,status FROM native_device_requests WHERE user_code_sha256=$1 AND context_sha256=$2 FOR UPDATE")
            .bind(hash_key(&code)).bind(self.device_context()?).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return fail("invalid_request");
        };
        let Some((user, epoch)) = self.device_browser(&mut tx, session).await? else {
            return fail("access_denied");
        };
        let valid: bool = sqlx::query_scalar("SELECT $1::timestamptz>clock_timestamp()")
            .bind(row.try_get::<DateTime<Utc>, _>("expires_at")?)
            .fetch_one(&mut *tx)
            .await?;
        if !valid
            || row.try_get::<String, _>("status")? != "pending"
            || cfg
                .client(&row.try_get::<String, _>("client_id")?)
                .is_none()
        {
            return fail("invalid_request");
        }
        if approve && !writable_project(&mut tx, &user, project).await? {
            return fail("access_denied");
        }
        sqlx::query("UPDATE native_device_requests SET status=$2,user_id=$3,credential_epoch=$4,project_id=$5 WHERE device_sha256=$1")
            .bind(row.try_get::<String,_>("device_sha256")?).bind(if approve {"approved"} else {"denied"}).bind(&user).bind(epoch).bind(if approve {Some(project)} else {None}).execute(&mut *tx).await?;
        audit(
            &mut tx,
            &user,
            if approve {
                "device.approved"
            } else {
                "device.denied"
            },
        )
        .await?;
        tx.commit().await?;
        Ok(Outcome::Value(()))
    }
}

async fn writable_project(
    tx: &mut Transaction<'_, Postgres>,
    user: &str,
    project: &str,
) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devspaces d JOIN organization_members m ON m.organization_id=d.organization_id WHERE d.id=$1 AND m.user_id=$2 AND m.role IN ('owner','admin','member'))")
        .bind(project).bind(user).fetch_one(&mut **tx).await?)
}

async fn device_audit(
    tx: &mut Transaction<'_, Postgres>,
    user: &str,
    project: &str,
    action: &str,
    grant: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO audit_events(organization_id,actor_user_id,action,target) SELECT organization_id,$1,$3,$4 FROM devspaces WHERE id=$2")
        .bind(user).bind(project).bind(action).bind(grant).execute(&mut **tx).await?;
    Ok(())
}
async fn identity(
    tx: &mut Transaction<'_, Postgres>,
    user: &str,
    epoch: i64,
    project: &str,
) -> Result<Option<String>> {
    let subject:Option<String>=sqlx::query_scalar("SELECT i.subject FROM native_credentials c JOIN user_identities i ON i.user_id=c.user_id AND i.issuer=$3 WHERE c.user_id=$1 AND c.credential_epoch=$2 FOR SHARE OF c")
        .bind(user).bind(epoch).bind(ISSUER).fetch_optional(&mut **tx).await?;
    if subject.is_none() || !writable_project(tx, user, project).await? {
        return Ok(None);
    }
    Ok(subject)
}
async fn clock(tx: &mut Transaction<'_, Postgres>) -> Result<DateTime<Utc>> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}
fn tokens(
    subject: String,
    project: String,
    grant: String,
    expires: DateTime<Utc>,
    grant_expires: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Tokens {
    Tokens {
        access_token: secret(ACCESS_PREFIX),
        refresh_token: secret(REFRESH_PREFIX),
        token_type: "Bearer".into(),
        expires_in: (expires - now).num_seconds(),
        expires_at: expires.timestamp(),
        scope: SCOPE.into(),
        subject,
        project_id: project,
        grant_id: grant,
        grant_expires_at: grant_expires.timestamp(),
    }
}
impl Accounts {
    pub(crate) async fn device_exchange(
        &self,
        client: &str,
        device: &str,
        request: &str,
    ) -> Result<Outcome<Tokens>> {
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        if cfg.client(client).is_none() {
            return fail("invalid_client");
        }
        if !valid_secret(device, DEVICE_PREFIX) || !valid_request_id(request) {
            return fail("invalid_grant");
        }
        let digest = hash_key(device);
        let request_hash = hash_key(request);
        let context = self.device_context()?;
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT * FROM native_device_requests WHERE device_sha256=$1 AND context_sha256=$2 AND client_id=$3 FOR UPDATE")
            .bind(&digest).bind(&context).bind(client).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return fail("invalid_grant");
        };
        let now = clock(&mut tx).await?;
        if row.try_get::<DateTime<Utc>, _>("expires_at")? <= now {
            return fail("expired_token");
        }
        let status: String = row.try_get("status")?;
        if status == "denied" {
            return fail("access_denied");
        }
        if status == "exchanged" {
            if row.try_get::<String, _>("exchange_sha256")? != request_hash {
                return fail("invalid_grant");
            }
            let grant: String = row.try_get("grant_id")?;
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_device_grants g JOIN native_credentials c ON c.user_id=g.user_id AND c.credential_epoch=g.credential_epoch JOIN devspaces d ON d.id=g.project_id JOIN organization_members m ON m.organization_id=d.organization_id AND m.user_id=g.user_id WHERE m.role IN ('owner','admin','member') AND g.id=$1 AND g.revoked_at IS NULL AND g.expires_at>clock_timestamp())")
                .bind(&grant).fetch_one(&mut *tx).await?;
            if !valid {
                return fail("invalid_grant");
            }
            let Some(value) = self.open_device(&row, "exchange", &digest, &request_hash, &grant)?
            else {
                return fail("invalid_grant");
            };
            return Ok(Outcome::Value(value));
        }
        let interval: i32 = row.try_get("interval_seconds")?;
        if row.try_get::<DateTime<Utc>, _>("next_poll_at")? > now {
            sqlx::query("UPDATE native_device_requests SET interval_seconds=LEAST(interval_seconds+5,900),next_poll_at=clock_timestamp()+make_interval(secs=>LEAST(interval_seconds+5,900)) WHERE device_sha256=$1")
                .bind(&digest).execute(&mut *tx).await?;
            tx.commit().await?;
            return fail("slow_down");
        }
        sqlx::query("UPDATE native_device_requests SET next_poll_at=clock_timestamp()+make_interval(secs=>$2) WHERE device_sha256=$1")
            .bind(&digest).bind(interval as f64).execute(&mut *tx).await?;
        if status == "pending" {
            tx.commit().await?;
            return fail("authorization_pending");
        }
        let user: String = row.try_get("user_id")?;
        let epoch: i64 = row.try_get("credential_epoch")?;
        let project: String = row.try_get("project_id")?;
        let Some(subject) = identity(&mut tx, &user, epoch, &project).await? else {
            return fail("access_denied");
        };
        // Two independently approved devices cannot exceed this person's capacity.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("device-user-capacity/{context}/{user}"))
            .execute(&mut *tx)
            .await?;
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM native_device_grants WHERE user_id=$1 AND credential_epoch=$3 AND context_sha256=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp()")
            .bind(&user).bind(&context).bind(epoch).fetch_one(&mut *tx).await?;
        if n >= cfg.max_grants_per_user {
            return fail("access_denied");
        }
        let now = clock(&mut tx).await?;
        if row.try_get::<DateTime<Utc>, _>("expires_at")? <= now {
            return fail("expired_token");
        }
        let grant = secret("ndg_");
        let expiry = now + chrono::Duration::seconds(cfg.grant_ttl_seconds);
        let access_expiry = (now + chrono::Duration::seconds(cfg.access_ttl_seconds)).min(expiry);
        let value = tokens(
            subject,
            project.clone(),
            grant.clone(),
            access_expiry,
            expiry,
            now,
        );
        sqlx::query("INSERT INTO native_device_grants(id,context_sha256,client_id,device_name,user_id,credential_epoch,project_id,access_sha256,access_expires_at,refresh_sha256,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(&grant).bind(&context).bind(client).bind(row.try_get::<String,_>("device_name")?).bind(&user).bind(epoch).bind(&project).bind(hash_key(&value.access_token)).bind(access_expiry).bind(hash_key(&value.refresh_token)).bind(expiry).execute(&mut *tx).await?;
        let (key, nonce, ciphertext) =
            self.seal_device("exchange", &digest, &request_hash, &value)?;
        sqlx::query("UPDATE native_device_requests SET status='exchanged',grant_id=$2,exchange_sha256=$3,response_key=$4,response_nonce=$5,response_ciphertext=$6 WHERE device_sha256=$1")
            .bind(&digest).bind(&grant).bind(&request_hash).bind(key).bind(nonce).bind(ciphertext).execute(&mut *tx).await?;
        device_audit(&mut tx, &user, &project, "device.connected", &grant).await?;
        tx.commit().await?;
        Ok(Outcome::Value(value))
    }

    pub(crate) async fn device_refresh(
        &self,
        client: &str,
        refresh: &str,
        request: &str,
    ) -> Result<Outcome<Tokens>> {
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        if cfg.client(client).is_none() {
            return fail("invalid_client");
        }
        if !valid_secret(refresh, REFRESH_PREFIX) || !valid_request_id(request) {
            return fail("invalid_grant");
        }
        let digest = hash_key(refresh);
        let request_hash = hash_key(request);
        let context = self.device_context()?;
        let grant_id:Option<String>=sqlx::query_scalar("SELECT id FROM native_device_grants WHERE refresh_sha256=$1 AND context_sha256=$2 AND client_id=$3 UNION ALL SELECT g.id FROM native_device_rotations r JOIN native_device_grants g ON g.id=r.grant_id WHERE r.refresh_sha256=$1 AND g.context_sha256=$2 AND g.client_id=$3 LIMIT 1")
            .bind(&digest).bind(&context).bind(client).fetch_optional(&self.pool).await?;
        let Some(grant_id) = grant_id else {
            return fail("invalid_grant");
        };
        // Do not hold a connection/row lock while acquiring a rate-limit connection.
        if !self
            .device_allow("device-refresh", &grant_id, false)
            .await?
        {
            return fail("temporarily_unavailable");
        }
        let mut tx = self.pool.begin().await?;
        // Identify without locking history, then always take the grant lock first.
        let row=sqlx::query("SELECT * FROM native_device_grants WHERE id=$1 AND context_sha256=$2 AND client_id=$3 FOR UPDATE")
            .bind(&grant_id).bind(&context).bind(client).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return fail("invalid_grant");
        };
        let now = clock(&mut tx).await?;
        if row
            .try_get::<Option<DateTime<Utc>>, _>("revoked_at")?
            .is_some()
            || row.try_get::<DateTime<Utc>, _>("expires_at")? <= now
        {
            return fail("invalid_grant");
        }
        let grant: String = row.try_get("id")?;
        let user: String = row.try_get("user_id")?;
        let project: String = row.try_get("project_id")?;
        let Some(subject) =
            identity(&mut tx, &user, row.try_get("credential_epoch")?, &project).await?
        else {
            return fail("invalid_grant");
        };
        let now = clock(&mut tx).await?;
        if row.try_get::<DateTime<Utc>, _>("expires_at")? <= now {
            return fail("invalid_grant");
        }
        if row.try_get::<String, _>("refresh_sha256")? != digest {
            let previous = sqlx::query(
                "SELECT * FROM native_device_rotations WHERE refresh_sha256=$1 AND grant_id=$2",
            )
            .bind(&digest)
            .bind(&grant)
            .fetch_one(&mut *tx)
            .await?;
            if previous.try_get::<String, _>("request_sha256")? != request_hash {
                sqlx::query(
                    "UPDATE native_device_grants SET revoked_at=clock_timestamp() WHERE id=$1",
                )
                .bind(&grant)
                .execute(&mut *tx)
                .await?;
                device_audit(&mut tx, &user, &project, "device.refresh_reuse", &grant).await?;
                tx.commit().await?;
                return fail("invalid_grant");
            }
            let Some(value) =
                self.open_device(&previous, "refresh", &digest, &request_hash, &grant)?
            else {
                return fail("invalid_grant");
            };
            return Ok(Outcome::Value(value));
        }
        let access_expiry = (now + chrono::Duration::seconds(cfg.access_ttl_seconds))
            .min(row.try_get("expires_at")?);
        let value = tokens(
            subject,
            project.clone(),
            grant.clone(),
            access_expiry,
            row.try_get("expires_at")?,
            now,
        );
        let (key, nonce, ciphertext) =
            self.seal_device("refresh", &digest, &request_hash, &value)?;
        sqlx::query("UPDATE native_device_rotations SET response_key=NULL,response_nonce=NULL,response_ciphertext=NULL WHERE grant_id=$1 AND response_key IS NOT NULL")
            .bind(&grant).execute(&mut *tx).await?;
        // After refresh, an old device-code exchange must not return superseded credentials.
        sqlx::query("UPDATE native_device_requests SET response_key=NULL,response_nonce=NULL,response_ciphertext=NULL WHERE grant_id=$1 AND response_key IS NOT NULL")
            .bind(&grant).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO native_device_rotations(refresh_sha256,grant_id,request_sha256,response_key,response_nonce,response_ciphertext) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&digest).bind(&grant).bind(&request_hash).bind(key).bind(nonce).bind(ciphertext).execute(&mut *tx).await?;
        sqlx::query("UPDATE native_device_grants SET access_sha256=$2,access_expires_at=$3,refresh_sha256=$4,last_used_at=clock_timestamp() WHERE id=$1")
            .bind(&grant).bind(hash_key(&value.access_token)).bind(access_expiry).bind(hash_key(&value.refresh_token)).execute(&mut *tx).await?;
        device_audit(&mut tx, &user, &project, "device.refreshed", &grant).await?;
        tx.commit().await?;
        Ok(Outcome::Value(value))
    }

    pub(crate) async fn device_subject(&self, access: &str) -> Result<Option<(Subject, String)>> {
        let Some(cfg) = self.device_cfg() else {
            return Ok(None);
        };
        if !valid_secret(access, ACCESS_PREFIX) {
            return Ok(None);
        }
        let row:Option<(String,Option<String>,String,String)>=sqlx::query_as("SELECT i.subject,u.display_name,g.project_id,g.client_id FROM native_device_grants g JOIN native_credentials c ON c.user_id=g.user_id AND c.credential_epoch=g.credential_epoch JOIN users u ON u.id=g.user_id JOIN user_identities i ON i.user_id=g.user_id AND i.issuer=$3 JOIN devspaces d ON d.id=g.project_id JOIN organization_members m ON m.organization_id=d.organization_id AND m.user_id=g.user_id WHERE g.access_sha256=$1 AND g.context_sha256=$2 AND g.revoked_at IS NULL AND g.expires_at>clock_timestamp() AND g.access_expires_at>clock_timestamp() AND m.role IN ('owner','admin','member')")
            .bind(hash_key(access)).bind(self.device_context()?).bind(ISSUER).fetch_optional(&self.pool).await?;
        Ok(row
            .filter(|r| cfg.client(&r.3).is_some())
            .map(|(id, name, project, _)| {
                (
                    Subject {
                        id,
                        name,
                        org: None,
                        via: "native_device",
                    },
                    project,
                )
            }))
    }

    pub(crate) async fn device_grants(&self, session: &str) -> Result<Outcome<Vec<Grant>>> {
        let Some(cfg) = self.device_cfg() else {
            return fail("unauthorized_client");
        };
        let mut tx = self.pool.begin().await?;
        let Some((user, epoch)) = self.device_browser(&mut tx, session).await? else {
            return fail("access_denied");
        };
        let rows=sqlx::query("SELECT * FROM native_device_grants WHERE user_id=$1 AND credential_epoch=$2 AND context_sha256=$3 AND revoked_at IS NULL AND expires_at>clock_timestamp() ORDER BY created_at DESC LIMIT 100")
            .bind(user).bind(epoch).bind(self.device_context()?).fetch_all(&mut *tx).await?;
        let mut grants = Vec::new();
        for row in rows {
            let client: String = row.try_get("client_id")?;
            let Some(client) = cfg.client(&client) else {
                continue;
            };
            grants.push(Grant {
                id: row.try_get("id")?,
                device_name: row.try_get("device_name")?,
                client_name: client.name.clone(),
                project_id: row.try_get("project_id")?,
                created_at: row.try_get::<DateTime<Utc>, _>("created_at")?.to_rfc3339(),
                expires_at: row.try_get::<DateTime<Utc>, _>("expires_at")?.to_rfc3339(),
                last_used_at: row
                    .try_get::<DateTime<Utc>, _>("last_used_at")?
                    .to_rfc3339(),
            });
        }
        Ok(Outcome::Value(grants))
    }
    pub(crate) async fn device_disconnect(
        &self,
        session: &str,
        grant: &str,
    ) -> Result<Outcome<()>> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT user_id,project_id FROM native_device_grants WHERE id=$1 AND context_sha256=$2 FOR UPDATE")
            .bind(grant).bind(self.device_context()?).fetch_optional(&mut *tx).await?;
        let Some((user, _)) = self.device_browser(&mut tx, session).await? else {
            return fail("access_denied");
        };
        if let Some(row) = row.filter(|r| r.get::<String, _>("user_id") == user) {
            sqlx::query("UPDATE native_device_grants SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE id=$1").bind(grant).execute(&mut *tx).await?;
            device_audit(
                &mut tx,
                &user,
                &row.try_get::<String, _>("project_id")?,
                "device.disconnected",
                grant,
            )
            .await?;
        } else {
            return fail("invalid_request");
        }
        tx.commit().await?;
        Ok(Outcome::Value(()))
    }
    pub(crate) async fn device_revoke(&self, client: &str, token: &str) -> Result<()> {
        let Some(cfg) = self.device_cfg() else {
            return Ok(());
        };
        if cfg.client(client).is_none()
            || !(valid_secret(token, ACCESS_PREFIX) || valid_secret(token, REFRESH_PREFIX))
        {
            return Ok(());
        }
        let digest = hash_key(token);
        let context = self.device_context()?;
        let grant_id:Option<String>=sqlx::query_scalar("SELECT id FROM native_device_grants WHERE access_sha256=$1 AND context_sha256=$2 AND client_id=$3 UNION ALL SELECT id FROM native_device_grants WHERE refresh_sha256=$1 AND context_sha256=$2 AND client_id=$3 UNION ALL SELECT g.id FROM native_device_rotations r JOIN native_device_grants g ON g.id=r.grant_id WHERE r.refresh_sha256=$1 AND g.context_sha256=$2 AND g.client_id=$3 LIMIT 1")
            .bind(digest).bind(&context).bind(client).fetch_optional(&self.pool).await?;
        let Some(grant_id) = grant_id else {
            return Ok(());
        };
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT id,user_id,project_id FROM native_device_grants WHERE id=$1 AND context_sha256=$2 AND client_id=$3 FOR UPDATE")
            .bind(grant_id).bind(context).bind(client).fetch_optional(&mut *tx).await?;
        if let Some(row) = row {
            let grant: String = row.try_get("id")?;
            sqlx::query("UPDATE native_device_grants SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE id=$1").bind(&grant).execute(&mut *tx).await?;
            device_audit(
                &mut tx,
                &row.try_get::<String, _>("user_id")?,
                &row.try_get::<String, _>("project_id")?,
                "device.signed_out",
                &grant,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
