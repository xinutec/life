//! Recipe against inventory: what to buy, and can I cook it. Pure.

use super::cooking::{LineOutcome, Untouched, plan_ingredients};
use super::types::RecipeIngredient;
use crate::inventory::types::Item;

pub(crate) fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Stock linked to the same product, plus stock with the same name. A union: a
/// link can only find more, so the jar you own still counts after you buy another
/// brand.
pub(crate) fn stock_for<'a>(ingredient: &RecipeIngredient, inventory: &'a [Item]) -> Vec<&'a Item> {
    let want_name = norm(&ingredient.name);
    inventory
        .iter()
        .filter(|it| {
            // Two unlinked rows are not "the same product".
            let same_product =
                ingredient.product_id.is_some() && it.product_id == ingredient.product_id;
            same_product || norm(&it.name) == want_name
        })
        .collect()
}

/// Read off the cooking plan, so "can I cook this" is "would cooking come up
/// short". A line the plan cannot measure is covered by any match not at zero.
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

pub fn can_cook(ingredients: &[RecipeIngredient], inventory: &[Item]) -> bool {
    shopping_list(ingredients, inventory).is_empty()
}
