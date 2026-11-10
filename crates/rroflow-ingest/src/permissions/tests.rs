use super::*;
use std::collections::BTreeSet;

/// Every `.route(path, method(..).method(..))` in `router()`, read from its source. A route added
/// any other way would not be found here, so `router()` must keep registering them this way.
fn served_routes() -> BTreeSet<(String, String)> {
    let source = include_str!("../lib.rs");
    let start = source.find("pub fn router(").expect("router() in lib.rs");
    let body = &source[start..];
    // The checkout may use CRLF line endings.
    let end = body
        .find("\n}\n")
        .or_else(|| body.find("\n}\r\n"))
        .expect("end of router()");
    let body = &body[..end];
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find(".route(") {
        rest = &rest[at + ".route(".len()..];
        let open = rest.find('"').expect("route path");
        let close = open + 1 + rest[open + 1..].find('"').expect("route path end");
        let path = rest[open + 1..close].to_string();
        // Walk the remaining arguments to the `)` that closes `.route(`, collecting method
        // calls at the top level: `get(..)`, `.post(..)`, `axum::routing::put(..)`.
        let args = &rest[close + 1..];
        let bytes = args.as_bytes();
        let (mut depth, mut in_string, mut end) = (0usize, false, None);
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'"' => in_string = !in_string,
                _ if in_string => {}
                b'(' => {
                    if depth == 0 {
                        let before = &args[..i];
                        let word_start = before
                            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                            .map_or(0, |p| p + 1);
                        let word = &before[word_start..];
                        if ["get", "post", "put", "delete", "patch"].contains(&word) {
                            out.insert((word.to_ascii_uppercase(), path.clone()));
                        }
                    }
                    depth += 1;
                }
                b')' => {
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        rest = &args[end.expect("closing parenthesis of .route(")..];
    }
    out
}

#[test]
fn every_served_route_is_declared_and_every_declaration_is_served() {
    let served = served_routes();
    assert!(served.len() > 60, "parser found too few routes: {served:?}");
    let declared: BTreeSet<(String, String)> = ROUTES
        .iter()
        .map(|(m, p, _)| (m.to_string(), p.to_string()))
        .collect();
    assert_eq!(ROUTES.len(), declared.len(), "a route is declared twice");
    let undeclared: Vec<_> = served.difference(&declared).collect();
    let unserved: Vec<_> = declared.difference(&served).collect();
    assert!(
        undeclared.is_empty(),
        "routes without a declared permission: {undeclared:?}"
    );
    assert!(unserved.is_empty(), "declared but not served: {unserved:?}");
}

#[test]
fn an_undeclared_route_is_refused_and_a_wrong_method_is_not_allowed() {
    assert_eq!(
        declared(&Method::GET, "/v1/turns"),
        Declared::Yes(Access::Requires(Permission::ProjectRead))
    );
    assert_eq!(
        declared(&Method::HEAD, "/healthz"),
        Declared::Yes(Access::Public)
    );
    assert_eq!(
        declared(&Method::DELETE, "/v1/turns"),
        Declared::OtherMethod
    );
    assert_eq!(declared(&Method::GET, "/v1/not-declared"), Declared::No);
}

#[test]
fn each_permission_in_a_route_has_the_scope_its_path_names() {
    for (method, path, access) in ROUTES {
        let needed: &[Permission] = match access {
            Access::Requires(permission) => std::slice::from_ref(permission),
            Access::AnyOf(permissions) => permissions,
            Access::Public | Access::SignIn | Access::Own => continue,
        };
        let scope = if path.contains("{organization}") {
            Scope::Organization
        } else {
            Scope::Project
        };
        for permission in needed {
            assert_eq!(permission.scope(), scope, "{method} {path}");
        }
    }
}

#[test]
fn role_bundles_allow_and_deny_exactly_this() {
    use Permission::*;
    let expected: [(Grant, &[Permission]); 6] = [
        (Grant::Role(Role::Viewer), &[ProjectRead, OrganizationRead]),
        (
            Grant::Role(Role::Member),
            &[ProjectRead, ProjectSend, ProjectWrite, OrganizationRead],
        ),
        (Grant::Role(Role::Admin), &Permission::ALL),
        (Grant::Role(Role::Owner), &Permission::ALL),
        (
            Grant::OperatorKey,
            &[ProjectRead, ProjectSend, ProjectWrite],
        ),
        (Grant::SendOnlyKey, &[ProjectSend]),
    ];
    for (grant, allowed) in expected {
        for permission in Permission::ALL {
            assert_eq!(
                grant.allows(permission),
                allowed.contains(&permission),
                "{grant:?} {permission:?}"
            );
            assert_eq!(
                require(grant, permission).is_ok(),
                allowed.contains(&permission)
            );
        }
    }
}

#[test]
fn a_higher_role_holds_everything_a_lower_one_does() {
    let roles = [Role::Viewer, Role::Member, Role::Admin, Role::Owner];
    for pair in roles.windows(2) {
        for permission in Permission::ALL {
            if Grant::Role(pair[0]).allows(permission) {
                assert!(
                    Grant::Role(pair[1]).allows(permission),
                    "{pair:?} {permission:?}"
                );
            }
        }
    }
}

#[test]
fn refusals_say_what_was_missing() {
    let message = |grant, permission| match require(grant, permission) {
        Err(ApiError::Forbidden(message)) => message,
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert_eq!(
        message(Grant::SendOnlyKey, Permission::ProjectRead),
        "This project key can send records but not read them."
    );
    assert_eq!(
        message(Grant::Role(Role::Viewer), Permission::ProjectWrite),
        "Your role can view this project but not change it."
    );
    assert_eq!(
        message(Grant::Role(Role::Member), Permission::DatasetsExport),
        "An organization admin or owner manages dataset consent and exports."
    );
    assert_eq!(
        message(Grant::Role(Role::Member), Permission::MembersInvite),
        "An organization admin or owner must do this."
    );
}

#[test]
fn permission_names_are_stable() {
    let names: Vec<String> = Permission::ALL
        .iter()
        .map(|p| {
            serde_json::to_value(p)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        [
            "project.read",
            "project.send",
            "project.write",
            "keys.manage",
            "datasets.manage",
            "datasets.export",
            "organization.read",
            "projects.create",
            "members.invite",
            "members.manage",
            "audit.read",
        ]
    );
}
