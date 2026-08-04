//! Price observations against a real MariaDB: append-only history hanging off a
//! product_listing, and the latest-price-per-shop query that feeds the
//! "cheapest shop" view. Runs only when LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId};
use life::products::prices::PriceInput;
use life::products::repo;
use life::products::source::Source;

fn gbp(amount_minor: i64, unit: Option<(i64, &str)>) -> PriceInput {
    PriceInput {
        amount_minor,
        currency: "GBP".into(),
        unit_amount_minor: unit.map(|(a, _)| a),
        unit_measure: unit.map(|(_, m)| m.to_string()),
        region: Some("EN".into()),
    }
}

#[tokio::test]
async fn latest_price_per_shop_cheapest_first_with_history() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode: Barcode = "5000000000123".parse().unwrap();
    let asda_id: ExternalId = "pricetest-asda-1".parse().unwrap();
    let wr_id: ExternalId = "pricetest-wr-1".parse().unwrap();
    // Deleting the product cascades listings and their price observations.
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [&asda_id, &wr_id] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Both shops list the same product (reconciled by barcode → one product).
    let product = repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_id,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Milk"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo::upsert_external(
        &pool,
        Source::Waitrose,
        &wr_id,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Milk"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let asda_listing = repo::listing_id(&pool, Source::Asda, &asda_id)
        .await
        .unwrap()
        .unwrap();
    let wr_listing = repo::listing_id(&pool, Source::Waitrose, &wr_id)
        .await
        .unwrap()
        .unwrap();

    // No prices yet.
    assert!(
        repo::latest_prices(&pool, product.id)
            .await
            .unwrap()
            .is_empty()
    );

    // Record one price per shop: Asda cheaper.
    repo::record_price(&pool, asda_listing, &gbp(357, Some((892, "KG"))))
        .await
        .unwrap();
    repo::record_price(&pool, wr_listing, &gbp(380, None))
        .await
        .unwrap();

    let prices = repo::latest_prices(&pool, product.id).await.unwrap();
    assert_eq!(prices.len(), 2, "one latest price per shop");
    assert_eq!(prices[0].source, Source::Asda); // cheapest first
    assert_eq!(prices[0].amount_minor, 357);
    assert_eq!(prices[0].unit_amount_minor, Some(892));
    assert_eq!(prices[0].unit_measure.as_deref(), Some("KG"));
    assert_eq!(prices[1].source, Source::Waitrose);
    assert_eq!(prices[1].amount_minor, 380);

    // A newer Asda observation is history, not a duplicate — latest reflects it.
    repo::record_price(&pool, asda_listing, &gbp(340, Some((850, "KG"))))
        .await
        .unwrap();
    let prices = repo::latest_prices(&pool, product.id).await.unwrap();
    assert_eq!(prices.len(), 2, "still one latest per shop");
    assert_eq!(prices[0].amount_minor, 340, "newest Asda price wins");

    // The full history is retained (two Asda rows + one Waitrose row).
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM price_observations po JOIN product_listings l ON l.id = po.listing_id \
         WHERE l.product_id = ?",
    )
    .bind(product.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 3, "observations are append-only");
}

#[tokio::test]
async fn a_shop_listing_a_product_twice_collapses_to_its_cheapest() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    // Asda can carry two CINs for one EAN (a relist), so one product ends up with
    // two 'asda' listings. "Where to buy" wants ONE answer per shop — its best
    // price — and the link must point at the listing that quoted it.
    let barcode: Barcode = "5000000000987".parse().unwrap();
    let (cin_a, cin_b): (ExternalId, ExternalId) = (
        "pricetest-asda-dup-a".parse().unwrap(),
        "pricetest-asda-dup-b".parse().unwrap(),
    );
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [&cin_a, &cin_b] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    let product = repo::upsert_external(
        &pool,
        Source::Asda,
        &cin_a,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Butter"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo::upsert_external(
        &pool,
        Source::Asda,
        &cin_b,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Butter"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let listing_a = repo::listing_id(&pool, Source::Asda, &cin_a)
        .await
        .unwrap()
        .unwrap();
    let listing_b = repo::listing_id(&pool, Source::Asda, &cin_b)
        .await
        .unwrap()
        .unwrap();

    repo::record_price(&pool, listing_a, &gbp(420, None))
        .await
        .unwrap();
    repo::record_price(&pool, listing_b, &gbp(395, None))
        .await
        .unwrap();

    let prices = repo::latest_prices(&pool, product.id).await.unwrap();
    assert_eq!(prices.len(), 1, "one row per shop, not one per listing");
    assert_eq!(prices[0].source, Source::Asda);
    assert_eq!(
        prices[0].amount_minor, 395,
        "the shop's cheapest listing wins"
    );
    assert_eq!(
        prices[0].external_id, cin_b,
        "the row names the listing that quoted the winning price"
    );
}
