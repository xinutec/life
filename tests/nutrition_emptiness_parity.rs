//! Both fact parsers must agree on what counts as "no panel".
//!
//! `nutrition::RawFacts::parse` reads Open Food Facts; `brandbank::parse` reads
//! Asda's product-page blob. Both end at the same `ProductFacts`, and both once
//! carried their own copy of the emptiness rule (#1449). The rule decides
//! whether a source is recorded as HAVING a panel at all — and `merge_nutrition`
//! picks ONE source's panel whole, so a panel that silently stops existing is
//! not a smaller answer, it is a different source's answer.
//!
//! These tests fail if the two ever diverge again, which moving the code alone
//! would not have caught.

use life::products::brandbank;
use life::products::nutrition::{Nutrition, RawFacts};

fn off(nutriments: serde_json::Value) -> Option<Nutrition> {
    serde_json::from_value::<RawFacts>(serde_json::json!({ "nutriments": nutriments }))
        .expect("RawFacts deserialize")
        .parse()
        .nutrition
}

fn asda(rows: &str) -> Option<Nutrition> {
    let json = format!(
        r#"{{ "calculatedNutritionPer100": "per 100g", "calculatedNutrition": [{rows}] }}"#
    );
    brandbank::parse(&json).expect("parse").nutrition
}

/// Numbers nowhere and a tail nowhere: neither source has a panel.
#[test]
fn a_panel_with_no_numbers_is_no_panel_on_either_side() {
    assert_eq!(off(serde_json::json!({})), None, "OFF");
    assert_eq!(asda(""), None, "Asda");
}

/// One promoted number is enough on both sides. If one parser ever demanded
/// more than the other, this is where a source would vanish.
#[test]
fn a_single_promoted_number_is_a_panel_on_either_side() {
    assert!(
        off(serde_json::json!({ "fat_100g": 31.5 })).is_some(),
        "OFF"
    );
    assert!(
        asda(r#"{ "nameValue": "Fat (g)", "per100": 31.5 }"#).is_some(),
        "Asda"
    );
}

/// Nothing promoted, one value in the tail. `extra` alone must keep the panel
/// alive on both sides — this is the arm most easily lost, because it is the one
/// an `all(Option::is_none)` over the big-8 does not see.
#[test]
fn a_tail_only_panel_survives_on_either_side() {
    assert!(
        off(serde_json::json!({ "sodium_100g": 0.4 })).is_some(),
        "OFF"
    );
    assert!(
        asda(r#"{ "nameValue": "Sodium", "per100": 0.4 }"#).is_some(),
        "Asda"
    );
}

/// A number that arrives as a string is still a number, on both sides.
#[test]
fn a_numeric_string_is_a_number_on_either_side() {
    assert_eq!(
        off(serde_json::json!({ "fat_100g": " 31.5 " }))
            .expect("OFF panel")
            .fat_g,
        Some(31.5)
    );
    assert_eq!(
        asda(r#"{ "nameValue": "Fat (g)", "per100": " 31.5 " }"#)
            .expect("Asda panel")
            .fat_g,
        Some(31.5)
    );
}
