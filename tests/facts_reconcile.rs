//! F4 — reconciling the source-picked facts (nutrition, ingredients). The pure
//! rules (which disagreements surface, how a pick settles them, how the merge
//! honours a pick) are tested without a database; the persistence round-trip
//! (pick a source → it drives the merge and settles the divergence) runs only
//! when LIFE_TEST_DATABASE_URL is set.

use std::collections::{BTreeMap, HashMap};

use life::db;
use life::products::nutrition::{Allergen, Nutrition, ProductFacts};
use life::products::repo::{self, FactSourceMap};
use life::products::types::SourceFacts;

/// A minimal panel carrying just an energy figure — enough to differ.
fn panel(kcal: f64) -> Nutrition {
    Nutrition {
        basis: "100ml".into(),
        serving_size: None,
        energy_kj: None,
        energy_kcal: Some(kcal),
        fat_g: None,
        saturates_g: None,
        carbohydrate_g: None,
        sugars_g: None,
        fibre_g: None,
        protein_g: None,
        salt_g: None,
        extra: BTreeMap::new(),
    }
}

fn source_facts(
    source: &str,
    nutrition: Option<Nutrition>,
    ingredients: Option<&str>,
) -> SourceFacts {
    SourceFacts {
        source: source.into(),
        facts: ProductFacts {
            nutrition,
            ingredients: ingredients.map(str::to_string),
            allergens: Vec::new(),
            dietary: Vec::new(),
        },
    }
}

// --- fact_divergences: disagreement surfaces, agreement doesn't, a pick settles ---

#[test]
fn differing_nutrition_surfaces_a_divergence() {
    // Precedence order (retailer before crowd), as facts_by_source returns it.
    let by_source = vec![
        source_facts("asda", Some(panel(61.0)), None),
        source_facts("off", Some(panel(59.0)), None),
    ];
    let divs = repo::fact_divergences(&by_source, &FactSourceMap::new());
    let nut = divs
        .iter()
        .find(|d| d.field == "nutrition")
        .expect("a nutrition divergence");
    // The current pick is the precedence winner (asda), and off is offered.
    assert!(nut.current.as_deref().unwrap().contains("61 kcal"));
    assert_eq!(nut.candidates.len(), 1);
    assert_eq!(nut.candidates[0].source, "off");
    assert!(nut.candidates[0].value.contains("59 kcal"));
}

#[test]
fn agreeing_facts_have_no_divergence() {
    let by_source = vec![
        source_facts("asda", Some(panel(61.0)), Some("Water, Oats")),
        source_facts("off", Some(panel(61.0)), Some("Water, Oats")),
    ];
    assert!(repo::fact_divergences(&by_source, &FactSourceMap::new()).is_empty());
}

#[test]
fn a_recorded_pick_settles_the_divergence() {
    let by_source = vec![
        source_facts("asda", Some(panel(61.0)), None),
        source_facts("off", Some(panel(59.0)), None),
    ];
    let prefs: FactSourceMap = HashMap::from([("nutrition".to_string(), "off".to_string())]);
    assert!(
        repo::fact_divergences(&by_source, &prefs).is_empty(),
        "once a source is picked the divergence is quiet"
    );
}

#[test]
fn a_single_source_is_not_a_divergence() {
    let by_source = vec![source_facts("asda", Some(panel(61.0)), Some("Water, Oats"))];
    assert!(repo::fact_divergences(&by_source, &FactSourceMap::new()).is_empty());
}

// --- merge_facts: pick honoured, else precedence; allergens always union ---

#[test]
fn merge_prefers_the_picked_source_else_precedence() {
    let mut asda = source_facts("asda", Some(panel(61.0)), None);
    asda.facts.allergens = vec![Allergen {
        allergen: "oats".into(),
        presence: "contains".into(),
    }];
    let mut off = source_facts("off", Some(panel(59.0)), None);
    off.facts.allergens = vec![Allergen {
        allergen: "soya".into(),
        presence: "may_contain".into(),
    }];
    let by_source = vec![asda, off];

    // No pick → precedence winner (asda, the retailer).
    let merged = repo::merge_facts(&by_source, &FactSourceMap::new());
    assert_eq!(merged.nutrition.unwrap().energy_kcal, Some(61.0));

    // Pick OFF → its panel shows instead.
    let prefs: FactSourceMap = HashMap::from([("nutrition".to_string(), "off".to_string())]);
    let merged = repo::merge_facts(&by_source, &prefs);
    assert_eq!(merged.nutrition.unwrap().energy_kcal, Some(59.0));
    // Allergens are unioned regardless of any pick — safety never gets narrowed.
    assert_eq!(merged.allergens.len(), 2, "both sources' allergens kept");
}

// --- persistence: reconcile records the pick, which drives the merge + settles ---

#[tokio::test]
async fn reconcile_records_a_fact_source_and_settles() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping facts reconcile DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000902";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        "off",
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oat Drink"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Two sources disagree on both nutrition and ingredients.
    repo::upsert_nutrition(&pool, product.id, &panel(61.0), "asda")
        .await
        .unwrap();
    repo::upsert_nutrition(&pool, product.id, &panel(59.0), "off")
        .await
        .unwrap();
    repo::set_ingredients(&pool, product.id, "Water, Oats 10%", "asda")
        .await
        .unwrap();
    repo::set_ingredients(&pool, product.id, "Oat base (water, oats)", "off")
        .await
        .unwrap();

    // Both surface as divergences; the default merge is the retailer's (asda).
    let by_source = repo::facts_by_source(&pool, product.id).await.unwrap();
    let prefs = repo::fact_source_prefs(&pool, product.id).await.unwrap();
    let divs = repo::fact_divergences(&by_source, &prefs);
    assert!(divs.iter().any(|d| d.field == "nutrition"));
    assert!(divs.iter().any(|d| d.field == "ingredients"));
    assert_eq!(
        repo::facts_for(&pool, product.id)
            .await
            .unwrap()
            .nutrition
            .unwrap()
            .energy_kcal,
        Some(61.0),
        "default pick is the retailer"
    );

    // Pick OFF for nutrition; keep the current (asda) for ingredients.
    repo::reconcile(
        &pool,
        product.id,
        &[
            repo::FieldChoice {
                field: "nutrition".into(),
                choice: "off".into(),
                value: None,
            },
            repo::FieldChoice {
                field: "ingredients".into(),
                choice: repo::KEEP.into(),
                value: None,
            },
        ],
    )
    .await
    .unwrap();

    // The pick drives the merge, and both divergences are now settled.
    let prefs = repo::fact_source_prefs(&pool, product.id).await.unwrap();
    assert_eq!(prefs.get("nutrition").map(String::as_str), Some("off"));
    assert_eq!(
        prefs.get("ingredients").map(String::as_str),
        Some("asda"),
        "keep records the current precedence winner"
    );
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(facts.nutrition.unwrap().energy_kcal, Some(59.0));
    assert_eq!(facts.ingredients.as_deref(), Some("Water, Oats 10%"));
    let by_source = repo::facts_by_source(&pool, product.id).await.unwrap();
    assert!(
        repo::fact_divergences(&by_source, &prefs).is_empty(),
        "picks settle both divergences"
    );

    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();
}
