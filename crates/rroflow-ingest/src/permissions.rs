//! Who may do what.
//!
//! Every route declares what it needs in [`ROUTES`], and [`enforce`] refuses any route that is not
//! declared, so a new route is closed until someone decides who may use it. Handlers decide each
//! request with one function, [`require`], against the grant the caller's credential carries.
//! Organization checks run inside the handler's transaction, so a role change that commits first
//! is always seen.
//!
//! A person's grant is their organization role, and each role is a fixed bundle of permissions
//! ([`Grant::permissions`]). Keys are not people: an operator key reads, sends and changes its one
//! project, and a project key only sends.
use crate::{ApiError, Role};
use axum::{
    Json,
    extract::{MatchedPath, Request},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// One thing a caller may do, named as the API and console show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Permission {
    /// Read a project's monitoring data.
    #[serde(rename = "project.read")]
    ProjectRead,
    /// Deliver captured records to a project.
    #[serde(rename = "project.send")]
    ProjectSend,
    /// Change a project's signal settings and reviews.
    #[serde(rename = "project.write")]
    ProjectWrite,
    /// List, create and revoke a project's keys.
    #[serde(rename = "keys.manage")]
    KeysManage,
    /// Change dataset consent for a project.
    #[serde(rename = "datasets.manage")]
    DatasetsManage,
    /// Download a published dataset.
    #[serde(rename = "datasets.export")]
    DatasetsExport,
    /// See an organization's members.
    #[serde(rename = "organization.read")]
    OrganizationRead,
    /// Create projects in an organization.
    #[serde(rename = "projects.create")]
    ProjectsCreate,
    /// Invite people, and see and revoke invitations.
    #[serde(rename = "members.invite")]
    MembersInvite,
    /// Change members' roles and remove members. Nobody manages someone above them.
    #[serde(rename = "members.manage")]
    MembersManage,
    /// Read an organization's audit log.
    #[serde(rename = "audit.read")]
    AuditRead,
    /// Change this instance's settings.
    #[serde(rename = "instance.settings")]
    InstanceSettings,
}

/// Where a permission applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// The project a request names, through its key, path or project header.
    Project,
    /// The organization in the request's path.
    Organization,
    /// The whole instance.
    Instance,
}

impl Permission {
    pub const ALL: [Permission; 12] = [
        Permission::ProjectRead,
        Permission::ProjectSend,
        Permission::ProjectWrite,
        Permission::KeysManage,
        Permission::DatasetsManage,
        Permission::DatasetsExport,
        Permission::OrganizationRead,
        Permission::ProjectsCreate,
        Permission::MembersInvite,
        Permission::MembersManage,
        Permission::AuditRead,
        Permission::InstanceSettings,
    ];

    pub fn scope(self) -> Scope {
        match self {
            Permission::ProjectRead
            | Permission::ProjectSend
            | Permission::ProjectWrite
            | Permission::KeysManage
            | Permission::DatasetsManage
            | Permission::DatasetsExport => Scope::Project,
            Permission::OrganizationRead
            | Permission::ProjectsCreate
            | Permission::MembersInvite
            | Permission::MembersManage
            | Permission::AuditRead => Scope::Organization,
            Permission::InstanceSettings => Scope::Instance,
        }
    }
}

/// What a credential carries into a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    /// A signed-in person, with their role in the organization that owns the project or that
    /// the request names.
    Role(Role),
    /// A key registered by the operator in the service's configuration.
    OperatorKey,
    /// A project key: it can deliver records and nothing else.
    SendOnlyKey,
    /// A person who administers this instance.
    InstanceAdmin,
    /// A person with an account here and no instance role.
    InstanceMember,
}

const VIEWER: &[Permission] = &[Permission::ProjectRead, Permission::OrganizationRead];
const MEMBER: &[Permission] = &[
    Permission::ProjectRead,
    Permission::ProjectSend,
    Permission::ProjectWrite,
    Permission::OrganizationRead,
];
/// Owners hold the same permissions as admins. What sets them apart is rank: only an owner
/// grants or removes an owner, and the last owner cannot leave.
const ADMIN: &[Permission] = &[
    Permission::ProjectRead,
    Permission::ProjectSend,
    Permission::ProjectWrite,
    Permission::KeysManage,
    Permission::DatasetsManage,
    Permission::DatasetsExport,
    Permission::OrganizationRead,
    Permission::ProjectsCreate,
    Permission::MembersInvite,
    Permission::MembersManage,
    Permission::AuditRead,
];
const INSTANCE_ADMIN: &[Permission] = &[Permission::InstanceSettings];
const OPERATOR_KEY: &[Permission] = &[
    Permission::ProjectRead,
    Permission::ProjectSend,
    Permission::ProjectWrite,
];
const SEND_ONLY_KEY: &[Permission] = &[Permission::ProjectSend];

