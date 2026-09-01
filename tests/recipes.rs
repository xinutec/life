//! Pure recipe↔inventory matching: shopping list and cook-now.

use life::inventory::types::{ExpiryPrecision, Item, ItemCategory};
use life::products::ids::ProductId;
use life::recipes::matching::{can_cook, shopping_list};
use life::recipes::types::RecipeIngredient;

fn ing(name: &str, qty: Option<f64>, unit: Option<&str>) -> RecipeIngredient {
    RecipeIngredient {
        name: name.into(),
        product_id: None,
        product_name: None,
        quantity: qty,
        unit: unit.map(Into::into),
    }
}

/// The same line, pinned to a catalog product.
fn linked(mut ing: RecipeIngredient, product_id: u64) -> RecipeIngredient {
    ing.product_id = Some(ProductId(product_id));
    ing
}

fn item(name: &str, qty: Option<f64>, unit: Option<&str>) -> Item {
    Item {
        id: 0,
        product_id: None,
        name: name.into(),
        brand: None,
        category: ItemCategory::Food,
        quantity: qty,
        unit: unit.map(Into::into),
        expiry: None,
        expiry_precision: ExpiryPrecision::Day,
        location_id: None,
        barcode: None,
        has_image: false,
    }
}

#[test]
fn presence_match_is_case_and_space_insensitive() {
    let recipe = [ing("  Cumin ", None, None)];
    let stock = [item("cumin", None, None)];
    assert!(can_cook(&recipe, &stock));
    assert!(shopping_list(&recipe, &stock).is_empty());
}

#[test]
fn missing_ingredient_goes_on_the_list() {
    let recipe = [ing("cumin", None, None), ing("salt", None, None)];
    let stock = [item("cumin", None, None)];
    let list = shopping_list(&recipe, &stock);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "salt");
    assert!(!can_cook(&recipe, &stock));
}

#[test]
fn quantity_shortfall_is_not_satisfied() {
    let recipe = [ing("flour", Some(500.0), Some("g"))];
    let stock = [item("flour", Some(200.0), Some("g"))];
    assert!(!can_cook(&recipe, &stock));
}

#[test]
fn quantity_summed_across_stock_rows() {
    let recipe = [ing("flour", Some(500.0), Some("g"))];
    let stock = [
        item("flour", Some(300.0), Some("g")),
        item("flour", Some(300.0), Some("g")),
    ];
    assert!(can_cook(&recipe, &stock));
}

#[test]
fn presence_fallback_when_units_differ() {
    // Need grams, but stock is counted in jars — fall back to presence.
    let recipe = [ing("paprika", Some(20.0), Some("g"))];
    let stock = [item("paprika", Some(1.0), Some("jar"))];
    assert!(can_cook(&recipe, &stock));
}

#[test]
fn a_product_link_matches_stock_the_name_never_would() {
    // THE case this column exists for. The jar in the cupboard is called what
    // the shop calls it; the recipe line is called what a cook calls it. Name
    // matching sent you out to buy the cumin already on the shelf.
    let recipe = [linked(ing("cumin", None, None), 42)];
    let mut jar = item("Bart Ground Cumin 38g", None, None);
    jar.product_id = Some(ProductId(42));
    assert!(can_cook(&recipe, &[jar]));
}

#[test]
fn two_unlinked_rows_are_not_the_same_product() {
    // `None == None` is true and would make every unlinked ingredient match
    // every unlinked item — every recipe cookable from an unrelated cupboard.
    // Not knowing what you are is not an identity.
    let recipe = [ing("cumin", None, None)];
    assert!(!can_cook(&recipe, &[item("paprika", None, None)]));
}

#[test]
fn a_link_never_costs_you_a_name_match() {
    // Pinning the line to one barcode must not un-match the other brand of the
    // same thing — the rules are a union, so linking can only find more stock.
    let recipe = [linked(ing("cumin", None, None), 42)];
    let other_brand = item("cumin", None, None); // unlinked, plain name match
    assert!(can_cook(&recipe, &[other_brand]));
}

#[test]
fn quantities_sum_across_both_kinds_of_match() {
    // 300g under the shop's name (linked) + 300g under the cook's name.
    let recipe = [linked(ing("flour", Some(500.0), Some("g")), 7)];
    let mut linked_stock = item("Allinson Plain White Flour", Some(300.0), Some("g"));
    linked_stock.product_id = Some(ProductId(7));
    let named_stock = item("flour", Some(300.0), Some("g"));
    assert!(can_cook(&recipe, &[linked_stock, named_stock]));
}

#[test]
fn a_link_to_a_different_product_is_not_a_match() {
    let recipe = [linked(ing("cumin", None, None), 42)];
    let mut jar = item("Bart Ground Cumin 38g", None, None);
    jar.product_id = Some(ProductId(99));
    assert!(!can_cook(&recipe, &[jar]));
}
