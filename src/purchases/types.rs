//! Purchases on the wire and in the database.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::products::ids::ProductId;
use crate::products::prices::{Currency, UnitPrice};

/// Only what the person types; the rest is copied from the row being bought.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewPurchase {
    /// Free text: "the corner shop" is a real answer.
    pub shop: String,
    /// Minor units.
    #[ts(type = "number")]
    pub amount_minor: i64,
    #[serde(default = "Currency::gbp")]
    pub currency: Currency,
    /// For something recorded after the fact; absent means now. A date: nobody
    /// knows what time they bought a dishwasher.
    #[serde(default)]
    pub bought_on: Option<NaiveDate>,
    /// Absent: none recorded, not none (0046).
    #[serde(default)]
    pub warranty_months: Option<i32>,
}

crate::row_id! {
    /// `purchases.id`.
    PurchaseId
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Purchase {
    #[ts(type = "number")]
    pub id: PurchaseId,
    pub product_id: Option<ProductId>,
    /// The one key that always exists.
    #[ts(type = "number | null")]
    pub item_id: Option<crate::inventory::types::ItemId>,
    pub barcode: Option<String>,
    /// The name when bought, which no later correction changes.
    pub name: String,
    pub shop: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    /// Derived on read: per kg, litre or item, for comparing packs.
    #[sqlx(skip)]
    pub unit_price: Option<UnitPrice>,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub bought_at: DateTime<Utc>,
    /// `None` is "not recorded", and renders as nothing.
    pub warranty_months: Option<i32>,
    /// Derived on read, so it cannot drift from what it is measured from.
    #[sqlx(default)]
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    #[ts(type = "number | null")]
    pub warranty_until: Option<DateTime<Utc>>,
}