impl Grant {
    pub fn permissions(self) -> &'static [Permission] {
        match self {
            Grant::Role(Role::Viewer) => VIEWER,
            Grant::Role(Role::Member) => MEMBER,
            Grant::Role(Role::Admin | Role::Owner) => ADMIN,
            Grant::OperatorKey => OPERATOR_KEY,
            Grant::SendOnlyKey => SEND_ONLY_KEY,
            Grant::InstanceAdmin => INSTANCE_ADMIN,
            Grant::InstanceMember => &[],
        }
    }

    pub fn allows(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}

/// The one access decision. Every refusal says what would have been needed.
pub fn require(grant: Grant, permission: Permission) -> Result<(), ApiError> {
    if grant.allows(permission) {
        return Ok(());
    }
    Err(ApiError::Forbidden(match (grant, permission) {
        (Grant::SendOnlyKey, _) => "This project key can send records but not read them.",
        (_, Permission::ProjectSend | Permission::ProjectWrite) => {
            "Your role can view this project but not change it."
        }
        (_, Permission::DatasetsManage | Permission::DatasetsExport) => {
            "An organization admin or owner manages dataset consent and exports."
        }
        (_, Permission::InstanceSettings) => "An instance admin must do this.",
        _ => "An organization admin or owner must do this.",
    }))
}

/// What a route needs before its handler decides the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Anyone: pages, assets, health and build identity.
    Public,
    /// A sign-in or device-approval step that checks the credential it is given.
    SignIn,
    /// Any valid credential, acting only on itself: who it is, its own account, organizations it
    /// creates and invitations it holds.
    Own,
    /// This permission, in the scope the request names.
    Requires(Permission),
    /// Any one of these; the handler says what each one shows.
    AnyOf(&'static [Permission]),
}

use Access::{AnyOf, Own, Public, Requires, SignIn};
use Permission::*;

