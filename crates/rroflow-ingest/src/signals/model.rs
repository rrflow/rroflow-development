//! The signal catalogue document.
use rroflow_core::signals::{CATALOGUE, Category};

/// Schema id of the signal catalogue document.
pub const SIGNALS_SCHEMA: &str = "rroflow.signals/v1";

/// The signal catalogue, grouped: every category with its signals in catalogue order.
pub(crate) fn catalogue() -> serde_json::Value {
    serde_json::json!({
        "schema": SIGNALS_SCHEMA,
        "categories": Category::ALL
            .iter()
            .map(|c| serde_json::json!({
                "id": c,
                "label": c.label(),
                "meaning": c.meaning(),
                "signals": CATALOGUE.iter().filter(|s| s.category == *c).map(|s| s.id).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
        "signals": CATALOGUE,
    })
}
