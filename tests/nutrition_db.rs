//! Product facts against a real MariaDB: storing a product's nutrition panel,
//! ingredients, allergens, and dietary flags, then reading them back — plus the
//! whole-product REPLACE semantics of a re-lookup. Runs only when
//! LIFE_TEST_DATABASE_URL is set.

use std::collections::BTreeMap;

use life::db;
use life::products::nutrition::{Allergen, Claim, DietaryFlag, Nutrition, Presence, ProductFacts};
use life::products::source::Source;
use life::products::{brandbank, repo};

const OALTY_BRANDBANK: &str = include_str!("fixtures/asda_brandbank_oalty.json");

fn nutrition() -> Nutrition {
    Nutrition {
        basis: "100g".into(),
        serving_size: Some("40g".into()),
        energy_kj: Some(1500.0),
        energy_kcal: Some(356.0),
        fat_g: Some(6.5),
        saturates_g: Some(1.2),
        carbohydrate_g: Some(60.0),
        sugars_g: Some(1.0),
        fibre_g: Some(10.0),
        protein_g: Some(11.0),
        salt_g: Some(0.1),
        extra: BTreeMap::from([("sodium".into(), 0.04)]),
    }
}

#[tokio::test]
async fn store_and_read_facts_then_replace_on_relookup() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping nutrition DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000456";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();

    let product = repo::upsert_external(
        &pool,
        Source::Off,
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Porridge Oats"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // A product with no facts yet reads back empty.
    let empty = repo::facts_for(&pool, product.id).await.unwrap();
    assert!(empty.nutrition.is_none());
    assert!(empty.ingredients.is_none());
    assert!(empty.allergens.is_empty());
    assert!(empty.dietary.is_empty());

    let facts = ProductFacts {
        nutrition: Some(nutrition()),
        ingredients: Some("Wholegrain oats (95%), sugar".into()),
        allergens: vec![
            Allergen {
                allergen: "gluten".into(),
                presence: Presence::Contains,
            },
            Allergen {
                allergen: "nuts".into(),
                presence: Presence::MayContain,
            },
        ],
        dietary: vec![
            DietaryFlag {
                flag: "vegan".into(),
                value: Claim::Yes,
            },
            DietaryFlag {
                flag: "palm_oil_free".into(),
                value: Claim::Maybe,
            },
        ],
    };
    repo::store_facts(&pool, product.id, &facts, Source::Off)
        .await
        .unwrap();

    let read = repo::facts_for(&pool, product.id).await.unwrap();
    // Nutrition round-trips, extra JSON included.
    let n = read.nutrition.expect("nutrition");
    assert_eq!(n.basis, "100g");
    assert_eq!(n.serving_size.as_deref(), Some("40g"));
    assert_eq!(n.energy_kcal, Some(356.0));
    assert_eq!(n.salt_g, Some(0.1));
    assert_eq!(n.extra.get("sodium"), Some(&0.04));
    assert_eq!(
        read.ingredients.as_deref(),
        Some("Wholegrain oats (95%), sugar")
    );
    // Allergens and dietary come back sorted by their key.
    assert_eq!(
        read.allergens
            .iter()
            .map(|a| (a.allergen.as_str(), a.presence))
            .collect::<Vec<_>>(),
        vec![
            ("gluten", Presence::Contains),
            ("nuts", Presence::MayContain)
        ]
    );
    assert_eq!(
        read.dietary
            .iter()
            .map(|d| (d.flag.as_str(), d.value))
            .collect::<Vec<_>>(),
        vec![("palm_oil_free", Claim::Maybe), ("vegan", Claim::Yes)]
    );

    // A re-lookup restates facts in full: the old allergen/flag sets are replaced,
    // not merged. Here the product turns out to have no allergens and one flag.
    let restated = ProductFacts {
        nutrition: Some(Nutrition {
            salt_g: Some(0.2),
            ..nutrition()
        }),
        ingredients: Some("Wholegrain oats (100%)".into()),
        allergens: vec![],
        dietary: vec![DietaryFlag {
            flag: "vegan".into(),
            value: Claim::Yes,
        }],
    };
    repo::store_facts(&pool, product.id, &restated, Source::Off)
        .await
        .unwrap();

    let read = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(read.nutrition.unwrap().salt_g, Some(0.2), "panel updated");
    assert_eq!(read.ingredients.as_deref(), Some("Wholegrain oats (100%)"));
    assert!(read.allergens.is_empty(), "allergens replaced, not merged");
    assert_eq!(read.dietary.len(), 1, "flags replaced");

    // Deleting the product cascades the nutrition/allergen/dietary rows.
    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM product_nutrition WHERE product_id = ?")
            .bind(product.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "nutrition cascades on product delete");
}

