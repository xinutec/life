//! Recipes.

use crate::products::ids::ProductId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One ingredient, matched to stock by product and by name ([[super::matching]]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeIngredient {
    pub name: String,
    /// Optional: an ingredient is a kind of thing, rarely one barcode.
    #[serde(default)]
    pub product_id: Option<ProductId>,
    /// Joined on read and never stored; `serde(default)` lets a client PUT back
    /// what it read.
    #[serde(default)]
    pub product_name: Option<String>,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
}

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

#[derive(Debug, Deserialize)]
pub struct NewRecipe {
    pub name: String,
    pub instructions: Option<String>,
    pub servings: Option<i32>,
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredient>,
}
