//! Settings an instance admin changes in the console.
//!
//! [`GROUPS`] is the one list of them. The API and the settings screen are both built from it, so
//! a setting added here appears in the console with its range, default and recommendations, and
//! nothing else needs to know its numbers. The file configuration keeps only what must exist
//! before the service can start, such as the database address.

use crate::permissions::{self, Grant, Permission};
use crate::tenancy::signed_in;
use crate::{ApiError, AppState, internal};
use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, types::Json as SqlJson};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Preset {
    value: i64,
    label: &'static str,
    recommended: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct Definition {
    key: &'static str,
    label: &'static str,
    help: &'static str,
    min: i64,
    max: i64,
    default: i64,
    presets: &'static [Preset],
}

#[derive(Debug, Serialize)]
pub(crate) struct Group {
    id: &'static str,
    label: &'static str,
    settings: &'static [Definition],
}

pub(crate) const PASSWORD_MIN_LENGTH: &str = "password_min_length";

const fn preset(value: i64, label: &'static str, recommended: bool) -> Preset {
    Preset {
        value,
        label,
        recommended,
    }
}

pub(crate) const GROUPS: &[Group] = &[Group {
    id: "sign_in",
    label: "Sign-in and passwords",
    settings: &[Definition {
        key: PASSWORD_MIN_LENGTH,
        label: "Minimum password length",
        help: "Applies when someone sets a new password. Existing passwords keep working.",
        min: 8,
        max: 18,
        default: 8,
        presets: &[
            preset(8, "8", false),
            preset(10, "10", false),
            preset(12, "12", false),
            preset(15, "15", true),
            preset(18, "18", true),
        ],
    }],
}];

fn definition(key: &str) -> Option<&'static Definition> {
    GROUPS
        .iter()
        .flat_map(|group| group.settings)
        .find(|setting| setting.key == key)
}

/// The value to use: what was saved if it is still in range, otherwise the default.
fn effective(setting: &Definition, document: &Map<String, Value>) -> i64 {
    document
        .get(setting.key)
        .and_then(Value::as_i64)
        .filter(|value| (setting.min..=setting.max).contains(value))
        .unwrap_or(setting.default)
}

async fn load(pool: &PgPool) -> Result<(Map<String, Value>, i64), sqlx::Error> {
    let row: Option<(SqlJson<Map<String, Value>>, i64)> =
        sqlx::query_as("SELECT document,revision FROM instance_settings")
            .fetch_optional(pool)
            .await?;
    Ok(row.map_or((Map::new(), 0), |(document, revision)| {
        (document.0, revision)
    }))
}

/// The current value of one setting.
pub(crate) async fn integer(pool: &PgPool, key: &str) -> anyhow::Result<i64> {
    let setting = definition(key).ok_or_else(|| anyhow::anyhow!("unknown setting {key}"))?;
    Ok(effective(setting, &load(pool).await?.0))
}

/// The lowest and highest values marked as recommended.
pub(crate) fn recommended_range(key: &str) -> Option<(i64, i64)> {
    let presets = definition(key)?.presets.iter().filter(|p| p.recommended);
    let values: Vec<i64> = presets.map(|p| p.value).collect();
    Some((*values.iter().min()?, *values.iter().max()?))
}

fn view(document: &Map<String, Value>, revision: i64) -> Value {
    let groups: Vec<Value> = GROUPS
        .iter()
        .map(|group| {
            let settings: Vec<Value> = group
                .settings
                .iter()
                .map(|setting| {
                    let mut item = json!(setting);
                    item["value"] = json!(effective(setting, document));
                    item
                })
                .collect();
            json!({"id": group.id, "label": group.label, "settings": settings})
        })
        .collect();
    json!({"revision": revision, "groups": groups})
}

pub(crate) async fn is_instance_admin(pool: &PgPool, user: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM instance_admins WHERE user_id=$1)")
        .bind(user)
        .fetch_one(pool)
        .await
}

