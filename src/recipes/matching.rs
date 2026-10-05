//! Pure recipe↔inventory matching: "shopping list = recipe − stock" and
//! "can I cook this now". Kept free of the DB so it is unit-tested directly.

use super::cooking::{LineOutcome, Untouched, plan_ingredients};
use super::types::RecipeIngredient;
use crate::inventory::types::Item;

pub(crate) fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// The stock that counts as this ingredient: anything linked to the same catalog
/// product, plus anything whose name matches case-insensitively.
///
/// A union, not a precedence: an ingredient is a kind ("cumin"), a product one
/// barcode, so a winning link would stop the jar you own counting the day you buy
/// another brand. A link can only find more stock, never less.
pub(crate) fn stock_for<'a>(ingredient: &RecipeIngredient, inventory: &'a [Item]) -> Vec<&'a Item> {
    let want_name = norm(&ingredient.name);
    inventory
        .iter()
        .filter(|it| {
            // `Some(x) == Some(x)` only: two unlinked rows are not "the same
            // product", they are two rows that know nothing about themselves.
            let same_product =
                ingredient.product_id.is_some() && it.product_id == ingredient.product_id;
            same_product || norm(&it.name) == want_name
        })
        .collect()
}

/// The ingredients NOT covered by current inventory — i.e. the shopping list.
///
/// Read off the cooking plan, so "can I cook this" is "would cooking come up
/// short", stock shared between lines included. A line the plan can't measure
/// ("salt", grams against jars) is covered by any match not used down to zero.
pub fn shopping_list(
    ingredients: &[RecipeIngredient],
    inventory: &[Item],
) -> Vec<RecipeIngredient> {
    let lines = plan_ingredients(ingredients, inventory);
    ingredients
        .iter()
        .zip(lines)
        .filter(|(ing, line)| match line.outcome {
            LineOutcome::Took { .. } => false,
            LineOutcome::Short { .. }
            | LineOutcome::Untouched {
                why: Untouched::NoStock,
            } => true,
            LineOutcome::Untouched { .. } => !stock_for(ing, inventory)
                .iter()
                .any(|it| it.quantity.is_none_or(|q| q > 0.0)),
        })
        .map(|(ing, _)| ing.clone())
        .collect()
}

/// True if every ingredient is satisfied by current inventory.
pub fn can_cook(ingredients: &[RecipeIngredient], inventory: &[Item]) -> bool {
    shopping_list(ingredients, inventory).is_empty()
}
