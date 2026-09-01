//! Domain types for the location/item model. `kind` and `category` are stored
//! as short strings in the DB and parsed into these enums at the repo boundary.

use std::fmt;
use std::str::FromStr;

use crate::products::ids::ProductId;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A node kind in the spatial tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LocationKind {
    House,
    Room,
    Cupboard,
    Fridge,
    Layer,
}

impl fmt::Display for LocationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            LocationKind::House => "house",
            LocationKind::Room => "room",
            LocationKind::Cupboard => "cupboard",
            LocationKind::Fridge => "fridge",
            LocationKind::Layer => "layer",
        };
        f.write_str(s)
    }
}

impl FromStr for LocationKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "house" => Ok(LocationKind::House),
            "room" => Ok(LocationKind::Room),
            "cupboard" => Ok(LocationKind::Cupboard),
            "fridge" => Ok(LocationKind::Fridge),
            "layer" => Ok(LocationKind::Layer),
            other => Err(format!("unknown location kind {other:?}")),
        }
    }
}

/// Whose name an item carries.
///
/// The server cannot work this out, and trying to is how it goes wrong. It sees
/// a name and a linked product; it cannot see whether a person TOUCHED the name
/// field. Inferring "differs from the product, so it was authored" mislabels
/// every hurried word typed at a cupboard door as an intention, and inferring it
/// only on update assumes every client prefills the displayed name — which the
/// web form happens to do and a sync client, a script or the Android app need
/// not. So the client that owns the form says so explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ItemNameSource {
    /// Somebody typed this name deliberately. It outranks the catalogue, and a
    /// later product correction leaves it alone.
    User,
    /// The name came from the catalogue, or was left to it. Follows the linked
    /// product forever, so a correction reaches the cupboard with no refresh.
    Product,
}

impl fmt::Display for ItemNameSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ItemNameSource::User => "user",
            ItemNameSource::Product => "product",
        })
    }
}

/// How much of an `expiry` date was actually printed on the thing.
///
/// A medicine box is printed MM/YYYY, and `items.expiry` is a DATE, so a day
/// has to be invented to store one at all. The convention is the month's LAST
/// day — a box marked 06/2028 is good THROUGH June, and the 1st would expire it
/// twenty-nine days early — but the convention alone is not enough, because a
/// reader cannot tell an invented 30th from a printed one. Rendering "30 Jun
/// 2028" states a day that appears nowhere on the box; counting down "in 2d"
/// through the end of the month claims something changes overnight that does
/// not. So the precision travels with the date (migration 0045).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExpiryPrecision {
    /// The date is exactly what was printed.
    Day,
    /// Only the month was printed; `expiry` holds that month's last day.
    Month,
}

impl fmt::Display for ExpiryPrecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ExpiryPrecision::Day => "day",
            ExpiryPrecision::Month => "month",
        })
    }
}

impl FromStr for ExpiryPrecision {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "day" => Ok(ExpiryPrecision::Day),
            "month" => Ok(ExpiryPrecision::Month),
            other => Err(format!("unknown expiry precision {other:?}")),
        }
    }
}

/// What kind of thing an item is.
///
/// "Generic from day one — food is just the first skin" was the intent, but the
/// list was food's skin and nothing else's: a house is full of pans, glasses and
/// clothes, and every one of them landed in `Other`. Measured 2026-08-31, and the
/// tell is that `Other` had come to hold BOTH — four genuinely non-food things
/// AND two foods (an avocado, a protein drink) that somebody filed there because
/// nothing fitted. A bucket that means "not food" and "nobody said" at the same
/// time cannot group, filter or answer anything.
///
/// Split by WHERE A THING LIVES AND WHAT YOU ASK OF IT, not by material:
/// `Cookware` and `Tableware` are different cupboards and different questions
/// ("which pan", "how many glasses"), while a steel pan and a steel fork have
/// nothing to say to each other.
///
/// ⚠ Still a closed set, so adding a kind needs a deploy. That is a known limit
/// rather than a decision that this is enough — the column is VARCHAR and the
/// client's sync schema already stores a free string, so nothing below this
/// layer constrains it. See the follow-up task on user-defined categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ItemCategory {
    Food,
    Medication,
    /// Pans, baking trays, the things you cook WITH.
    Cookware,
    /// Glasses, plates, cutlery — what you eat and drink FROM.
    Tableware,
    Clothing,
    /// Anything with a plug and a warranty; the category #131 hangs off.
    Appliance,
    /// Detergent, sponges, refills — bought repeatedly, never eaten.
    Cleaning,
    Tool,
    Document,
    Other,
}

