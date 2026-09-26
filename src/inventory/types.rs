//! Domain types for the location/item model. `kind` and `category` are stored
//! as short strings in the DB and parsed into these enums at the repo boundary.

use crate::products::ids::ProductId;
use crate::str_enum;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    /// A node kind in the spatial tree.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum LocationKind: "location kind" {
        House => "house",
        Room => "room",
        Cupboard => "cupboard",
        Fridge => "fridge",
        Layer => "layer",
    }
}

str_enum! {
    /// Whose name an item carries.
    ///
    /// Stated by the client that owns the form: the server cannot see whether a
    /// person touched the name field, and "differs from the product" is not it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ItemNameSource: "item name source" {
        /// Somebody typed this name deliberately. It outranks the catalogue, and a
        /// later product correction leaves it alone.
        User => "user",
        /// The name came from the catalogue, or was left to it. Follows the linked
        /// product forever, so a correction reaches the cupboard with no refresh.
        Product => "product",
    }
}

str_enum! {
    /// How much of an `expiry` date was actually printed on the thing.
    ///
    /// A medicine box is printed MM/YYYY and `items.expiry` is a DATE, so the
    /// month's LAST day is stored (good THROUGH June). The precision travels with
    /// it so nothing renders or counts down to a day that was never printed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ExpiryPrecision: "expiry precision" {
        /// The date is exactly what was printed.
        Day => "day",
        /// Only the month was printed; `expiry` holds that month's last day.
        Month => "month",
    }
}

str_enum! {
    /// What kind of thing an item is.
    ///
    /// Split by where a thing lives and what you ask of it, not by material:
    /// `Cookware` and `Tableware` are different cupboards and questions. `Other`
    /// is offered last, or it becomes the bucket for everything.
    ///
    /// ⚠ A closed set, so a new kind needs a deploy; the column and the sync
    /// schema are free strings (see docs/TODO.md).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ItemCategory: "item category" {
        Food => "food",
        Medication => "medication",
        /// Pans, baking trays, the things you cook WITH.
        Cookware => "cookware",
        /// Glasses, plates, cutlery — what you eat and drink FROM.
        Tableware => "tableware",
        Clothing => "clothing",
        /// Anything with a plug and a warranty.
        Appliance => "appliance",
        /// Detergent, sponges, refills — bought repeatedly, never eaten.
        Cleaning => "cleaning",
        Tool => "tool",
        Document => "document",
        Other => "other",
    }
}
/// A spatial node as returned by the API. (Exported to TS as `Loc`.)
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, rename = "Loc")]
pub struct Location {
    // ids are JSON numbers on the wire; ts-rs would otherwise emit `bigint`.
    #[ts(type = "number")]
    pub id: u64,
    pub kind: LocationKind,
    pub name: String,
    #[ts(type = "number | null")]
    pub parent_id: Option<u64>,
    pub sort_order: i32,
    #[ts(type = "unknown | null")]
    pub position: Option<serde_json::Value>,
}

/// A tracked item (holding) as returned by the API. `name`/`brand`/`barcode`/
/// `has_image` are *resolved*: they come from the linked catalog product when
/// `product_id` is set, falling back to the item's own fields otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Item {
    #[ts(type = "number")]
    pub id: u64,
    pub product_id: Option<ProductId>,
    pub name: String,
    pub brand: Option<String>,
    pub category: ItemCategory,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub expiry: Option<NaiveDate>,
    /// How much of `expiry` was printed rather than invented to fill the DATE.
    /// Meaningless when `expiry` is `None`.
    pub expiry_precision: ExpiryPrecision,
    #[ts(type = "number | null")]
    pub location_id: Option<u64>,
    pub barcode: Option<String>,
    /// True when the linked product has a cached image
    /// (served from /api/products/{barcode}/image).
    pub has_image: bool,
}

str_enum! {
    /// What happened to a stock row, as recorded in `item_history`.
    ///
    /// A closed set, for the reason `products::Source` is one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export)]
    pub enum ItemEvent: "item event" {
        Added => "added",
        Moved => "moved",
        Removed => "removed",
        Restored => "restored",
        /// Some of it was used up. The only event that carries a *delta* rather
        /// than a state: `quantity` is how much went, not how much is left.
        Used => "used",
        /// You judged it low, by putting it on the Buy list. A decision, not a
        /// measurement, so it carries no quantity — the signal is the INTERVAL
        /// between them. Prefer it to `used`, which nobody ever writes.
        Low => "low",
    }
}