#[tokio::test]
async fn two_sources_dietary_claims_coexist_and_merge() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping multi-source dietary test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000654";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        Source::Off,
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oat Drink"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Open Food Facts' ingredient analysis: a guess on vegan, firm on palm oil.
    repo::replace_dietary(
        &pool,
        product.id,
        &[
            DietaryFlag {
                flag: "vegan".into(),
                value: Claim::Maybe,
            },
            DietaryFlag {
                flag: "palm_oil_free".into(),
                value: Claim::Yes,
            },
            DietaryFlag {
                flag: "vegetarian".into(),
                value: Claim::No,
            },
        ],
        Source::Off,
    )
    .await
    .unwrap();

    // Asda's lifestyle tags for the same product: firm claims, 'yes'-only.
    repo::replace_dietary(
        &pool,
        product.id,
        &[
            DietaryFlag {
                flag: "vegan".into(),
                value: Claim::Yes,
            },
            DietaryFlag {
                flag: "vegetarian".into(),
                value: Claim::Yes,
            },
        ],
        Source::Asda,
    )
    .await
    .unwrap();

    let read = |flag: &str, facts: &life::products::nutrition::ProductFacts| {
        facts
            .dietary
            .iter()
            .find(|d| d.flag == flag)
            .map(|d| d.value)
    };
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(
        read("vegan", &facts),
        Some(Claim::Yes),
        "a firm claim settles a maybe"
    );
    assert_eq!(
        read("palm_oil_free", &facts),
        Some(Claim::Yes),
        "OFF's own claim survives"
    );
    assert_eq!(
        read("vegetarian", &facts),
        Some(Claim::Maybe),
        "sources disagree — say so rather than over-claim"
    );

    // The regression this migration exists for: re-looking-up the barcode on OFF
    // restates OFF's flags, and must NOT wipe Asda's.
    repo::replace_dietary(
        &pool,
        product.id,
        &[DietaryFlag {
            flag: "vegan".into(),
            value: Claim::Maybe,
        }],
        Source::Off,
    )
    .await
    .unwrap();
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(
        read("vegan", &facts),
        Some(Claim::Yes),
        "Asda's claim survives an OFF re-lookup"
    );
    assert_eq!(
        read("vegetarian", &facts),
        Some(Claim::Yes),
        "and OFF dropping its own claim leaves Asda's standing"
    );
}

#[tokio::test]
async fn two_sources_nutrition_allergens_ingredients_coexist_and_merge() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping multi-source facts test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000655";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        Source::Off,
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oat Drink"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // OFF: a crowd panel + ingredients, and milk only as a trace.
    repo::store_facts(
        &pool,
        product.id,
        &ProductFacts {
            nutrition: Some(Nutrition {
                salt_g: Some(0.1),
                ..nutrition()
            }),
            ingredients: Some("crowd-entered ingredients".into()),
            allergens: vec![Allergen {
                allergen: "milk".into(),
                presence: Presence::MayContain,
            }],
            dietary: vec![],
        },
        Source::Off,
    )
    .await
    .unwrap();

    // Asda (Brandbank): its own panel + ingredients, milk declared, soya added.
    repo::store_facts(
        &pool,
        product.id,
        &ProductFacts {
            nutrition: Some(Nutrition {
                salt_g: Some(0.9),
                ..nutrition()
            }),
            ingredients: Some("Water, Oats 10%".into()),
            allergens: vec![
                Allergen {
                    allergen: "milk".into(),
                    presence: Presence::Contains,
                },
                Allergen {
                    allergen: "soya".into(),
                    presence: Presence::Contains,
                },
            ],
            dietary: vec![],
        },
        Source::Asda,
    )
    .await
    .unwrap();

    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    // Nutrition + ingredients: the retailer's panel wins whole (not blended).
    assert_eq!(
        facts.nutrition.unwrap().salt_g,
        Some(0.9),
        "Asda's panel is shown, not OFF's or an average"
    );
    assert_eq!(facts.ingredients.as_deref(), Some("Water, Oats 10%"));
    // Allergens union, most-severe presence winning.
    assert_eq!(
        facts
            .allergens
            .iter()
            .map(|a| (a.allergen.as_str(), a.presence))
            .collect::<Vec<_>>(),
        vec![("milk", Presence::Contains), ("soya", Presence::Contains)],
        "milk upgraded to 'contains'; soya kept though OFF was silent"
    );

    // OFF re-lookup restates only OFF's rows — Asda's facts survive.
    repo::store_facts(
        &pool,
        product.id,
        &ProductFacts {
            nutrition: Some(Nutrition {
                salt_g: Some(0.1),
                ..nutrition()
            }),
            ingredients: Some("crowd-entered ingredients v2".into()),
            allergens: vec![],
            dietary: vec![],
        },
        Source::Off,
    )
    .await
    .unwrap();
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(
        facts.nutrition.unwrap().salt_g,
        Some(0.9),
        "Asda's panel still wins after an OFF re-lookup"
    );
    assert_eq!(facts.ingredients.as_deref(), Some("Water, Oats 10%"));
    assert!(
        facts.allergens.iter().any(|a| a.allergen == "soya"),
        "Asda's soya allergen survives OFF clearing its own"
    );
}

