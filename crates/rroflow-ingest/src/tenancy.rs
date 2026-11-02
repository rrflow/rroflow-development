//! Accounts, organizations and projects. A project is a devspace under its product name, so
//! the monitoring tables keep partitioning on `devspace_id` and no record moves.
//!
//! Access is decided here and in [`AppState::caller`](crate::AppState::caller): every request
//! re-reads membership, so removing someone or revoking a key takes effect on their next
//! request. Someone outside an organization gets the same answer as for one that does not
//! exist. Keys and invitation tokens are stored only as digests and shown once.
use crate::nomen::Subject;
use crate::{ApiError, AppState, PgStore, Store, hash_key, internal};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Json as SqlJson};

/// Project keys start with this, so a mistyped one is never sent to the identity provider.
pub(crate) const KEY_PREFIX: &str = "rrop_";
const INVITATION_PREFIX: &str = "rroi_";
/// The project a member's request applies to. Checked against membership, never trusted.
pub(crate) const PROJECT_HEADER: &str = "x-rroflow-project";
const MAX_ACTIVE_KEYS: i64 = 50;
const MAX_PENDING_INVITATIONS: i64 = 100;
const INVITATION_DAYS: i32 = 7;
const AUDIT_PAGE: i64 = 100;
const AUDIT_PAGE_MAX: i64 = 500;
/// Would collide with console routes under `/app/{organization}`.
const RESERVED_SLUGS: &[&str] = &[
    "account",
    "admin",
    "api",
    "app",
    "auth",
    "invitations",
    "login",
    "logout",
    "new",
    "onboarding",
    "settings",
    "signup",
];

/// Bounds on what one person can create. These limit abuse and are not plan allocations:
/// allowances pool per organization, so `max_organizations_per_user` also bounds how many
/// default allowances one person can hold.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TenancyConfig {
    /// Team organizations one person may create; their personal one is not counted.
    pub max_organizations_per_user: i64,
    pub max_projects_per_organization: i64,
}

impl Default for TenancyConfig {
    fn default() -> Self {
        Self {
            max_organizations_per_user: 3,
            max_projects_per_organization: 10,
        }
    }
}

/// What a member may do, weakest first; each role includes the ones before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Reads the organization's projects.
    Viewer,
    /// Also sends records and changes signal settings and reviews.
    Member,
    /// Also manages projects, keys, invitations, members below owner and dataset consent.
    Admin,
    /// Also grants and removes owners.
    Owner,
}

impl Role {
    fn parse(value: &str) -> anyhow::Result<Self> {
        Ok(match value {
            "viewer" => Role::Viewer,
            "member" => Role::Member,
            "admin" => Role::Admin,
            "owner" => Role::Owner,
            other => anyhow::bail!("unknown role {other:?}"),
        })
    }
    fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }
}

/// A signed-in person's standing in the project a request applies to.
#[derive(Clone, Debug)]
pub struct Membership {
    pub user: String,
    pub organization: String,
    pub role: Role,
    pub devspace: String,
}

/// A project key's project, as its digest proved it.
#[derive(Clone, Debug)]
pub(crate) struct ProjectKey {
    pub(crate) id: String,
    pub(crate) devspace: String,
    pub(crate) organization: String,
}

fn identifier(prefix: &str) -> String {
    format!("{prefix}_{}", hex::encode(rand::random::<[u8; 16]>()))
}

fn secret(prefix: &str) -> String {
    format!("{prefix}{}", hex::encode(rand::random::<[u8; 32]>()))
}

fn db(error: sqlx::Error) -> ApiError {
    internal(error.into())
}

fn display_name(subject: &Subject) -> Option<String> {
    subject
        .name
        .as_deref()
        .map(|name| name.chars().take(200).collect())
}

/// Plain text a person chose; the console renders it as text, never as markup.
fn valid_label(value: &str, what: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(format!(
            "{what} must be 1-100 characters of plain text"
        )));
    }
    Ok(value.to_string())
}

/// URL form of a name: lowercase letters, digits and single hyphens.
fn slug_base(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(40);
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        fallback.to_string()
    } else {
        out.to_string()
    }
}

