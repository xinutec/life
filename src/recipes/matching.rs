//! Pure recipe↔inventory matching: "shopping list = recipe − stock" and
//! "can I cook this now". Kept free of the DB so it is unit-tested directly.

use super::types::RecipeIngredient;
use crate::inventory::types::Item;

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// The stock that counts as this ingredient: anything linked to the same
/// catalog product, plus anything whose name matches case-insensitively.
///
/// The two rules are ALTERNATIVES, deliberately — not a precedence with the
/// link winning. An ingredient is a kind of thing ("cumin") and a product is one
/// barcode ("Bart Ground Cumin 38g"), so treating a link as authoritative would
/// make the jar you actually own stop counting the day you buy another brand.
/// Union means a link can only ever find MORE stock than before, never less,
/// and an unlinked line behaves exactly as it did before links existed.
fn stock_for<'a>(ingredient: &RecipeIngredient, inventory: &'a [Item]) -> Vec<&'a Item> {
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

/// Whether the inventory satisfies one ingredient. When the ingredient gives a
/// quantity AND some stock of the same unit also gives quantities, the summed
/// stock must meet the amount; otherwise presence of a match is enough.
fn is_satisfied(ingredient: &RecipeIngredient, inventory: &[Item]) -> bool {
    let matches = stock_for(ingredient, inventory);
    if matches.is_empty() {
        return false;
    }
    match (ingredient.quantity, ingredient.unit.as_deref()) {
        (Some(needed), Some(unit)) => {
            let want_unit = norm(unit);
            let available: f64 = matches
                .iter()
                .filter(|it| it.unit.as_deref().map(norm).as_deref() == Some(want_unit.as_str()))
                .filter_map(|it| it.quantity)
                .sum();
            // No comparable-unit quantities on hand → fall back to presence.
            if available == 0.0 {
                true
            } else {
                available >= needed
            }
        }
        _ => true,
    }
}

/// The ingredients NOT covered by current inventory — i.e. the shopping list.
pub fn shopping_list(
    ingredients: &[RecipeIngredient],
    inventory: &[Item],
) -> Vec<RecipeIngredient> {
    ingredients
        .iter()
        .filter(|ing| !is_satisfied(ing, inventory))
        .cloned()
        .collect()
}

/// True if every ingredient is satisfied by current inventory.
pub fn can_cook(ingredients: &[RecipeIngredient], inventory: &[Item]) -> bool {
    shopping_list(ingredients, inventory).is_empty()
}
