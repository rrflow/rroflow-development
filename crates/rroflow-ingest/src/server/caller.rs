//! Who is calling: the credential a request carries, checked against operator keys, project
//! keys, native sessions and Nomen.
use super::{ApiError, AppState, hash_key, internal, valid_devspace_id};
use crate::{AuthError, Grant, Permission, Role, Store, accounts, permissions, tenancy};
use axum::http::HeaderMap;
use serde::Serialize;

/// Who is calling, as their credential proved it.
#[derive(Clone, Debug, Serialize)]
pub struct Caller {
    /// The devspace every read and write of this request is confined to.
    pub devspace: String,
    /// `key` (operator-registered key), `project_key` or `nomen` (Nomen sign-in).
    pub via: &'static str,
    /// Nomen account ID, for `nomen`.
    pub subject: Option<String>,
    /// Nomen organization, when the token carried it. Not an RROFLOW organization.
    pub org: Option<String>,
    /// Login name Nomen reported, for display only.
    pub name: Option<String>,
    /// The RROFLOW account, for a signed-in member. Keys are not people.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// The organization that owns this project, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    /// The member's role in that organization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The project key's id, for `project_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// This credential can send records but read nothing back.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub write_only: bool,
}

impl Caller {
    /// What this credential carries into [`permissions::require`].
    pub fn grant(&self) -> Grant {
        match (self.write_only, self.role) {
            (true, _) => Grant::SendOnlyKey,
            (false, Some(role)) => Grant::Role(role),
            (false, None) => Grant::OperatorKey,
        }
    }
}

/// The project a request names, if any. Malformed or repeated values get the same answer as
/// a project that does not exist.
fn requested_project(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    let mut values = headers.get_all(tenancy::PROJECT_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(ApiError::Missing);
    }
    value
        .to_str()
        .ok()
        .filter(|id| valid_devspace_id(id))
        .map(Some)
        .ok_or(ApiError::Missing)
}

impl AppState {
    /// The caller, if their credential holds `permission` in the project the request names.
    /// Project routes start here, with the permission their entry in
    /// [`permissions::ROUTES`] declares.
    pub async fn allowed(
        &self,
        headers: &HeaderMap,
        permission: Permission,
    ) -> Result<Caller, ApiError> {
        let caller = self.authenticate(headers).await?;
        permissions::require(caller.grant(), permission)?;
        Ok(caller)
    }

    /// Identify the caller from `Authorization: Bearer <credential>`: an operator-registered
    /// key first (a hash lookup), then a project key, then a Nomen access token when sign-in
    /// is configured. Routes that act on a project use [`Self::allowed`].
    pub async fn authenticate(&self, headers: &HeaderMap) -> Result<Caller, ApiError> {
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or(ApiError::Unauthorized)?;
        let requested = requested_project(headers)?;
        // A key opens exactly one project; naming any other is refused, not ignored.
        let refuse_other = |devspace: &str| match requested {
            Some(project) if project != devspace => Err(ApiError::Missing),
            _ => Ok(()),
        };
        if let Some(devspace) = self.keys.get(&hash_key(token)) {
            refuse_other(devspace)?;
            return Ok(Caller {
                devspace: devspace.clone(),
                via: "key",
                subject: None,
                org: None,
                name: None,
                user: None,
                organization: None,
                role: None,
                key: None,
                write_only: false,
            });
        }
        if token.starts_with(tenancy::KEY_PREFIX) {
            let Store::Postgres(pg) = self.store.as_ref() else {
                return Err(ApiError::Unauthorized);
            };
            let key = pg
                .project_key(&hash_key(token))
                .await
                .map_err(internal)?
                .ok_or(ApiError::Unauthorized)?;
            refuse_other(&key.devspace)?;
            return Ok(Caller {
                devspace: key.devspace,
                via: "project_key",
                subject: None,
                org: None,
                name: None,
                user: None,
                organization: Some(key.organization),
                role: None,
                key: Some(key.id),
                write_only: true,
            });
        }
        if token.starts_with(accounts::SESSION_PREFIX)
            || token.starts_with(accounts::device::ACCESS_PREFIX)
        {
            let accounts = self.accounts.as_ref().ok_or(ApiError::Unauthorized)?;
            let device = token.starts_with(accounts::device::ACCESS_PREFIX);
            let (subject, project) = if device {
                let (subject, project) = accounts
                    .device_subject(token)
                    .await
                    .map_err(|_| ApiError::AccountUnavailable)?
                    .ok_or(ApiError::Unauthorized)?;
                refuse_other(&project)?;
                (subject, Some(project))
            } else {
                (
                    accounts
                        .subject(token)
                        .await
                        .map_err(|_| ApiError::AccountUnavailable)?
                        .ok_or(ApiError::Unauthorized)?,
                    None,
                )
            };
            let membership = self
                .store
                .membership(accounts::ISSUER, &subject, project.as_deref().or(requested))
                .await
                .map_err(internal)?
                .ok_or(ApiError::Missing)?;
            return Ok(Caller {
                devspace: membership.devspace,
                via: if device { "native_device" } else { "native" },
                subject: Some(subject.id),
                org: None,
                name: subject.name,
                user: Some(membership.user),
                organization: Some(membership.organization),
                role: Some(membership.role),
                key: None,
                write_only: false,
            });
        }
        let Some(nomen) = &self.nomen else {
            return Err(ApiError::Unauthorized);
        };
        let subject = match nomen.verify(token).await {
            Ok(s) => s,
            Err(AuthError::Invalid(why)) => {
                eprintln!("sign-in refused: {why}");
                return Err(ApiError::Unauthorized);
            }
            Err(AuthError::Unavailable(why)) => {
                eprintln!("issuer_unavailable: {why}");
                return Err(ApiError::IssuerUnavailable);
            }
        };
        let membership = self
            .store
            .membership(nomen.issuer(), &subject, requested)
            .await
            .map_err(internal)?
            .ok_or(ApiError::Missing)?;
        Ok(Caller {
            devspace: membership.devspace,
            via: subject.via,
            subject: Some(subject.id),
            org: subject.org,
            name: subject.name,
            user: Some(membership.user),
            organization: Some(membership.organization),
            role: Some(membership.role),
            key: None,
            write_only: false,
        })
    }
}