/// The name itself first; a short random suffix when it is taken or reserved.
fn slug_candidate(base: &str, attempt: usize) -> String {
    if attempt == 0 && !RESERVED_SLUGS.contains(&base) {
        base.to_string()
    } else {
        format!("{base}-{}", hex::encode(rand::random::<[u8; 3]>()))
    }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    organization: &str,
    actor: Option<&str>,
    action: &str,
    target: Option<&str>,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit_events (organization_id,actor_user_id,action,target,detail) VALUES ($1,$2,$3,$4,$5)")
        .bind(organization).bind(actor).bind(action).bind(target).bind(SqlJson(detail))
        .execute(&mut **tx).await?;
    Ok(())
}

async fn ensure_user(
    tx: &mut Transaction<'_, Postgres>,
    issuer: &str,
    subject: &str,
    name: Option<&str>,
) -> Result<String, sqlx::Error> {
    let found: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM user_identities WHERE issuer=$1 AND subject=$2")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(user) = found {
        return Ok(user);
    }
    let user = identifier("u");
    sqlx::query("INSERT INTO users (id,display_name) VALUES ($1,$2)")
        .bind(&user)
        .bind(name)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO user_identities (issuer,subject,user_id) VALUES ($1,$2,$3)")
        .bind(issuer)
        .bind(subject)
        .bind(&user)
        .execute(&mut **tx)
        .await?;
    Ok(user)
}

async fn create_personal_organization(
    tx: &mut Transaction<'_, Postgres>,
    user: &str,
) -> Result<String, sqlx::Error> {
    let organization = identifier("o");
    sqlx::query("INSERT INTO organizations (id,slug,name,personal,created_by) VALUES ($1,$2,'Personal',true,$3)")
        .bind(&organization).bind(format!("personal-{}", &organization[2..14])).bind(user)
        .execute(&mut **tx).await?;
    sqlx::query(
        "INSERT INTO organization_members (organization_id,user_id,role) VALUES ($1,$2,'owner')",
    )
    .bind(&organization)
    .bind(user)
    .execute(&mut **tx)
    .await?;
    audit(
        tx,
        &organization,
        Some(user),
        "organization.created",
        Some(&organization),
        json!({"personal":true}),
    )
    .await?;
    Ok(organization)
}

const OWNED_PROJECT: &str = "SELECT d.id,d.organization_id,m.role,m.user_id,d.owner_org,u.display_name FROM devspaces d \
    JOIN user_identities i ON i.issuer=d.owner_issuer AND i.subject=d.owner_subject \
    JOIN organization_members m ON m.organization_id=d.organization_id AND m.user_id=i.user_id \
    JOIN users u ON u.id=i.user_id WHERE d.owner_issuer=$1 AND d.owner_subject=$2";
type OwnedRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

/// Shared first-account transaction for federation and verified native registration.
pub(crate) async fn create_account_project(
    tx: &mut Transaction<'_, Postgres>,
    issuer: &str,
    subject: &Subject,
) -> anyhow::Result<Membership> {
    let name = display_name(subject);
    let user = ensure_user(tx, issuer, &subject.id, name.as_deref()).await?;
    let organization = create_personal_organization(tx, &user).await?;
    let devspace = crate::pg::owner_devspace_id(issuer, &subject.id);
    let created: Option<String> = sqlx::query_scalar("INSERT INTO devspaces (id,owner_issuer,owner_subject,owner_org,organization_id,slug,name) VALUES ($1,$2,$3,$4,$5,'default','Default project') ON CONFLICT DO NOTHING RETURNING id")
        .bind(&devspace).bind(issuer).bind(&subject.id).bind(&subject.org).bind(&organization)
        .fetch_optional(&mut **tx).await?;
    anyhow::ensure!(
        created.is_some(),
        "devspace id already exists without this owner; explicit operator claim required"
    );
    Ok(Membership {
        user,
        organization,
        role: Role::Owner,
        devspace,
    })
}

