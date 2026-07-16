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

/// The latest price for one shop of a product — returned by
/// GET /api/products/id/{id}/prices, cheapest first.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ShopPrice {
    /// The listing's source ('asda', 'waitrose', …).
    pub source: String,
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
