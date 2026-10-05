//! Locations and items.

use crate::products::ids::ProductId;
use crate::str_enum;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
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
    /// Whose name an item carries, stated by the form: the server cannot see
    /// whether the name field was touched.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ItemNameSource: "item name source" {
    /// Typed deliberately: outranks the catalogue and keeps through corrections.
        User => "user",
    /// The catalogue's, following the product's corrections.
        Product => "product",
    }
}

str_enum! {
    /// How much of `expiry` was printed. MM/YYYY is stored as the month's last day,
    /// and nothing may count down to a day that was never printed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ExpiryPrecision: "expiry precision" {
        Day => "day",
        Month => "month",
    }
}

str_enum! {
    /// Split by where it lives (`Cookware` and `Tableware` are different
    /// cupboards). Closed: a new category is added here.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ItemCategory: "item category" {
        Food => "food",
        Medication => "medication",
        Cookware => "cookware",
        Tableware => "tableware",
        Clothing => "clothing",
        Appliance => "appliance",
        Cleaning => "cleaning",
        Tool => "tool",
        Document => "document",
        Other => "other",
    }
}
crate::row_id! {
    /// `locations.id`.
    LocationId
}

crate::row_id! {
    /// `items.id`.
    ItemId
}

/// Exported to TypeScript as `Loc`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, rename = "Loc")]
pub struct Location {
    #[ts(type = "number")]
    pub id: LocationId,
    pub kind: LocationKind,
    pub name: String,
    #[ts(type = "number | null")]
    pub parent_id: Option<LocationId>,
    pub sort_order: i32,
    #[ts(type = "unknown | null")]
    pub position: Option<serde_json::Value>,
}

/// `name`, `brand`, `barcode` and `has_image` come from the linked product when
/// there is one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Item {
    #[ts(type = "number")]
    pub id: ItemId,
    pub product_id: Option<ProductId>,
    pub name: String,
    pub brand: Option<String>,
    pub category: ItemCategory,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub expiry: Option<NaiveDate>,
    /// Meaningless without an `expiry`.
    pub expiry_precision: ExpiryPrecision,
    #[ts(type = "number | null")]
    pub location_id: Option<LocationId>,
    pub barcode: Option<String>,
    pub has_image: bool,
}

str_enum! {
    /// What happened to a stock row (`item_history`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export)]
    pub enum ItemEvent: "item event" {
        Added => "added",
        Moved => "moved",
        Removed => "removed",
        Restored => "restored",
    /// The only event with a delta: `quantity` is how much went.
        Used => "used",
    /// Judged low by putting it on the Buy list; no quantity, as the signal is the
    /// interval between them.
        Low => "low",
    }
}

/// The history dialog: events, and beside them the purchases, which no event
/// records.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ItemHistory {
    pub entries: Vec<ItemHistoryEntry>,
    pub purchases: Vec<crate::purchases::types::Purchase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct ItemHistoryEntry {
    #[ts(type = "number")]
    pub id: u64,
    pub event: ItemEvent,
    /// For [`ItemEvent::Used`] the amount that went; otherwise what the row held.
    pub quantity: Option<f64>,
    /// `None` if unrecorded or since deleted.
    pub location: Option<String>,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub at: DateTime<Utc>,
}

/// `unit` must agree with the row's own ([[super::consume]]), so "200 g" against a
/// jar is refused rather than taken from 1. Absent means unitless.
#[derive(Debug, Deserialize)]
pub struct UseItem {
    pub quantity: f64,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewLocation {
    pub kind: LocationKind,
    pub name: String,
    pub parent_id: Option<LocationId>,
    #[serde(default)]
    pub sort_order: i32,
    pub position: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct NewItem {
    pub name: String,
    #[serde(default = "default_category")]
    pub category: ItemCategory,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub expiry: Option<NaiveDate>,
    /// Absent is "no statement": a new item gets `Day`, an update keeps its own.
    #[serde(default)]
    pub expiry_precision: Option<ExpiryPrecision>,
    pub location_id: Option<LocationId>,
    #[serde(default)]
    pub barcode: Option<String>,
    /// The only way to link a barcodeless product.
    #[serde(default)]
    pub product_id: Option<ProductId>,
    /// Absent is "no statement": a new item gets `Product`, an update keeps its own.
    #[serde(default)]
    pub name_source: Option<ItemNameSource>,
}

fn default_category() -> ItemCategory {
    ItemCategory::Other
}
