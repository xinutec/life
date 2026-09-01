//! Taking an amount out of a stock row — the pure rule behind "I used 200g of
//! flour". No DB: these pin the arithmetic and, more importantly, the two cases
//! where the honest answer is to take nothing at all.

use life::inventory::consume::{Taken, take};
use life::inventory::types::{ExpiryPrecision, Item, ItemCategory};

fn stock(quantity: Option<f64>, unit: Option<&str>) -> Item {
    Item {
        id: 1,
        product_id: None,
        name: "Flour".into(),
        brand: None,
        category: ItemCategory::Food,
        quantity,
        unit: unit.map(Into::into),
        expiry: None,
        expiry_precision: ExpiryPrecision::Day,
        location_id: None,
        barcode: None,
        has_image: false,
    }
}

#[test]
fn taking_some_leaves_the_rest() {
    assert_eq!(
        take(&stock(Some(950.0), Some("g")), 200.0, Some("g")),
        Taken::Left(750.0)
    );
}

#[test]
fn taking_exactly_what_is_there_empties_it_without_a_shortfall() {
    // `Left(0.0)`, not `Emptied` — using the last of something is not the same
    // event as running out mid-recipe, and the UI says different things.
    assert_eq!(
        take(&stock(Some(200.0), Some("g")), 200.0, Some("g")),
        Taken::Left(0.0)
    );
}

#[test]
fn a_row_that_reaches_zero_reports_a_zero_not_a_removal() {
    // "We have none" is knowledge — it is what makes the thing worth rebuying —
    // so the rule has no way to say "delete this row", only to say zero. That
    // the row really does survive in the database is pinned separately, in
    // tests/inventory_use_db.rs.
    assert_eq!(
        take(&stock(Some(1.0), Some("jar")), 1.0, Some("jar")),
        Taken::Left(0.0)
    );
}

#[test]
fn using_more_than_you_had_empties_it_and_says_by_how_much() {
    // The food really was used; the cupboard's number was just behind. Refusing
    // would leave a count we know to be wrong.
    assert_eq!(
        take(&stock(Some(150.0), Some("g")), 200.0, Some("g")),
        Taken::Emptied { short: 50.0 }
    );
}

#[test]
fn a_different_unit_takes_nothing() {
    // 200g out of "1 jar" is not 199 of anything, and guessing how many grams
    // are in a jar would put a number in the cupboard nobody ever measured.
    assert_eq!(
        take(&stock(Some(1.0), Some("jar")), 200.0, Some("g")),
        Taken::UnitMismatch
    );
    assert_eq!(
        take(&stock(Some(1.0), Some("jar")), 1.0, None),
        Taken::UnitMismatch
    );
    assert_eq!(
        take(&stock(Some(500.0), None), 200.0, Some("g")),
        Taken::UnitMismatch
    );
}

#[test]
fn kilograms_are_not_grams() {
    // Deliberately NOT converted. A conversion table is a real feature with real
    // edge cases (does `oz` mean weight or fluid?), and half of one would be
    // worse than none.
    assert_eq!(
        take(&stock(Some(1.0), Some("kg")), 200.0, Some("g")),
        Taken::UnitMismatch
    );
}

#[test]
fn units_agree_past_case_and_padding() {
    // Both sides are typed by the same person about the same kind of thing, so
    // "G" and " g " are the same unit; nothing cleverer is implied.
    assert_eq!(
        take(&stock(Some(950.0), Some("G")), 200.0, Some(" g ")),
        Taken::Left(750.0)
    );
}

#[test]
fn two_unitless_rows_agree_with_each_other() {
    // "2 eggs" out of "6 eggs" — countable things carry no unit at all.
    assert_eq!(take(&stock(Some(6.0), None), 2.0, None), Taken::Left(4.0));
}

#[test]
fn stock_with_no_quantity_has_nothing_to_take() {
    // Most stock is like this ("a jar of cumin"), and it simply isn't the kind
    // of thing this can track — which is a fact about the row, not an error.
    assert_eq!(
        take(&stock(None, Some("g")), 200.0, Some("g")),
        Taken::Untracked
    );
}

#[test]
fn a_nonsense_amount_takes_nothing() {
    // NaN loses every comparison, so a naive `> 0.0` guard would wave it
    // through and write NaN into the cupboard, where it would poison every
    // later subtraction silently.
    assert_eq!(
        take(&stock(Some(950.0), Some("g")), f64::NAN, Some("g")),
        Taken::Left(950.0)
    );
    assert_eq!(
        take(&stock(Some(950.0), Some("g")), f64::INFINITY, Some("g")),
        Taken::Left(950.0)
    );
}

#[test]
fn using_none_of_it_changes_nothing() {
    // Not a back door for adding stock: a non-positive amount is a no-op.
    assert_eq!(
        take(&stock(Some(950.0), Some("g")), 0.0, Some("g")),
        Taken::Left(950.0)
    );
    assert_eq!(
        take(&stock(Some(950.0), Some("g")), -50.0, Some("g")),
        Taken::Left(950.0)
    );
}
