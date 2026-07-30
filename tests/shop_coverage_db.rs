//! The two coverage queries against a real MariaDB — the only check on their
//! SQL. Runs only when LIFE_TEST_DATABASE_URL is set.

use life::db;
use life::products::repo;
use life::products::shop_cache::{self, CachedListing};
use life::products::source::Source;
use sqlx::MySqlPool;

const BARCODE: &str = "5000000000901";

async fn fixture() -> Option<(MySqlPool, u64)> {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping coverage DB test");
        return None;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    // A product listed at Asda and at Open Food Facts, plus a Waitrose sighting
    // of the same barcode that we have never attached.
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(BARCODE)
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
        "cov-asda-1",
        Some(BARCODE),
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
            external_id: "cov-wtr-1".to_string(),
            barcode: Some(BARCODE.to_string()),
            name: Some("Coverage Test Product".to_string()),
            brand: None,
            quantity_label: None,
            image_url: None,
        }],
    )
    .await
    .expect("seed sighting");
    Some((pool, id))
}

#[tokio::test]
async fn a_held_listing_is_found_and_off_is_not_a_shop() {
    let Some((pool, id)) = fixture().await else {
        return;
    };
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
    let Some((pool, _)) = fixture().await else {
        return;
    };
    let seen = repo::shops_seen_carrying(&pool, &[BARCODE.to_string()])
        .await
        .unwrap();
    assert!(
        seen.iter()
            .any(|s| s.barcode == BARCODE && s.source == Source::Waitrose),
        "{seen:?}"
    );
}

#[tokio::test]
async fn asking_about_nothing_queries_nothing() {
    let Some((pool, _)) = fixture().await else {
        return;
    };
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