impl PgStore {
    /// The default project of a signed-in account, created with its user and personal
    /// organization on first sign-in. Its id stays the account ID, as it always has been.
    pub async fn account_project(
        &self,
        issuer: &str,
        subject: &Subject,
    ) -> anyhow::Result<Membership> {
        let name = display_name(subject);
        let found: Option<OwnedRow> = sqlx::query_as(OWNED_PROJECT)
            .bind(issuer)
            .bind(&subject.id)
            .fetch_optional(&self.pool)
            .await?;
        if let Some((devspace, organization, role, user, owner_org, shown)) = found {
            // Written only on a change: this runs on every signed-in request.
            if owner_org != subject.org {
                sqlx::query("UPDATE devspaces SET owner_org=$2 WHERE id=$1")
                    .bind(&devspace)
                    .bind(&subject.org)
                    .execute(&self.pool)
                    .await?;
            }
            if name.is_some() && shown != name {
                sqlx::query("UPDATE users SET display_name=$2 WHERE id=$1")
                    .bind(&user)
                    .bind(&name)
                    .execute(&self.pool)
                    .await?;
            }
            return Ok(Membership {
                user,
                organization,
                role: Role::parse(&role)?,
                devspace,
            });
        }
        let mut tx = self.pool.begin().await?;
        // Concurrent first sign-ins of one account wait here instead of racing to create it.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("account\n{issuer}\n{}", subject.id))
            .execute(&mut *tx)
            .await?;
        let raced: Option<OwnedRow> = sqlx::query_as(OWNED_PROJECT)
            .bind(issuer)
            .bind(&subject.id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some((devspace, organization, role, user, ..)) = raced {
            return Ok(Membership {
                user,
                organization,
                role: Role::parse(&role)?,
                devspace,
            });
        }
        let membership = create_account_project(&mut tx, issuer, subject).await?;
        tx.commit().await?;
        Ok(membership)
    }

    /// `None` both for a project that does not exist and for one this account cannot see.
    pub async fn member_project(
        &self,
        issuer: &str,
        subject: &str,
        project: &str,
    ) -> anyhow::Result<Option<Membership>> {
        let row: Option<(String, String, String)> = sqlx::query_as("SELECT m.user_id,m.organization_id,m.role FROM user_identities i JOIN organization_members m ON m.user_id=i.user_id JOIN devspaces d ON d.organization_id=m.organization_id WHERE i.issuer=$1 AND i.subject=$2 AND d.id=$3")
            .bind(issuer).bind(subject).bind(project).fetch_optional(&self.pool).await?;
        row.map(|(user, organization, role)| {
            Ok(Membership {
                user,
                organization,
                role: Role::parse(&role)?,
                devspace: project.to_string(),
            })
        })
        .transpose()
    }

    /// An unrevoked key's project. Revocation takes effect on the next request: there is no
    /// cache. Recording last use writes at most once a minute per key.
    pub(crate) async fn project_key(&self, hash: &str) -> anyhow::Result<Option<ProjectKey>> {
        let row: Option<(String, String, String)> = sqlx::query_as("WITH k AS (SELECT id,devspace_id FROM project_keys WHERE key_sha256=$1 AND revoked_at IS NULL), touched AS (UPDATE project_keys p SET last_used_at=now() FROM k WHERE p.id=k.id AND (p.last_used_at IS NULL OR p.last_used_at < now() - interval '1 minute')) SELECT k.id,k.devspace_id,d.organization_id FROM k JOIN devspaces d ON d.id=k.devspace_id")
            .bind(hash).fetch_optional(&self.pool).await?;
        Ok(row.map(|(id, devspace, organization)| ProjectKey {
            id,
            devspace,
            organization,
        }))
    }
}

/// Attach a claimed devspace to its new owner's personal organization. The caller holds the
/// devspace's row lock and has removed any empty default project this one replaces.
pub(crate) async fn attach_claimed(
    tx: &mut Transaction<'_, Postgres>,
    devspace: &str,
    issuer: &str,
    subject: &str,
    organization: Option<String>,
    replaced: Option<&str>,
) -> anyhow::Result<()> {
    let user = ensure_user(tx, issuer, subject, None).await?;
    let organization = match organization {
        Some(organization) => organization,
        None => create_personal_organization(tx, &user).await?,
    };
    sqlx::query("UPDATE devspaces SET owner_issuer=$2,owner_subject=$3,organization_id=$4,slug='default',name='Default project' WHERE id=$1")
        .bind(devspace).bind(issuer).bind(subject).bind(&organization)
        .execute(&mut **tx).await?;
    audit(
        tx,
        &organization,
        None,
        "project.claimed",
        Some(devspace),
        json!({"user":user,"replaced":replaced}),
    )
    .await?;
    Ok(())
}

/// The signed-in person behind a management request. Project keys and operator keys are not
/// people, and neither exists without Postgres.
async fn signed_in<'a>(
    st: &'a AppState,
    headers: &HeaderMap,
) -> Result<(&'a PgStore, String), ApiError> {
    let user = st
        .authenticate(headers)
        .await?
        .user
        .ok_or(ApiError::MemberRequired)?;
    let Store::Postgres(pg) = st.store.as_ref() else {
        return Err(ApiError::MemberRequired);
    };
    Ok((pg, user))
}

