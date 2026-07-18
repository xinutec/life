//! Parsing Asda's Brandbank product-content blob into our domain facts, against a
//! real capture (the "Oalty" oat drink page, tests/fixtures/asda_brandbank_oalty.json —
//! projected to the fields the parser consumes, real values, real structure).

use life::products::brandbank;

const OALTY: &str = include_str!("fixtures/asda_brandbank_oalty.json");

#[test]
fn parses_the_real_oalty_blob() {
    let facts = brandbank::parse(OALTY).expect("parse");

    // Nutrition: liquid basis, big-8 mapped, "of which" rows kept distinct.
    let n = facts.nutrition.expect("a panel");
    assert_eq!(n.basis, "100ml");
    assert_eq!(n.energy_kj, Some(257.0));
    assert_eq!(n.energy_kcal, Some(61.0));
    assert_eq!(n.fat_g, Some(3.0));
    assert_eq!(n.saturates_g, Some(0.3));
    assert_eq!(n.carbohydrate_g, Some(7.1));
    assert_eq!(n.sugars_g, Some(3.4));
    assert_eq!(n.fibre_g, Some(0.8));
    assert_eq!(n.protein_g, Some(1.1));
    assert_eq!(n.salt_g, Some(0.1));
    // Vitamins/minerals fall into the tail, not the panel.
    assert_eq!(n.extra.get("calcium"), Some(&120.0));
    assert_eq!(n.extra.get("vitamin-d"), Some(&1.1));
    assert_eq!(n.extra.get("vitamin-b12"), Some(&0.38));
    assert_eq!(n.extra.get("iodine"), Some(&22.5));
    // The big-8 are NOT duplicated into the tail.
    assert!(!n.extra.contains_key("fat"));
    assert!(!n.extra.contains_key("energy"));

    // Ingredients: the component list, joined.
    assert_eq!(
        facts.ingredients.as_deref(),
        Some(
            "Water, Oats 10%, Rapeseed Oil, Acidity Regulator (Dipotassium Phosphate), \
             Minerals (Calcium Carbonate, Potassium Iodide), Salt, \
             Vitamins (D2, Riboflavin and B12)"
        )
    );

    // Allergens: only positive presences. Oats "Contains" is kept; Milk and Soya
    // are "Free From" — negatives, not allergens.
    assert_eq!(
        facts
            .allergens
            .iter()
            .map(|a| (a.allergen.as_str(), a.presence.as_str()))
            .collect::<Vec<_>>(),
        vec![("oats", "contains")]
    );

    // Dietary: only asserted (true) flags become 'yes'. halal/kosher/noGluten are
    // false → absent (not a firm 'no'). The free-from booleans map to our slugs.
    let dietary: Vec<(&str, &str)> = facts
        .dietary
        .iter()
        .map(|d| (d.flag.as_str(), d.value.as_str()))
        .collect();
    assert_eq!(
        dietary,
        vec![
            ("egg_free", "yes"),
            ("lactose_free", "yes"),
            ("milk_free", "yes"),
            ("nut_free", "yes"),
            ("soya_free", "yes"),
            ("vegan", "yes"),
            ("vegetarian", "yes"),
        ]
    );
    assert!(
        !facts.dietary.iter().any(|d| d.flag == "gluten_free"),
        "noGluten:false must not assert gluten_free — oats contain gluten"
    );
}

#[test]
fn a_may_contain_advice_is_a_trace_not_an_ingredient() {
    let json = r#"{
        "allergyAdvice": [
            { "lookupValue": "Contains", "nameValue": "Milk" },
            { "lookupValue": "May Contain", "nameValue": "Nuts" },
            { "lookupValue": "Free From", "nameValue": "Soya" }
        ]
    }"#;
    let facts = brandbank::parse(json).expect("parse");
    assert_eq!(
        facts
            .allergens
            .iter()
            .map(|a| (a.allergen.as_str(), a.presence.as_str()))
            .collect::<Vec<_>>(),
        vec![("milk", "contains"), ("nuts", "may_contain")],
        "Free From is dropped; May Contain is a trace"
    );
}

#[test]
fn a_solid_basis_and_numeric_strings_are_tolerated() {
    let json = r#"{
        "calculatedNutritionPer100": "per 100g",
        "calculatedNutrition": [
            { "nameValue": "Energy (kcal)", "per100": "534" },
            { "nameValue": "Fat (g)", "per100": 31.5 }
        ]
    }"#;
    let n = brandbank::parse(json)
        .expect("parse")
        .nutrition
        .expect("panel");
    assert_eq!(n.basis, "100g");
    assert_eq!(n.energy_kcal, Some(534.0), "numeric string coerced");
    assert_eq!(n.fat_g, Some(31.5));
}

#[test]
fn an_empty_blob_yields_no_facts() {
    let facts = brandbank::parse("{}").expect("parse");
    assert!(facts.nutrition.is_none());
    assert!(facts.ingredients.is_none());
    assert!(facts.allergens.is_empty());
    assert!(facts.dietary.is_empty());
}

#[test]
fn malformed_json_is_an_error_not_a_panic() {
    assert!(brandbank::parse("not json").is_err());
}
