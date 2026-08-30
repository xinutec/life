//! The shapes a purchase takes on the wire and in the database.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::products::ids::ProductId;

/// What the client says when a buy-list row is marked bought AND the price was
/// noted. Every field the person has to type is here; everything else (what it
/// was, what pack, when) is copied from the row being bought, because asking
/// again for what the app already knows is how a capture step stops being used.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewPurchase {
    /// Free text. "the corner shop" is a real answer — see migration 0043.
    pub shop: String,
    /// Minor units (pence for GBP). Integer, never a float: money must be exact.
    #[ts(type = "number")]
    pub amount_minor: i64,
    /// ISO 4217. Defaulted rather than required, because the overwhelmingly
    /// common case should cost no keystrokes.
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "GBP".into()
}

/// A recorded purchase, as it reads back.
#[derive(Debug, Clone, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Purchase {
    #[ts(type = "number")]
    pub id: u64,
    pub product_id: Option<ProductId>,
    pub barcode: Option<String>,
    /// What it was called when it was bought — the one field no later
    /// correction to the catalogue can invalidate.
    pub name: String,
    pub shop: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    #[ts(type = "string")]
    pub bought_at: DateTime<Utc>,
}