/// Serialize changes to an organization for this transaction and read the actor's standing,
/// so limits and the last-owner rule cannot be raced past.
async fn locked_organization(
    tx: &mut Transaction<'_, Postgres>,
    organization: &str,
    user: &str,
) -> Result<(Role, bool), ApiError> {
    let row: Option<(String, bool)> = sqlx::query_as("SELECT m.role,o.personal FROM organizations o JOIN organization_members m ON m.organization_id=o.id AND m.user_id=$2 WHERE o.id=$1 FOR NO KEY UPDATE OF o")
        .bind(organization).bind(user).fetch_optional(&mut **tx).await.map_err(db)?;
    let (_, personal) = row.ok_or(ApiError::Missing)?;
    // The join's snapshot may precede a lock wait. Re-read membership after
    // acquiring the organization lock; a concurrent demotion/removal is now visible.
    let role = role_in(&mut **tx, organization, user).await?;
    Ok((role, personal))
}

/// Same order as delivery, which locks the project and then the organization. A new key's
/// foreign key needs this share lock anyway, and waiting for it while holding the organization
/// would deadlock with a delivery waiting for the organization.
async fn locked_project(
    tx: &mut Transaction<'_, Postgres>,
    project: &str,
    user: &str,
) -> Result<(String, Role), ApiError> {
    sqlx::query("SELECT id FROM devspaces WHERE id=$1 FOR KEY SHARE")
        .bind(project)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;
    let row: Option<(String, String)> = sqlx::query_as("SELECT d.organization_id,m.role FROM devspaces d JOIN organizations o ON o.id=d.organization_id JOIN organization_members m ON m.organization_id=o.id AND m.user_id=$2 WHERE d.id=$1 FOR NO KEY UPDATE OF o")
        .bind(project).bind(user).fetch_optional(&mut **tx).await.map_err(db)?;
    let (organization, _) = row.ok_or(ApiError::Missing)?;
    let role = role_in(&mut **tx, &organization, user).await?;
    Ok((organization, role))
}

/// `user`'s role in `organization`; a non-member gets the same answer as no organization.
async fn role_in<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    organization: &str,
    user: &str,
) -> Result<Role, ApiError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM organization_members WHERE organization_id=$1 AND user_id=$2",
    )
    .bind(organization)
    .bind(user)
    .fetch_optional(executor)
    .await
    .map_err(db)?;
    Role::parse(&role.ok_or(ApiError::Missing)?).map_err(internal)
}

fn require_admin(role: Role) -> Result<(), ApiError> {
    if role >= Role::Admin {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "An organization admin or owner must do this.",
        ))
    }
}

/// Nobody manages someone above them or grants a role above their own.
fn may_change(actor: Role, current: Role, new: Option<Role>) -> bool {
    actor >= Role::Admin && current <= actor && new.is_none_or(|role| role <= actor)
}

const PERSONAL_ONE_MEMBER: ApiError = ApiError::Refused(
    "A personal organization has one member. Create a team organization to work with others.",
);

