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

/// Item category. Generic from day one — food is just the first skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ItemCategory {
    Food,
    Medication,
    Tool,
    Document,
    Other,
}

impl fmt::Display for ItemCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ItemCategory::Food => "food",
            ItemCategory::Medication => "medication",
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
    pub location_id: Option<u64>,
    #[serde(default)]
    pub barcode: Option<String>,
    /// Explicit catalog link. Takes precedence over barcode-based resolution and
    /// is the only way to link a barcodeless shop product (Waitrose etc.).
    #[serde(default)]
    pub product_id: Option<ProductId>,
}

fn default_category() -> ItemCategory {
    ItemCategory::Other
}
