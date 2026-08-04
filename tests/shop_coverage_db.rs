//! The two coverage queries against a real MariaDB — the only check on their
//! SQL. Runs only when LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId, ProductId};
use life::products::repo;
use life::products::shop_cache::{self, CachedListing};
use life::products::source::Source;
use sqlx::MySqlPool;

fn barcode() -> Barcode {
    "5000000000901".parse().unwrap()
}

fn ext(id: &str) -> ExternalId {
    id.parse().unwrap()
}

async fn fixture() -> (MySqlPool, ProductId) {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    // A product listed at Asda and at Open Food Facts, plus a Waitrose sighting
    // of the same barcode that we have never attached.
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM shop_listings WHERE external_id LIKE 'cov-%'")
        .execute(&pool)
        .await
        .unwrap();
    let id = repo::upsert_external(
        &pool,
        Source::Asda,
        &ext("cov-asda-1"),
        Some(&barcode()),
        &repo::ListingFields {
            raw_name: Some("Coverage Test Product"),
            ..Default::default()
        },
    )
    .await
    .expect("seed product")
    .id;
    shop_cache::remember(
        &pool,
        &[CachedListing {
            source: Source::Waitrose,
            external_id: ext("cov-wtr-1"),
            barcode: Some(barcode()),
            name: Some("Coverage Test Product".to_string()),
            brand: None,
            quantity_label: None,
            image_url: None,
        }],
    )
    .await
    .expect("seed sighting");
    (pool, id)
}

#[tokio::test]
async fn a_held_listing_is_found_and_off_is_not_a_shop() {
    let (pool, id) = fixture().await;
    let held = repo::shops_holding(&pool, &[id]).await.unwrap();
    let sources: Vec<Source> = held.iter().map(|l| l.source).collect();
    assert!(sources.contains(&Source::Asda), "{sources:?}");
    // The catalogue row also has an 'off' listing (barcode-keyed) — Open Food
    // Facts is not a shop you can walk into, so it must not appear here.
    assert!(!sources.contains(&Source::Off), "{sources:?}");
    assert!(held.iter().all(|l| l.product_id == id));
}

#[tokio::test]
async fn a_sighting_is_found_by_barcode_without_any_listing_of_ours() {
    let (pool, _) = fixture().await;
    let seen = repo::shops_seen_carrying(&pool, &[barcode()])
        .await
        .unwrap();
    assert!(
        seen.iter()
            .any(|s| s.barcode == barcode() && s.source == Source::Waitrose),
        "{seen:?}"
    );
}

#[tokio::test]
async fn asking_about_nothing_queries_nothing() {
    let (pool, _) = fixture().await;
    // Guarded in the repo rather than the caller: an empty IN () is a SQL syntax
    // error, so the empty case has to be a real answer, not a crash.
    assert!(repo::shops_holding(&pool, &[]).await.unwrap().is_empty());
    assert!(
        repo::shops_seen_carrying(&pool, &[])
            .await
            .unwrap()
            .is_empty()
    );
}