/// The account, its organizations and their projects: what a console needs to start.
pub(crate) async fn me(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let name: Option<String> = sqlx::query_scalar("SELECT display_name FROM users WHERE id=$1")
        .bind(&user)
        .fetch_one(&pg.pool)
        .await
        .map_err(db)?;
    let default_project: Option<String> = sqlx::query_scalar("SELECT d.id FROM devspaces d JOIN user_identities i ON i.issuer=d.owner_issuer AND i.subject=d.owner_subject WHERE i.user_id=$1 ORDER BY d.created_at,d.id LIMIT 1")
        .bind(&user).fetch_optional(&pg.pool).await.map_err(db)?;
    let organizations: Vec<SqlJson<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'slug',o.slug,'name',o.name,'personal',o.personal,'role',m.role,'projects',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',d.id,'slug',d.slug,'name',d.name) ORDER BY d.created_at,d.id) FROM devspaces d WHERE d.organization_id=o.id),'[]'::jsonb)) FROM organization_members m JOIN organizations o ON o.id=m.organization_id WHERE m.user_id=$1 ORDER BY o.personal DESC,o.created_at,o.id")
        .bind(&user).fetch_all(&pg.pool).await.map_err(db)?;
    Ok(Json(json!({
        "user": {"id": user, "name": name},
        "default_project": default_project,
        "organizations": organizations.into_iter().map(|o| o.0).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NameBody {
    name: String,
}

pub(crate) async fn create_organization(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<NameBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let name = valid_label(&body.name, "An organization name")?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("organizations\n{user}"))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let created: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organizations WHERE created_by=$1 AND NOT personal",
    )
    .bind(&user)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    if created >= st.tenancy.max_organizations_per_user {
        return Err(ApiError::Refused(
            "You have reached the limit on organizations you can create.",
        ));
    }
    let organization = identifier("o");
    let base = slug_base(&name, "organization");
    let mut slug = None;
    for attempt in 0..5 {
        slug = sqlx::query_scalar("INSERT INTO organizations (id,slug,name,created_by) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING slug")
            .bind(&organization).bind(slug_candidate(&base, attempt)).bind(&name).bind(&user)
            .fetch_optional(&mut *tx).await.map_err(db)?;
        if slug.is_some() {
            break;
        }
    }
    let slug: String = slug.ok_or(ApiError::Internal)?;
    sqlx::query(
        "INSERT INTO organization_members (organization_id,user_id,role) VALUES ($1,$2,'owner')",
    )
    .bind(&organization)
    .bind(&user)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "organization.created",
        Some(&organization),
        json!({}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"id":organization,"slug":slug,"name":name,"personal":false,"role":"owner","projects":[]}),
    ))
}

pub(crate) async fn create_project(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(organization): Path<String>,
    Json(body): Json<NameBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let name = valid_label(&body.name, "A project name")?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (role, _) = locked_organization(&mut tx, &organization, &user).await?;
    require_admin(role)?;
    let existing: i64 =
        sqlx::query_scalar("SELECT count(*) FROM devspaces WHERE organization_id=$1")
            .bind(&organization)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if existing >= st.tenancy.max_projects_per_organization {
        return Err(ApiError::Refused(
            "This organization has reached its project limit.",
        ));
    }
    let project = identifier("p");
    let base = slug_base(&name, "project");
    let mut slug = None;
    for attempt in 0..5 {
        slug = sqlx::query_scalar("INSERT INTO devspaces (id,organization_id,slug,name) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING slug")
            .bind(&project).bind(&organization).bind(slug_candidate(&base, attempt)).bind(&name)
            .fetch_optional(&mut *tx).await.map_err(db)?;
        if slug.is_some() {
            break;
        }
    }
    let slug: String = slug.ok_or(ApiError::Internal)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "project.created",
        Some(&project),
        json!({"name":name}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"id":project,"organization":organization,"slug":slug,"name":name}),
    ))
}

pub(crate) async fn members(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(organization): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    role_in(&pg.pool, &organization, &user).await?;
    let members: Vec<SqlJson<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('user',u.id,'name',u.display_name,'role',m.role,'joined_at',m.created_at) FROM organization_members m JOIN users u ON u.id=m.user_id WHERE m.organization_id=$1 ORDER BY m.created_at,u.id")
        .bind(&organization).fetch_all(&pg.pool).await.map_err(db)?;
    Ok(Json(
        json!({"members": members.into_iter().map(|m| m.0).collect::<Vec<_>>()}),
    ))
}

