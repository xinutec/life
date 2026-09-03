//! The shapes a purchase takes on the wire and in the database.

use chrono::{DateTime, NaiveDate, Utc};
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
    /// When it was bought, for something being recorded AFTER the fact — an
    /// appliance you have owned for two years, entered so its warranty has a
    /// start. Absent means now, which is what the buy-list flow means every
    /// time: you are standing in the shop.
    ///
    /// A DATE, not a datetime, and the server does the conversion. Nobody knows
    /// what time of day they bought a dishwasher, and a client that picks
    /// midnight in its own zone hands the server a day that can be off by one.
    #[serde(default)]
    pub bought_on: Option<NaiveDate>,
    /// How many months of cover the receipt says, if any. Absent means no
    /// warranty was recorded — NOT that there is none. See migration 0046.
    #[serde(default)]
    pub warranty_months: Option<i32>,
}

fn default_currency() -> String {
    "GBP".into()
}

/// A recorded purchase, as it reads back.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Purchase {
    #[ts(type = "number")]
    pub id: u64,
    pub product_id: Option<ProductId>,
    /// The cupboard item this bought. The one key that ALWAYS exists, because a
    /// purchase is only recorded by buying something and buying is what creates
    /// the item — a hand-typed buy-list row has no barcode and no product, and
    /// was unreachable without this (migration 0044).
    #[ts(type = "number | null")]
    pub item_id: Option<u64>,
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
    /// DERIVED, never stored: what this works out to per kg / per litre / per
    /// item, in minor units. Computed on read from `amount_minor` and the pack,
    /// so it cannot drift from them the way a second stored column would.
    ///
    /// `None` when the pack is unknown or its unit is one `packsize::parse`
    /// refuses — an unreadable unit means the rate is unknown, and inventing a
    /// dimension for it would be worse than saying nothing.
    ///
    /// Rounded to the nearest minor unit. It is a RATE for comparing packs, not
    /// an amount anybody paid; the exact figure is `amount_minor`.
    #[sqlx(default)]
    #[ts(type = "number | null")]
    pub unit_amount_minor: Option<i64>,
    /// "KG" / "L" / "each" — the scale `unit_amount_minor` is quoted per,
    /// matching how the shop rows above it read ("£8.00/KG").
    #[sqlx(default)]
    pub unit_measure: Option<String>,
    #[ts(type = "string")]
    pub bought_at: DateTime<Utc>,
    /// Months of cover from `bought_at`, as recorded. `None` is "not recorded",
    /// which most purchases are and should render as nothing at all.
    pub warranty_months: Option<i32>,
    /// DERIVED, never stored: `bought_at` plus `warranty_months`. Computed on
    /// read so it cannot drift from the purchase it is measured from — a stored
    /// end date can outlive a correction to either half.
    #[sqlx(default)]
    #[ts(type = "string | null")]
    pub warranty_until: Option<DateTime<Utc>>,
}
