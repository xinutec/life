//! Recipe domain types.

use crate::products::ids::ProductId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One ingredient line of a recipe. Matched to inventory by `product_id` when
/// the line names a catalog product and the stock does too, and by `name`
/// otherwise — the two are alternatives, not a precedence: see
/// [[super::matching]] for why a link can only ever find MORE stock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeIngredient {
    pub name: String,
    /// The catalog product this line names, if it names one. Optional forever:
    /// an ingredient is a kind of thing and most lines will never be worth
    /// pinning to one barcode.
    #[serde(default)]
    pub product_id: Option<ProductId>,
    /// The linked product's canonical name, joined on read; never stored, so
    /// whatever a client sends is ignored. `serde(default)` lets a client PUT
    /// back a recipe it just read (`skip_deserializing` makes ts-rs warn).
    #[serde(default)]
    pub product_name: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
}

/// A recipe as returned by the API, ingredients nested.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Recipe {
    #[ts(type = "number")]
    pub id: u64,
    pub name: String,
    pub instructions: Option<String>,
    pub servings: Option<i32>,
    pub ingredients: Vec<RecipeIngredient>,
}

/// Request body for creating a recipe.
#[derive(Debug, Deserialize)]
pub struct NewRecipe {
    pub name: String,
    pub instructions: Option<String>,
    pub servings: Option<i32>,
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,
}
