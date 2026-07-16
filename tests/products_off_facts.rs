//! Pure parsing of Open Food Facts product facts into our domain shapes — the
//! `RawFacts::parse` half of the OFF lookup, exercised without any network by
//! deserializing captured-shape OFF product JSON.

use life::products::nutrition::RawFacts;

fn parse(value: serde_json::Value) -> life::products::nutrition::ProductFacts {
    serde_json::from_value::<RawFacts>(value)
        .expect("RawFacts deserialize")
        .parse()
}

#[test]
fn full_panel_ingredients_allergens_and_flags() {
    let facts = parse(serde_json::json!({
        "nutrition_data_per": "100g",
        "serving_size": "40 g",
        "ingredients_text": "Wholegrain oats (95%), sugar",
        "ingredients_text_en": "Wholegrain oats (95%), sugar",
        "allergens_tags": ["en:gluten", "en:oats"],
        "traces_tags": ["en:nuts", "en:milk"],
        "ingredients_analysis_tags": ["en:palm-oil-free", "en:vegan", "en:vegetarian"],
        "labels_tags": ["en:organic", "en:gluten-free"],
        "nutriments": {
            "energy-kj_100g": 1500,
            "energy-kcal_100g": 356,
            "fat_100g": 6.5,
            "saturated-fat_100g": 1.2,
            "carbohydrates_100g": 60,
            "sugars_100g": 1.0,
            "fiber_100g": 10,
            "proteins_100g": 11,
            "salt_100g": 0.1,
            // Tail (kept in `extra`) and noise that must be dropped:
            "sodium_100g": 0.04,
            "energy_100g": 1500,     // promoted-key alias → dropped
            "fat_serving": 2.6       // not a _100g key → dropped
        }
    }));

    let n = facts.nutrition.expect("a panel");
    assert_eq!(n.basis, "100g");
    assert_eq!(n.serving_size.as_deref(), Some("40 g"));
    assert_eq!(n.energy_kj, Some(1500.0));
    assert_eq!(n.energy_kcal, Some(356.0));
    assert_eq!(n.fat_g, Some(6.5));
    assert_eq!(n.saturates_g, Some(1.2));
    assert_eq!(n.carbohydrate_g, Some(60.0));
    assert_eq!(n.sugars_g, Some(1.0));
    assert_eq!(n.fibre_g, Some(10.0));
    assert_eq!(n.protein_g, Some(11.0));
    assert_eq!(n.salt_g, Some(0.1));
    // Only the non-promoted per-100 tail survives, suffix stripped.
    assert_eq!(n.extra.len(), 1);
    assert_eq!(n.extra.get("sodium"), Some(&0.04));

    assert_eq!(
        facts.ingredients.as_deref(),
        Some("Wholegrain oats (95%), sugar")
    );

    // Allergens: contains from allergens_tags, may_contain from traces, sorted.
    let allergens: Vec<(&str, &str)> = facts
        .allergens
        .iter()
        .map(|a| (a.allergen.as_str(), a.presence.as_str()))
        .collect();
    assert_eq!(
        allergens,
        vec![
            ("gluten", "contains"),
            ("milk", "may_contain"),
            ("nuts", "may_contain"),
            ("oats", "contains"),
        ]
    );

    // Dietary: analysis + label claims, deduped, sorted by flag.
    let dietary: Vec<(&str, &str)> = facts
        .dietary
        .iter()
        .map(|d| (d.flag.as_str(), d.value.as_str()))
        .collect();
    assert_eq!(
        dietary,
        vec![
            ("gluten_free", "yes"),
            ("organic", "yes"),
            ("palm_oil_free", "yes"),
            ("vegan", "yes"),
            ("vegetarian", "yes"),
        ]
    );
}

#[test]
fn analysis_tristate_and_label_overrides_softer_guess() {
    let facts = parse(serde_json::json!({
        "ingredients_analysis_tags": [
            "en:non-vegan",
            "en:maybe-vegetarian",
            "en:palm-oil"
        ],
        // A firm label claim must beat OFF's "maybe" analysis for the same flag.
        "labels_tags": ["en:vegetarian"]
    }));
    let dietary: Vec<(&str, &str)> = facts
        .dietary
        .iter()
        .map(|d| (d.flag.as_str(), d.value.as_str()))
        .collect();
    assert_eq!(
        dietary,
        vec![
            ("palm_oil_free", "no"),
            ("vegan", "no"),
            ("vegetarian", "yes"), // label 'yes' overrode 'maybe'
        ]
    );
    // No nutriments at all → no panel.
    assert!(facts.nutrition.is_none());
}

#[test]
fn liquid_basis_and_prefixless_ingredients_fallback() {
    let facts = parse(serde_json::json!({
        "nutrition_data_per": "100ml",
        // No localized English copy: fall back to the generic ingredients_text.
        "ingredients_text": "Water, apple juice concentrate",
        "nutriments": { "carbohydrates_100g": 10.5 }
    }));
    let n = facts.nutrition.expect("a panel");
    assert_eq!(n.basis, "100ml");
    assert_eq!(n.carbohydrate_g, Some(10.5));
    assert!(n.extra.is_empty());
    assert_eq!(
        facts.ingredients.as_deref(),
        Some("Water, apple juice concentrate")
    );
    assert!(facts.allergens.is_empty());
    assert!(facts.dietary.is_empty());
}

#[test]
fn numeric_strings_and_missing_fields_are_tolerated() {
    // OFF sometimes reports nutriment values as strings; treat them as numbers.
    let facts = parse(serde_json::json!({
        "nutriments": { "salt_100g": "1.25", "proteins_100g": "not a number" }
    }));
    let n = facts
        .nutrition
        .expect("a panel from the one parseable value");
    assert_eq!(n.salt_g, Some(1.25));
    assert_eq!(n.protein_g, None); // unparseable → absent, not zero
    assert!(n.extra.is_empty());
}

#[test]
fn a_product_with_no_facts_parses_to_nothing() {
    let facts = parse(serde_json::json!({}));
    assert!(facts.nutrition.is_none());
    assert!(facts.ingredients.is_none());
    assert!(facts.allergens.is_empty());
    assert!(facts.dietary.is_empty());
}