/// Every route the service answers, by method and path pattern. `HEAD` follows `GET`.
pub const ROUTES: &[(&str, &str, Access)] = &[
    // Native accounts and device approval.
    ("GET", "/auth/native/config", Public),
    ("POST", "/auth/native/request", SignIn),
    ("POST", "/auth/native/login", SignIn),
    ("POST", "/auth/native/complete", SignIn),
    ("POST", "/auth/native/first-run", SignIn),
    ("POST", "/auth/native/password/status", SignIn),
    ("POST", "/auth/native/password/saved", SignIn),
    ("POST", "/auth/native/password/replace", SignIn),
    ("GET", "/auth/native/device/config", Public),
    ("POST", "/auth/native/device/start", SignIn),
    ("POST", "/auth/native/device/token", SignIn),
    ("POST", "/auth/native/device/revoke", SignIn),
    ("POST", "/auth/native/device/inspect", SignIn),
    ("POST", "/auth/native/device/decide", SignIn),
    ("POST", "/auth/native/device/grants", SignIn),
    ("POST", "/auth/native/device/disconnect", SignIn),
    // Browser sign-in.
    ("GET", "/auth/config", Public),
    ("GET", "/auth/login", SignIn),
    ("GET", "/auth/federation", SignIn),
    ("GET", "/auth/callback", SignIn),
    ("POST", "/auth/logout", SignIn),
    // Project monitoring data.
    ("POST", "/v1/events", Requires(ProjectSend)),
    ("GET", "/v1/events", Requires(ProjectRead)),
    ("POST", "/v1/reviews", Requires(ProjectWrite)),
    ("GET", "/v1/reviews", Requires(ProjectRead)),
    ("POST", "/v1/turns", Requires(ProjectSend)),
    ("GET", "/v1/turns", Requires(ProjectRead)),
    ("GET", "/v1/summary", Requires(ProjectRead)),
    ("GET", "/v1/whoami", Own),
    ("GET", "/v1/provenance", Requires(ProjectRead)),
    ("GET", "/v1/provenance/coverage", Requires(ProjectRead)),
    ("GET", "/v1/allowances", Requires(ProjectRead)),
    ("GET", "/v1/sdk/activity", Requires(ProjectRead)),
    ("GET", "/v1/sdk/summary", Requires(ProjectRead)),
    ("GET", "/v1/sdk/reviews", Requires(ProjectRead)),
    ("POST", "/v1/sdk/reviews", Requires(ProjectWrite)),
    ("GET", "/v1/datasets", Requires(ProjectRead)),
    ("PUT", "/v1/datasets/policy", Requires(DatasetsManage)),
    (
        "GET",
        "/v1/datasets/jobs/{id}/artifact",
        Requires(DatasetsExport),
    ),
    // The built-in signal catalogue is public.
    ("GET", "/v1/signals", Public),
    ("GET", "/v1/signals/stats", Requires(ProjectRead)),
    ("POST", "/v1/signals/preview", Requires(ProjectRead)),
    // A collector's send-only key reads the built-in settings it applies; readers see all.
    (
        "GET",
        "/v1/signals/settings",
        AnyOf(&[ProjectRead, ProjectSend]),
    ),
    ("PUT", "/v1/signals/settings", Requires(ProjectWrite)),
    ("GET", "/v1/sources/health", Requires(ProjectRead)),
    ("POST", "/v1/sources/health", Requires(ProjectSend)),
    ("GET", "/assets/research.json", Requires(ProjectRead)),
    // Accounts, organizations and projects.
    ("GET", "/v1/me", Own),
    ("GET", "/v1/instance/settings", Requires(InstanceSettings)),
    ("PUT", "/v1/instance/settings", Requires(InstanceSettings)),
    ("POST", "/v1/organizations", Own),
    ("POST", "/v1/invitations/accept", Own),
    (
        "POST",
        "/v1/organizations/{organization}/projects",
        Requires(ProjectsCreate),
    ),
    (
        "GET",
        "/v1/organizations/{organization}/members",
        Requires(OrganizationRead),
    ),
    (
        "PUT",
        "/v1/organizations/{organization}/members/{user}",
        Requires(MembersManage),
    ),
    // Leaving an organization yourself needs no permission; removing someone else does.
    (
        "DELETE",
        "/v1/organizations/{organization}/members/{user}",
        Requires(MembersManage),
    ),
    (
        "GET",
        "/v1/organizations/{organization}/invitations",
        Requires(MembersInvite),
    ),
    (
        "POST",
        "/v1/organizations/{organization}/invitations",
        Requires(MembersInvite),
    ),
    (
        "DELETE",
        "/v1/organizations/{organization}/invitations/{invitation}",
        Requires(MembersInvite),
    ),
    (
        "GET",
        "/v1/organizations/{organization}/audit",
        Requires(AuditRead),
    ),
    ("GET", "/v1/projects/{project}/keys", Requires(KeysManage)),
    ("POST", "/v1/projects/{project}/keys", Requires(KeysManage)),
    (
        "DELETE",
        "/v1/projects/{project}/keys/{key}",
        Requires(KeysManage),
    ),
    // Pages, assets, health and build identity.
    ("GET", "/healthz", Public),
    ("GET", "/readyz", Public),
    ("GET", "/build-info", Public),
    ("GET", "/", Public),
    ("GET", "/app", Public),
    ("GET", "/app/", Public),
    ("GET", "/app/manage", Public),
    ("GET", "/app/account", Public),
    ("GET", "/app/device", Public),
    ("GET", "/app/settings", Public),
    ("GET", "/assets/dashboard-datasets.js", Public),
    ("GET", "/assets/dashboard-sdk.js", Public),
    ("GET", "/assets/member-console/app.js", Public),
    ("GET", "/assets/member-console/index.css", Public),
    ("GET", "/assets/brand.css", Public),
    ("GET", "/assets/public.css", Public),
    ("GET", "/assets/public.js", Public),
    ("GET", "/assets/dashboard.css", Public),
    ("GET", "/assets/dashboard-a2ui.js", Public),
    ("GET", "/assets/dashboard.js", Public),
];

/// What the route table says about a request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Declared {
    Yes(Access),
    /// The path is declared, but not for this method.
    OtherMethod,
    /// Nothing declares this path.
    No,
}

pub(crate) fn declared(method: &Method, path: &str) -> Declared {
    let method = if method == Method::HEAD {
        "GET"
    } else {
        method.as_str()
    };
    let mut path_known = false;
    for (m, p, access) in ROUTES {
        if *p == path {
            if *m == method {
                return Declared::Yes(*access);
            }
            path_known = true;
        }
    }
    if path_known {
        Declared::OtherMethod
    } else {
        Declared::No
    }
}

/// Refuse any matched route the table does not declare. A request that matched no route goes on
/// to the router's own not-found answer.
pub(crate) async fn enforce(req: Request, next: Next) -> Response {
    let Some(path) = req.extensions().get::<MatchedPath>().cloned() else {
        return next.run(req).await;
    };
    match declared(req.method(), path.as_str()) {
        Declared::Yes(_) => next.run(req).await,
        Declared::OtherMethod => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        Declared::No => {
            eprintln!(
                "refused undeclared route {} {}",
                req.method(),
                path.as_str()
            );
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error":"This route has no declared permission."})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests;