async fn admin_pool<'a>(
    st: &'a AppState,
    headers: &HeaderMap,
) -> Result<(&'a PgPool, String), ApiError> {
    let (pg, user) = signed_in(st, headers).await?;
    let grant = if is_instance_admin(&pg.pool, &user)
        .await
        .map_err(|e| internal(e.into()))?
    {
        Grant::InstanceAdmin
    } else {
        Grant::InstanceMember
    };
    permissions::require(grant, Permission::InstanceSettings)?;
    Ok((&pg.pool, user))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let (pool, _) = admin_pool(&st, &headers).await?;
    let (document, revision) = load(pool).await.map_err(|e| internal(e.into()))?;
    Ok(Json(view(&document, revision)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Update {
    revision: i64,
    values: BTreeMap<String, i64>,
}

fn validate(values: &BTreeMap<String, i64>) -> Result<(), ApiError> {
    for (key, value) in values {
        let setting = definition(key)
            .ok_or_else(|| ApiError::BadRequest(format!("Unknown setting {key}.")))?;
        if !(setting.min..=setting.max).contains(value) {
            return Err(ApiError::BadRequest(format!(
                "{} must be between {} and {}.",
                setting.label, setting.min, setting.max
            )));
        }
    }
    Ok(())
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Update>,
) -> Result<Json<Value>, ApiError> {
    let (pool, user) = admin_pool(&st, &headers).await?;
    validate(&body.values)?;
    let db = |e: sqlx::Error| internal(e.into());
    let mut tx = pool.begin().await.map_err(db)?;
    sqlx::query("INSERT INTO instance_settings(singleton) VALUES(true) ON CONFLICT DO NOTHING")
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let (SqlJson(mut document), revision): (SqlJson<Map<String, Value>>, i64) =
        sqlx::query_as("SELECT document,revision FROM instance_settings FOR UPDATE")
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
    if revision != body.revision {
        return Err(ApiError::SettingsConflict);
    }
    let mut changed = Map::new();
    for (key, value) in &body.values {
        let setting = definition(key).ok_or(ApiError::Internal)?;
        let before = effective(setting, &document);
        if before != *value {
            changed.insert(key.clone(), json!({"from": before, "to": value}));
        }
        document.insert(key.clone(), json!(value));
    }
    let revision = revision + 1;
    sqlx::query("UPDATE instance_settings SET document=$1,revision=$2,updated_by=$3,updated_at=clock_timestamp()")
        .bind(SqlJson(&document))
        .bind(revision)
        .bind(&user)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let audited = sqlx::query("INSERT INTO audit_events(organization_id,actor_user_id,action,target,detail) SELECT id,$1,'instance.settings_changed','instance',$2 FROM organizations WHERE personal AND created_by=$1")
        .bind(&user)
        .bind(SqlJson(json!({"changed": changed})))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    if audited.rows_affected() == 0 {
        return Err(ApiError::Internal);
    }
    tx.commit().await.map_err(db)?;
    Ok(Json(view(&document, revision)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_minimum_starts_low_and_recommends_longer() {
        let setting = definition(PASSWORD_MIN_LENGTH).unwrap();
        assert_eq!((setting.min, setting.max, setting.default), (8, 18, 8));
        assert_eq!(recommended_range(PASSWORD_MIN_LENGTH), Some((15, 18)));
        let starting: Vec<i64> = setting
            .presets
            .iter()
            .filter(|p| !p.recommended)
            .map(|p| p.value)
            .collect();
        assert_eq!(starting, [8, 10, 12]);
    }

    #[test]
    fn a_saved_value_out_of_range_falls_back_to_the_default() {
        let setting = definition(PASSWORD_MIN_LENGTH).unwrap();
        let mut document = Map::new();
        assert_eq!(effective(setting, &document), 8);
        document.insert(PASSWORD_MIN_LENGTH.into(), json!(12));
        assert_eq!(effective(setting, &document), 12);
        document.insert(PASSWORD_MIN_LENGTH.into(), json!(3));
        assert_eq!(effective(setting, &document), 8);
    }

    #[test]
    fn updates_are_refused_when_unknown_or_out_of_range() {
        let ok = BTreeMap::from([(PASSWORD_MIN_LENGTH.to_string(), 15)]);
        assert!(validate(&ok).is_ok());
        let low = BTreeMap::from([(PASSWORD_MIN_LENGTH.to_string(), 7)]);
        assert!(matches!(validate(&low), Err(ApiError::BadRequest(_))));
        let high = BTreeMap::from([(PASSWORD_MIN_LENGTH.to_string(), 19)]);
        assert!(matches!(validate(&high), Err(ApiError::BadRequest(_))));
        let unknown = BTreeMap::from([("nonsense".to_string(), 1)]);
        assert!(matches!(validate(&unknown), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn every_setting_has_a_default_inside_its_range() {
        for setting in GROUPS.iter().flat_map(|group| group.settings) {
            assert!(
                (setting.min..=setting.max).contains(&setting.default),
                "{}",
                setting.key
            );
            for preset in setting.presets {
                assert!(
                    (setting.min..=setting.max).contains(&preset.value),
                    "{}",
                    setting.key
                );
            }
        }
    }
}
