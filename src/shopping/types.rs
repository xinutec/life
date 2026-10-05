//! Buy-list rows.

use serde::Serialize;
use ts_rs::TS;

use crate::inventory::types::ItemCategory;
use crate::products::ids::ProductId;

#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct ShoppingItem {
    #[ts(type = "number")]
    pub id: u64,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub barcode: Option<String>,
    pub category: ItemCategory,
    pub product_id: Option<ProductId>,
    pub done: bool,
}
