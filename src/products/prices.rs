//! Price observations: what a shop charged for a listing, when. Money is always
//! integer minor units (pence) on the wire and in the DB — never a float.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A price a source reported for a listing. The client sends this on import
/// (derived from an Asda hit or a Waitrose product); the backend appends it to
/// the listing's price history.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PriceInput {
    /// Shelf price in minor units (pence for GBP).
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: String,
    /// Price per unit of measure (minor units) + the measure, for fair
    /// cross-pack comparison. e.g. 892 + "KG".
    #[ts(type = "number | null")]
    pub unit_amount_minor: Option<i64>,
    pub unit_measure: Option<String>,
    /// Nation the price is for (Asda EN/NI/SC/WA); null when the source has one.
    pub region: Option<String>,
}

/// What one shop currently charges for a product — the `prices` part of the
/// product detail (GET /api/products/id/{id}), cheapest shop first.
///
/// Exactly one row per source: a shop can list the same physical product twice
/// (two Asda CINs sharing an EAN), and "where do I buy this, for how much" wants
/// one answer per shop — the cheapest. `external_id` names the listing that
/// quoted this price, so the shop link goes to the item actually being quoted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ShopPrice {
    /// The listing's source ('asda', 'waitrose', …). Unique within a response.
    pub source: String,
    /// Source-scoped id of the listing this price came from.
    pub external_id: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: String,
    #[ts(type = "number | null")]
    pub unit_amount_minor: Option<i64>,
    pub unit_measure: Option<String>,
    pub region: Option<String>,
    /// When observed, epoch milliseconds (UTC).
    #[ts(type = "number")]
    pub observed_at: i64,
}
