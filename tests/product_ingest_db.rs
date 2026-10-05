//! Taking in one source's account of a product, against a real MariaDB. The
//! decisions are pinned in `product_ingest.rs`; this is what they write, and
//! that it is written whole or not at all.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId};
use life::products::ingest::{FactsUpdate, SourceAccount};
use life::products::nutrition::{Allergen, Presence, ProductFacts};
use life::products::prices::{Currency, PriceInput};
use life::products::repo;
use life::products::source::Source;

async fn pool() -> sqlx::MySqlPool {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

async fn fresh(pool: &sqlx::MySqlPool, barcode: &Barcode) {
    sqlx::query(
        "DELETE l FROM product_listings l JOIN products p ON p.id = l.product_id \
         WHERE p.barcode = ?",
    )
    .bind(barcode)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(pool)
        .await
        .unwrap();
}

fn off(barcode: &Barcode) -> SourceAccount {
    SourceAccount {
        source: Source::Off,
        external_id: ExternalId::from(barcode),
        barcode: Some(barcode.clone()),
        name: Some("porridge oats".into()),
        brand: Some("Quaker".into()),
        quantity_label: Some("500 g".into()),
        url: None,
        image_url: Some("https://images.openfoodfacts.org/x.jpg".into()),
        raw_json: Some(r#"{"status":1}"#.into()),
        price: None,
        facts: FactsUpdate::Full(Box::new(ProductFacts {
            nutrition: None,
            ingredients: Some("Oats".into()),
            allergens: vec![Allergen {
                allergen: "gluten".parse().unwrap(),
                presence: Presence::Contains,
            }],
            dietary: vec![],
        })),
    }
}

#[tokio::test]
async fn an_open_food_facts_account_lands_whole() {
    let pool = pool().await;
    let bc: Barcode = "9993300000001".parse().unwrap();
    fresh(&pool, &bc).await;

    let p = repo::ingest(
        &pool,
        &off(&bc),
        Some((vec![0xFF, 0xD8, 0xFF], "image/jpeg".into())),
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("porridge oats"));
    assert_eq!(p.quantity_label.as_deref(), Some("500 g"));
    assert_eq!(p.pack.map(|s| s.value), Some(500.0));
    assert!(p.has_image);
    assert_eq!(
        p.image_source,
        Some(Source::Off),
        "a fetched picture says where it came from, so it is not offered against itself"
    );
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    assert_eq!((listings.len(), listings[0].source), (1, Source::Off));
    let facts = repo::facts_for(&pool, p.id).await.unwrap();
    assert_eq!(facts.ingredients.as_deref(), Some("Oats"));
    assert_eq!(facts.allergens.len(), 1);

    // A second picture never replaces the first: a held one is changed only
    // through the picture reconcile.
    let again = repo::ingest(
        &pool,
        &off(&bc),
        Some((vec![0x89, b'P'], "image/png".into())),
    )
    .await
    .unwrap();
    let (bytes, _) = repo::get_image_by_id(&pool, again.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bytes, [0xFF, 0xD8, 0xFF]);
}

#[tokio::test]
async fn a_shop_fills_a_missing_brand_and_says_whose_it_is() {
    let pool = pool().await;
    let bc: Barcode = "9993300000002".parse().unwrap();
    fresh(&pool, &bc).await;
    let brandless = SourceAccount {
        brand: None,
        image_url: None,
        facts: FactsUpdate::None,
        ..off(&bc)
    };
    repo::ingest(&pool, &brandless, None).await.unwrap();
    let asda = SourceAccount {
        source: Source::Asda,
        external_id: "9993300002".parse().unwrap(),
        name: Some("Quaker Oats".into()),
        brand: Some("Quaker".into()),
        quantity_label: Some("1KG".into()),
        image_url: None,
        raw_json: None,
        facts: FactsUpdate::None,
        ..off(&bc)
    };
    let p = repo::ingest(&pool, &asda, None).await.unwrap();
    assert_eq!(p.brand.as_deref(), Some("Quaker"), "a gap is filled");
    assert_eq!(
        p.quantity_label.as_deref(),
        Some("500 g"),
        "a held pack stands"
    );
    assert_eq!(
        p.name.as_deref(),
        Some("porridge oats"),
        "a held name stands"
    );
    let (brand_source,): (Option<String>,) =
        sqlx::query_as("SELECT brand_source FROM products WHERE id = ?")
            .bind(p.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(brand_source.as_deref(), Some("asda"));
}

#[tokio::test]
async fn an_ingest_that_cannot_record_its_price_writes_nothing() {
    let pool = pool().await;
    let bc: Barcode = "9993300000003".parse().unwrap();
    fresh(&pool, &bc).await;
    let account = SourceAccount {
        source: Source::Asda,
        external_id: "INGEST-ATOMIC-1".parse().unwrap(),
        price: Some(PriceInput {
            amount_minor: 199,
            currency: Currency::gbp(),
            unit_price: None,
        }),
        ..off(&bc)
    };
    sqlx::query("DROP TRIGGER IF EXISTS refuse_test_price")
        .execute(&pool)
        .await
        .unwrap();
    // Refuses the price of this one listing only, so the step fails last of all.
    sqlx::query(
        "CREATE TRIGGER refuse_test_price BEFORE INSERT ON price_observations FOR EACH ROW \
         IF (SELECT external_id FROM product_listings WHERE id = NEW.listing_id) = 'INGEST-ATOMIC-1' \
         THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'price refused by the test'; END IF",
    )
    .execute(&pool)
    .await
    .unwrap();
    let res = repo::ingest(&pool, &account, Some((vec![1], "image/jpeg".into()))).await;
    sqlx::query("DROP TRIGGER refuse_test_price")
        .execute(&pool)
        .await
        .unwrap();

    assert!(res.is_err(), "the refused price must fail the ingest");
    assert!(
        repo::get(&pool, &bc).await.unwrap().is_none(),
        "no product, so no listing, facts or picture either"
    );
    let (listings,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM product_listings WHERE external_id = 'INGEST-ATOMIC-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(listings, 0);
}