/// The caller holds the organization lock, so two owners cannot both step down at once.
async fn refuse_last_owner(
    tx: &mut Transaction<'_, Postgres>,
    organization: &str,
) -> Result<(), ApiError> {
    let owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM organization_members WHERE organization_id=$1 AND role='owner'",
    )
    .bind(organization)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    if owners <= 1 {
        return Err(ApiError::Refused(
            "An organization needs an owner. Make someone else an owner first.",
        ));
    }
    Ok(())
}

/// Invitations carry their creator's authority and end with it. Otherwise someone could keep a
/// token they minted, lose their role, and use it to come back. The caller holds the
/// organization lock, so no new one can be minted with the old role while this runs.
async fn revoke_invitations_from(
    tx: &mut Transaction<'_, Postgres>,
    organization: &str,
    user: &str,
) -> Result<u64, ApiError> {
    Ok(sqlx::query("UPDATE organization_invitations SET revoked_at=now() WHERE organization_id=$1 AND created_by=$2 AND accepted_at IS NULL AND revoked_at IS NULL")
        .bind(organization).bind(user).execute(&mut **tx).await.map_err(db)?.rows_affected())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RoleBody {
    role: Role,
}

pub(crate) async fn set_role(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((organization, target)): Path<(String, String)>,
    Json(body): Json<RoleBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (actor, personal) = locked_organization(&mut tx, &organization, &user).await?;
    if personal {
        return Err(PERSONAL_ONE_MEMBER);
    }
    let current = role_in(&mut *tx, &organization, &target).await?;
    if !may_change(actor, current, Some(body.role)) {
        return Err(ApiError::Forbidden(
            "You cannot change this member to that role.",
        ));
    }
    if current == Role::Owner && body.role != Role::Owner {
        refuse_last_owner(&mut tx, &organization).await?;
    }
    sqlx::query("UPDATE organization_members SET role=$3 WHERE organization_id=$1 AND user_id=$2")
        .bind(&organization)
        .bind(&target)
        .bind(body.role.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let invitations_revoked = if body.role < Role::Admin {
        revoke_invitations_from(&mut tx, &organization, &target).await?
    } else {
        0
    };
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "member.role_changed",
        Some(&target),
        json!({"from":current,"to":body.role,"invitations_revoked":invitations_revoked}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(json!({"user":target,"role":body.role})))
}

/// Remove someone, or leave yourself: anyone may leave.
pub(crate) async fn remove_member(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((organization, target)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (actor, personal) = locked_organization(&mut tx, &organization, &user).await?;
    if personal {
        return Err(PERSONAL_ONE_MEMBER);
    }
    let current = role_in(&mut *tx, &organization, &target).await?;
    if target != user && !may_change(actor, current, None) {
        return Err(ApiError::Forbidden("You cannot remove this member."));
    }
    if current == Role::Owner {
        refuse_last_owner(&mut tx, &organization).await?;
    }
    sqlx::query("DELETE FROM organization_members WHERE organization_id=$1 AND user_id=$2")
        .bind(&organization)
        .bind(&target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let invitations_revoked = revoke_invitations_from(&mut tx, &organization, &target).await?;
    // Keys belong to the project, so someone leaving does not stop a running delivery. Report
    // what they created, so whoever removed them can decide what to rotate.
    let active_keys_created: i64 = sqlx::query_scalar("SELECT count(*) FROM project_keys k JOIN devspaces d ON d.id=k.devspace_id WHERE d.organization_id=$1 AND k.created_by=$2 AND k.revoked_at IS NULL")
        .bind(&organization).bind(&target).fetch_one(&mut *tx).await.map_err(db)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "member.removed",
        Some(&target),
        json!({"role":current,"invitations_revoked":invitations_revoked,"active_keys_created":active_keys_created}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"removed":true,"active_keys_created":active_keys_created}),
    ))
}

pub(crate) async fn invite(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(organization): Path<String>,
    Json(body): Json<RoleBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (actor, personal) = locked_organization(&mut tx, &organization, &user).await?;
    require_admin(actor)?;
    if personal {
        return Err(PERSONAL_ONE_MEMBER);
    }
    if body.role == Role::Owner || body.role > actor {
        return Err(ApiError::Forbidden(
            "An invitation cannot grant that role. An owner can promote a member after they join.",
        ));
    }
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM organization_invitations WHERE organization_id=$1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at>now()")
        .bind(&organization).fetch_one(&mut *tx).await.map_err(db)?;
    if pending >= MAX_PENDING_INVITATIONS {
        return Err(ApiError::Refused(
            "Too many invitations are waiting. Revoke some first.",
        ));
    }
    let (id, token) = (identifier("i"), secret(INVITATION_PREFIX));
    let expires_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("INSERT INTO organization_invitations (id,organization_id,role,token_sha256,created_by,expires_at) VALUES ($1,$2,$3,$4,$5,now()+make_interval(days=>$6)) RETURNING expires_at")
        .bind(&id).bind(&organization).bind(body.role.as_str()).bind(hash_key(&token)).bind(&user).bind(INVITATION_DAYS)
        .fetch_one(&mut *tx).await.map_err(db)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "invitation.created",
        Some(&id),
        json!({"role":body.role}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"id":id,"token":token,"role":body.role,"expires_at":expires_at.to_rfc3339()}),
    ))
}