impl fmt::Display for ItemCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ItemCategory::Food => "food",
            ItemCategory::Medication => "medication",
            ItemCategory::Cookware => "cookware",
            ItemCategory::Tableware => "tableware",
            ItemCategory::Clothing => "clothing",
            ItemCategory::Appliance => "appliance",
            ItemCategory::Cleaning => "cleaning",
            ItemCategory::Tool => "tool",
            ItemCategory::Document => "document",
            ItemCategory::Other => "other",
        };
        f.write_str(s)
    }
}

impl FromStr for ItemCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "food" => Ok(ItemCategory::Food),
            "medication" => Ok(ItemCategory::Medication),
            "cookware" => Ok(ItemCategory::Cookware),
            "tableware" => Ok(ItemCategory::Tableware),
            "clothing" => Ok(ItemCategory::Clothing),
            "appliance" => Ok(ItemCategory::Appliance),
            "cleaning" => Ok(ItemCategory::Cleaning),
            "tool" => Ok(ItemCategory::Tool),
            "document" => Ok(ItemCategory::Document),
            "other" => Ok(ItemCategory::Other),
            other => Err(format!("unknown item category {other:?}")),
        }
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

/// What happened to a stock row, as recorded in `item_history`.
///
/// A closed set in the type system rather than four spellings of a `VARCHAR(16)`
/// scattered through the repo — the same reason `products::Source` is one. The
/// history table is about to start earning its keep (consumption is what makes
/// "how much is left" and "what am I running out of" answerable), so the set it
/// is keyed on should be something the compiler knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ItemEvent {
    Added,
    Moved,
    Removed,
    Restored,
    /// Some of it was used up. The only event that carries a *delta* rather
    /// than a state: `quantity` is how much went, not how much is left.
    Used,
}

/// Everything the history dialog shows for one stock row.
///
/// The purchases ride ALONGSIDE the events rather than among them. A purchase is
/// a fact about the item, not one of the things that happened to it, and
/// `ItemEvent` is read back out of the database under a rule that an unknown
/// value fails loudly — synthesising a `bought` event that nothing ever stores
/// would put a value in that union which the table can never contain.
///
/// This is also the ONLY way to reach a purchase made against a hand-typed
/// buy-list row: it has no barcode and no catalogue product, so the product page
/// cannot show it (migration 0044).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ItemHistory {
    pub entries: Vec<ItemHistoryEntry>,
    pub purchases: Vec<crate::purchases::types::Purchase>,
}

/// One thing that happened to a stock row — a line of its history.
///
/// The table has been written on every add, move, remove and use since the
/// schema's first migration ("cheap now, impossible to backfill") and read by
/// nothing. This is the read.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ItemHistoryEntry {
    #[ts(type = "number")]
    pub id: u64,
    pub event: ItemEvent,
    /// How much, in the item's own unit — and it means two different things.
    /// For [`ItemEvent::Used`] it is the amount that WENT; for every other
    /// event it is what the row held at the time. That is not an inconsistency
    /// to iron out: a use is the one event that is a change rather than a
    /// state, and recording it as a delta is what makes a consumption rate
    /// computable later. Whoever renders this has to say which it is.
    pub quantity: Option<f64>,
    /// Where the row was when this happened, named rather than numbered — a
    /// history that says "moved" without saying where to is not worth reading.
    /// `None` for an event with no place recorded, or a place since deleted.
    pub location: Option<String>,
    /// When, Unix milliseconds (UTC).
    #[ts(type = "number")]
    pub at: i64,
}

impl ItemEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemEvent::Added => "added",
            ItemEvent::Moved => "moved",
            ItemEvent::Removed => "removed",
            ItemEvent::Restored => "restored",
            ItemEvent::Used => "used",
        }
    }
}

impl fmt::Display for ItemEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ItemEvent {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "added" => Ok(ItemEvent::Added),
            "moved" => Ok(ItemEvent::Moved),
            "removed" => Ok(ItemEvent::Removed),
            "restored" => Ok(ItemEvent::Restored),
            "used" => Ok(ItemEvent::Used),
            other => Err(format!("unknown item event {other:?}")),
        }
    }
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
    /// The same rule as `name_source`, for the same reason. A caller that is not
    /// the item form — sync, a script, the Android app — sends nothing here, and
    /// defaulting it to `Day` on update would silently re-print an invented 30th
    /// as a real one on the single row where that matters most.
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
    /// Absent is the common case and it is not a shrug — it is what every caller
    /// that is not the item form sends, and it deliberately cannot disturb a
    /// choice the person already made.
    #[serde(default)]
    pub name_source: Option<ItemNameSource>,
}

fn default_category() -> ItemCategory {
    ItemCategory::Other
}
