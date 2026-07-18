//! Pure parsing of Open Food Facts product facts into our domain shapes — the
//! `RawFacts::parse` half of the OFF lookup, exercised without any network by
//! deserializing captured-shape OFF product JSON.

use life::products::nutrition::{
    Allergen, DietaryFlag, Nutrition, RawFacts, merge_allergens, merge_dietary, merge_ingredients,
    merge_nutrition,
};

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

// --- merge_dietary: reconciling what several sources claim about one product ---

fn claim(flag: &str, value: &str) -> DietaryFlag {
    DietaryFlag {
        flag: flag.to_string(),
        value: value.to_string(),
    }
}

fn merged(claims: &[(&str, &str)]) -> Vec<(String, String)> {
    merge_dietary(claims.iter().map(|(f, v)| claim(f, v)).collect())
        .into_iter()
        .map(|d| (d.flag, d.value))
        .collect()
}

#[test]
fn a_firm_claim_settles_a_soft_guess() {
    // Asda tags its own product Vegan; OFF's ingredient analysis only guessed.
    assert_eq!(
        merged(&[("vegan", "yes"), ("vegan", "maybe")]),
        [("vegan".to_string(), "yes".to_string())]
    );
}

#[test]
fn sources_that_agree_just_agree() {
    assert_eq!(
        merged(&[("gluten_free", "yes"), ("gluten_free", "yes")]),
        [("gluten_free".to_string(), "yes".to_string())]
    );
    assert_eq!(
        merged(&[("vegan", "no"), ("vegan", "no")]),
        [("vegan".to_string(), "no".to_string())]
    );
}

#[test]
fn a_yes_against_a_no_is_never_reported_as_yes() {
    // The point of the tri-state: telling someone avoiding animal products that
    // this is vegan, while a source says it isn't, is the harmful direction.
    // Say we're unsure and let them read the label.
    assert_eq!(
        merged(&[("vegan", "yes"), ("vegan", "no")]),
        [("vegan".to_string(), "maybe".to_string())]
    );
}

#[test]
fn flags_stay_independent_and_sorted() {
    assert_eq!(
        merged(&[("vegan", "yes"), ("halal", "yes"), ("vegan", "no")]),
        [
            ("halal".to_string(), "yes".to_string()),
            ("vegan".to_string(), "maybe".to_string()),
        ]
    );
}

#[test]
fn a_single_source_passes_straight_through() {
    assert_eq!(
        merged(&[("vegan", "maybe"), ("organic", "yes")]),
        [
            ("organic".to_string(), "yes".to_string()),
            ("vegan".to_string(), "maybe".to_string()),
        ]
    );
    assert!(merged(&[]).is_empty());
}

// --- merge_nutrition / merge_ingredients: pick one source by precedence ---

/// A minimal panel tagged by its salt figure, so tests can tell which source's
/// panel was chosen.
fn panel(salt: f64) -> Nutrition {
    Nutrition {
        basis: "100g".to_string(),
        serving_size: None,
        energy_kj: None,
        energy_kcal: None,
        fat_g: None,
        saturates_g: None,
        carbohydrate_g: None,
        sugars_g: None,
        fibre_g: None,
        protein_g: None,
        salt_g: Some(salt),
        extra: std::collections::BTreeMap::new(),
    }
}

#[test]
fn nutrition_prefers_the_retailer_over_the_crowd() {
    // Asda (Brandbank, manufacturer-grade) outranks OFF (crowd) — whole panel, not
    // a blend of the two.
    let chosen = merge_nutrition(vec![
        ("off".to_string(), panel(1.0)),
        ("asda".to_string(), panel(2.0)),
    ])
    .unwrap();
    assert_eq!(chosen.salt_g, Some(2.0), "Asda's panel wins");
}

#[test]
fn nutrition_of_one_source_is_that_source() {
    assert_eq!(
        merge_nutrition(vec![("off".to_string(), panel(1.0))])
            .unwrap()
            .salt_g,
        Some(1.0)
    );
    assert!(merge_nutrition(vec![]).is_none());
}

#[test]
fn ingredients_prefer_the_retailer_and_skip_empties() {
    assert_eq!(
        merge_ingredients(vec![
            ("off".to_string(), "crowd text".to_string()),
            ("asda".to_string(), "Water, Oats 10%".to_string()),
        ]),
        Some("Water, Oats 10%".to_string()),
    );
    // A source that stored a blank contributes nothing.
    assert_eq!(
        merge_ingredients(vec![
            ("asda".to_string(), "   ".to_string()),
            ("off".to_string(), "crowd text".to_string()),
        ]),
        Some("crowd text".to_string()),
    );
    assert_eq!(merge_ingredients(vec![]), None);
}

// --- merge_allergens: union across sources, most-severe presence wins ---

fn allergen(name: &str, presence: &str) -> Allergen {
    Allergen {
        allergen: name.to_string(),
        presence: presence.to_string(),
    }
}

fn allergens_merged(claims: &[(&str, &str, &str)]) -> Vec<(String, String)> {
    merge_allergens(
        claims
            .iter()
            .map(|(src, name, pres)| (src.to_string(), allergen(name, pres)))
            .collect(),
    )
    .into_iter()
    .map(|a| (a.allergen, a.presence))
    .collect()
}

#[test]
fn allergens_union_every_source_never_dropping_one() {
    // OFF names milk, Asda names soya — a shopper must see both. Absence from one
    // source is not a "free from".
    assert_eq!(
        allergens_merged(&[("off", "milk", "contains"), ("asda", "soya", "contains"),]),
        [
            ("milk".to_string(), "contains".to_string()),
            ("soya".to_string(), "contains".to_string()),
        ]
    );
}

#[test]
fn a_declared_allergen_beats_a_mere_trace() {
    // One source declares milk as an ingredient, another only as a trace: the
    // firmer, more dangerous claim wins.
    assert_eq!(
        allergens_merged(&[("off", "milk", "may_contain"), ("asda", "milk", "contains"),]),
        [("milk".to_string(), "contains".to_string())]
    );
    // Order of sources doesn't change the outcome.
    assert_eq!(
        allergens_merged(&[("asda", "milk", "contains"), ("off", "milk", "may_contain"),]),
        [("milk".to_string(), "contains".to_string())]
    );
}

#[test]
fn allergens_empty_is_empty() {
    assert!(allergens_merged(&[]).is_empty());
}