/// Everything the history dialog shows for one stock row. Purchases sit beside
/// the events, not among them: an `ItemEvent` read back must be a stored value,
/// and nothing stores a `bought` event. It is the only view of a purchase made
/// from a hand-typed buy-list row, which has no product page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ItemHistory {
    pub entries: Vec<ItemHistoryEntry>,
    pub purchases: Vec<crate::purchases::types::Purchase>,
}

/// One thing that happened to a stock row — a line of its history.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ItemHistoryEntry {
    #[ts(type = "number")]
    pub id: u64,
    pub event: ItemEvent,
    /// How much, in the item's own unit. ⚠ For [`ItemEvent::Used`] the amount
    /// that WENT; for every other event what the row held. Say which when
    /// rendering.
    pub quantity: Option<f64>,
    /// Where the row was, by name. `None` if unrecorded or since deleted.
    pub location: Option<String>,
    /// When, Unix milliseconds (UTC).
    #[ts(type = "number")]
    pub at: i64,
}

// `event` is a VARCHAR, so this delegates to `str` rather than deriving
// sqlx::Type (which would declare a SQL ENUM — see products::source).
impl sqlx::Type<sqlx::MySql> for ItemEvent {
    fn type_info() -> <sqlx::MySql as sqlx::Database>::TypeInfo {
        <str as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &<sqlx::MySql as sqlx::Database>::TypeInfo) -> bool {
        <str as sqlx::Type<sqlx::MySql>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::MySql> for ItemEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::MySql as sqlx::Database>::ArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<'q, sqlx::MySql>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ItemEvent {
    fn decode(
        value: <sqlx::MySql as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        <&str as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?
            .parse()
            .map_err(Into::into)
    }
}

/// Request body for "I used some of this": how much went, in which unit.
///
/// `unit` is required to *agree* with the row's own (see
/// [[super::consume]]) — sending it rather than assuming the row's unit is what
/// lets the server refuse "200 g" against a jar instead of subtracting 200 from
/// 1. Absent means the row is expected to be unitless too.
#[derive(Debug, Deserialize)]
pub struct UseItem {
    pub quantity: f64,
    #[serde(default)]
    pub unit: Option<String>,
}

/// Request body for creating a location.
#[derive(Debug, Deserialize)]
pub struct NewLocation {
    pub kind: LocationKind,
    pub name: String,
    pub parent_id: Option<u64>,
    #[serde(default)]
    pub sort_order: i32,
    pub position: Option<serde_json::Value>,
}

/// Request body for creating an item.
#[derive(Debug, Deserialize)]
pub struct NewItem {
    pub name: String,
    #[serde(default = "default_category")]
    pub category: ItemCategory,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub expiry: Option<NaiveDate>,
    /// How much of `expiry` is real, when the client knows. Absent means "no
    /// statement": a new item defaults to [`ExpiryPrecision::Day`], and an
    /// update leaves whatever the item already had.
    ///
    /// The same rule as `name_source`: callers other than the item form send
    /// nothing, and must not turn an invented month-end into a printed day.
    #[serde(default)]
    pub expiry_precision: Option<ExpiryPrecision>,
    pub location_id: Option<u64>,
    #[serde(default)]
    pub barcode: Option<String>,
    /// Explicit catalog link. Takes precedence over barcode-based resolution and
    /// is the only way to link a barcodeless shop product (Waitrose etc.).
    #[serde(default)]
    pub product_id: Option<ProductId>,
    /// Whose name `name` is, when the client knows. Absent means "no statement":
    /// a new item defaults to [`ItemNameSource::Product`], and an update leaves
    /// whatever the item already had.
    ///
    /// Absent is what every caller but the item form sends.
    #[serde(default)]
    pub name_source: Option<ItemNameSource>,
}

fn default_category() -> ItemCategory {
    ItemCategory::Other
}
