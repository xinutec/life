//! Product facts against a real MariaDB: storing a product's nutrition panel,
//! ingredients, allergens, and dietary flags, then reading them back — plus the
//! whole-product REPLACE semantics of a re-lookup. Runs only when
//! LIFE_TEST_DATABASE_URL is set.

use std::collections::BTreeMap;

use life::db;
use life::products::nutrition::{Allergen, DietaryFlag, Nutrition, ProductFacts};
use life::products::repo;

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
        "off",
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
                presence: "contains".into(),
            },
            Allergen {
                allergen: "nuts".into(),
                presence: "may_contain".into(),
            },
        ],
        dietary: vec![
            DietaryFlag {
                flag: "vegan".into(),
                value: "yes".into(),
            },
            DietaryFlag {
                flag: "palm_oil_free".into(),
                value: "maybe".into(),
            },
        ],
    };
    repo::store_facts(&pool, product.id, &facts, "off")
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
            .map(|a| (a.allergen.as_str(), a.presence.as_str()))
            .collect::<Vec<_>>(),
        vec![("gluten", "contains"), ("nuts", "may_contain")]
    );
    assert_eq!(
        read.dietary
            .iter()
            .map(|d| (d.flag.as_str(), d.value.as_str()))
            .collect::<Vec<_>>(),
        vec![("palm_oil_free", "maybe"), ("vegan", "yes")]
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
            value: "yes".into(),
        }],
    };
    repo::store_facts(&pool, product.id, &restated, "off")
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

    // Open Food Facts' ingredient analysis: a guess on vegan, firm on palm oil.
    repo::replace_dietary(
        &pool,
        product.id,
        &[
            DietaryFlag {
                flag: "vegan".into(),
                value: "maybe".into(),
            },
            DietaryFlag {
                flag: "palm_oil_free".into(),
                value: "yes".into(),
            },
            DietaryFlag {
                flag: "vegetarian".into(),
                value: "no".into(),
            },
        ],
        "off",
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
                value: "yes".into(),
            },
            DietaryFlag {
                flag: "vegetarian".into(),
                value: "yes".into(),
            },
        ],
        "asda",
    )
    .await
    .unwrap();

    let read = |flag: &str, facts: &life::products::nutrition::ProductFacts| {
        facts
            .dietary
            .iter()
            .find(|d| d.flag == flag)
            .map(|d| d.value.clone())
    };
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(
        read("vegan", &facts).as_deref(),
        Some("yes"),
        "a firm claim settles a maybe"
    );
    assert_eq!(
        read("palm_oil_free", &facts).as_deref(),
        Some("yes"),
        "OFF's own claim survives"
    );
    assert_eq!(
        read("vegetarian", &facts).as_deref(),
        Some("maybe"),
        "sources disagree — say so rather than over-claim"
    );

    // The regression this migration exists for: re-looking-up the barcode on OFF
    // restates OFF's flags, and must NOT wipe Asda's.
    repo::replace_dietary(
        &pool,
        product.id,
        &[DietaryFlag {
            flag: "vegan".into(),
            value: "maybe".into(),
        }],
        "off",
    )
    .await
    .unwrap();
    let facts = repo::facts_for(&pool, product.id).await.unwrap();
    assert_eq!(
        read("vegan", &facts).as_deref(),
        Some("yes"),
        "Asda's claim survives an OFF re-lookup"
    );
    assert_eq!(
        read("vegetarian", &facts).as_deref(),
        Some("yes"),
        "and OFF dropping its own claim leaves Asda's standing"
    );
}
