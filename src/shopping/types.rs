//! Shopping-list types. A shopping item is a "to buy" line; quantity/unit are
//! optional. `done` = ticked off as bought. `category`/`product_id` are the
//! identity the buy→inventory conversion carries onto the created item.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::inventory::types::ItemCategory;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ShoppingItem {
    #[ts(type = "number")]
    pub id: u64,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub barcode: Option<String>,
    pub category: ItemCategory,
    #[ts(type = "number | null")]
    pub product_id: Option<u64>,
    pub done: bool,
}

/// The Buy list is a grocery list first — absent a stated category, assume food.
fn default_category() -> ItemCategory {
    ItemCategory::Food
}

/// Request body for adding something to buy.
#[derive(Debug, Deserialize)]
pub struct NewShoppingItem {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default = "default_category")]
    pub category: ItemCategory,
    #[serde(default)]
    pub product_id: Option<u64>,
}

/// Full update (used for edits and the done toggle).
#[derive(Debug, Deserialize)]
pub struct UpdateShoppingItem {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default = "default_category")]
    pub category: ItemCategory,
    #[serde(default)]
    pub product_id: Option<u64>,
    pub done: bool,
}