pub(crate) async fn invitations(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(organization): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    require_admin(role_in(&pg.pool, &organization, &user).await?)?;
    let pending: Vec<SqlJson<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'role',role,'created_by',created_by,'created_at',created_at,'expires_at',expires_at) FROM organization_invitations WHERE organization_id=$1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at>now() ORDER BY created_at DESC,id")
        .bind(&organization).fetch_all(&pg.pool).await.map_err(db)?;
    Ok(Json(
        json!({"invitations": pending.into_iter().map(|i| i.0).collect::<Vec<_>>()}),
    ))
}

pub(crate) async fn revoke_invitation(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((organization, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (actor, _) = locked_organization(&mut tx, &organization, &user).await?;
    require_admin(actor)?;
    let revoked = sqlx::query("UPDATE organization_invitations SET revoked_at=now() WHERE id=$1 AND organization_id=$2 AND accepted_at IS NULL AND revoked_at IS NULL")
        .bind(&id).bind(&organization).execute(&mut *tx).await.map_err(db)?;
    if revoked.rows_affected() == 0 {
        return Err(ApiError::Missing);
    }
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "invitation.revoked",
        Some(&id),
        json!({}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(json!({"revoked":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcceptBody {
    token: String,
}

/// Unknown, expired, revoked and already-used invitations all look the same.
pub(crate) async fn accept_invitation(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AcceptBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let invitation: Option<(String, String, String)> = sqlx::query_as("SELECT id,organization_id,role FROM organization_invitations WHERE token_sha256=$1 AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at>now() FOR UPDATE")
        .bind(hash_key(&body.token)).fetch_optional(&mut *tx).await.map_err(db)?;
    let (id, organization, role) = invitation.ok_or(ApiError::Missing)?;
    let joined = sqlx::query("INSERT INTO organization_members (organization_id,user_id,role) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(&organization).bind(&user).bind(&role).execute(&mut *tx).await.map_err(db)?;
    if joined.rows_affected() == 0 {
        // Left unused, so it can still go to the person it was meant for.
        return Err(ApiError::Refused(
            "You are already a member of this organization.",
        ));
    }
    sqlx::query("UPDATE organization_invitations SET accepted_by=$2,accepted_at=now() WHERE id=$1")
        .bind(&id)
        .bind(&user)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "member.joined",
        Some(&user),
        json!({"role":role,"invitation":id}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(json!({"organization":organization,"role":role})))
}

pub(crate) async fn keys(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let role: Option<String> = sqlx::query_scalar("SELECT m.role FROM devspaces d JOIN organization_members m ON m.organization_id=d.organization_id AND m.user_id=$2 WHERE d.id=$1")
        .bind(&project).bind(&user).fetch_optional(&pg.pool).await.map_err(db)?;
    require_admin(Role::parse(&role.ok_or(ApiError::Missing)?).map_err(internal)?)?;
    let keys: Vec<SqlJson<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'label',label,'prefix',prefix,'created_by',created_by,'created_at',created_at,'last_used_at',last_used_at,'revoked_at',revoked_at) FROM project_keys WHERE devspace_id=$1 ORDER BY created_at DESC,id")
        .bind(&project).fetch_all(&pg.pool).await.map_err(db)?;
    Ok(Json(
        json!({"keys": keys.into_iter().map(|k| k.0).collect::<Vec<_>>()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyBody {
    label: String,
}

pub(crate) async fn create_key(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(body): Json<KeyBody>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let label = valid_label(&body.label, "A key label")?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (organization, role) = locked_project(&mut tx, &project, &user).await?;
    require_admin(role)?;
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_keys WHERE devspace_id=$1 AND revoked_at IS NULL",
    )
    .bind(&project)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    if active >= MAX_ACTIVE_KEYS {
        return Err(ApiError::Refused(
            "This project has too many active keys. Revoke some first.",
        ));
    }
    let (id, key) = (identifier("k"), secret(KEY_PREFIX));
    let prefix = &key[..KEY_PREFIX.len() + 8];
    sqlx::query("INSERT INTO project_keys (id,devspace_id,label,key_sha256,prefix,created_by) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(&id).bind(&project).bind(&label).bind(hash_key(&key)).bind(prefix).bind(&user)
        .execute(&mut *tx).await.map_err(db)?;
    audit(
        &mut tx,
        &organization,
        Some(&user),
        "key.created",
        Some(&id),
        json!({"project":project,"label":label}),
    )
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Json(
        json!({"id":id,"key":key,"prefix":prefix,"label":label,"project":project}),
    ))
}

pub(crate) async fn revoke_key(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    let mut tx = pg.pool.begin().await.map_err(db)?;
    let (organization, role) = locked_project(&mut tx, &project, &user).await?;
    require_admin(role)?;
    let revoked = sqlx::query("UPDATE project_keys SET revoked_at=now(),revoked_by=$3 WHERE id=$1 AND devspace_id=$2 AND revoked_at IS NULL")
        .bind(&id).bind(&project).bind(&user).execute(&mut *tx).await.map_err(db)?;
    if revoked.rows_affected() > 0 {
        audit(
            &mut tx,
            &organization,
            Some(&user),
            "key.revoked",
            Some(&id),
            json!({"project":project}),
        )
        .await
        .map_err(db)?;
    } else {
        // Revoking twice succeeds; a key of another project stays invisible.
        let known: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_keys WHERE id=$1 AND devspace_id=$2)",
        )
        .bind(&id)
        .bind(&project)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !known {
            return Err(ApiError::Missing);
        }
    }
    tx.commit().await.map_err(db)?;
    Ok(Json(json!({"revoked":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuditQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

pub(crate) async fn audit_log(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(organization): Path<String>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, ApiError> {
    let (pg, user) = signed_in(&st, &headers).await?;
    require_admin(role_in(&pg.pool, &organization, &user).await?)?;
    let limit = q.limit.unwrap_or(AUDIT_PAGE).clamp(1, AUDIT_PAGE_MAX);
    let rows: Vec<(i64, SqlJson<Value>)> = sqlx::query_as("SELECT id,jsonb_build_object('id',id,'actor',actor_user_id,'action',action,'target',target,'detail',detail,'at',at) FROM audit_events WHERE organization_id=$1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
        .bind(&organization).bind(q.before).bind(limit).fetch_all(&pg.pool).await.map_err(db)?;
    let next_before = (rows.len() as i64 == limit)
        .then(|| rows.last().map(|(id, _)| *id))
        .flatten();
    Ok(Json(json!({
        "events": rows.into_iter().map(|(_, event)| event.0).collect::<Vec<_>>(),
        "next_before": next_before,
    })))
}

#[cfg(test)]
mod tests;