#[tokio::test]
async fn real_brandbank_facts_parse_store_and_read_back() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping Brandbank end-to-end test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "7394376616228"; // the real Oalty EAN from the fixture
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        Source::Asda,
        "6163443",
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oalty Oat Drink Barista Edition 1L Long Life"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // The whole chain the endpoint runs: parse the page blob, store as 'asda'.
    let facts = brandbank::parse(OALTY_BRANDBANK).expect("parse Brandbank");
    repo::store_facts(&pool, product.id, &facts, Source::Asda)
        .await
        .unwrap();

    let read = repo::facts_for(&pool, product.id).await.unwrap();
    let n = read.nutrition.expect("panel");
    assert_eq!(n.basis, "100ml");
    assert_eq!(n.energy_kj, Some(257.0));
    assert_eq!(n.salt_g, Some(0.1));
    assert!(
        read.ingredients
            .as_deref()
            .unwrap()
            .starts_with("Water, Oats 10%")
    );
    assert!(
        read.allergens.iter().any(|a| a.allergen == "oats"),
        "Oats declared"
    );
    let vegan = read.dietary.iter().find(|d| d.flag == "vegan");
    assert_eq!(vegan.map(|d| d.value), Some(Claim::Yes));
    assert!(
        read.dietary.iter().any(|d| d.flag == "milk_free"),
        "free-from booleans became dietary flags"
    );
}

/// A replace that fails partway must leave the source's PREVIOUS set intact.
///
/// This is the property that makes allergens safe to merge: `facts_for` unions
/// every source, so a half-applied replace doesn't read as "we're unsure about
/// nuts" — it reads as a product with no nut allergen at all. Before the delete
/// and the re-inserts shared a transaction, the DELETE autocommitted and any
/// failing INSERT left exactly that hole.
///
/// The failure is forced by a value the column cannot hold (`allergen` is
/// VARCHAR(48)), so it's the database that rejects the write, at the same place
/// a connection reset or a pod eviction would land.
#[tokio::test]
async fn a_failed_allergen_replace_keeps_the_previous_set() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping allergen atomicity test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000459";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    let product = repo::upsert_external(
        &pool,
        Source::Off,
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Nut Bar"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let declared = |a: &str| Allergen {
        allergen: a.into(),
        presence: Presence::Contains,
    };
    repo::replace_allergens(
        &pool,
        product.id,
        &[declared("nuts"), declared("milk")],
        Source::Asda,
    )
    .await
    .unwrap();

    // A second allergen too long for the column: the first INSERT of this batch
    // succeeds, the second fails — the case that used to leave the DELETE applied.
    let err = repo::replace_allergens(
        &pool,
        product.id,
        &[declared("soya"), declared(&"x".repeat(64))],
        Source::Asda,
    )
    .await;
    assert!(err.is_err(), "an over-long allergen must be rejected");

    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    let mut kept: Vec<&str> = facts
        .allergens
        .iter()
        .map(|a| a.allergen.as_str())
        .collect();
    kept.sort_unstable();
    assert_eq!(
        kept,
        ["milk", "nuts"],
        "the rejected replace must roll back whole — not drop the declared allergens"
    );
}
