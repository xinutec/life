//! Increment 4 semantics: deep links derived from listing identity (pure), and
//! the canonical-name preference across sources against a real MariaDB (those
//! tests run only when LIFE_TEST_DATABASE_URL is set).

use life::db;
use life::products::{repo, source};

#[test]
fn deep_links_derive_from_listing_identity() {
    assert_eq!(
        source::listing_url("off", "5000328042732").as_deref(),
        Some("https://world.openfoodfacts.org/product/5000328042732")
    );
    assert_eq!(
        source::listing_url("asda", "9346702").as_deref(),
        Some("https://www.asda.com/groceries/product/9346702")
    );
    // Waitrose PDP URLs carry a slug, but any slug redirects to the canonical
    // one — the trailing lineNumber is the key.
    assert_eq!(
        source::listing_url("waitrose", "271105").as_deref(),
        Some("https://www.waitrose.com/ecom/products/x/271105")
    );
    // Sources without a public product page yield no link.
    assert_eq!(source::listing_url("user", "anything"), None);
}

#[tokio::test]
async fn canonical_name_is_sticky_a_new_source_does_not_silently_switch_it() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping canonical-name DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000789";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in ["dettest-asda-1", "dettest-wr-1"] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Open Food Facts sees the barcode first: its crowd name seeds the empty
    // canonical name (fill-if-empty).
    let p = repo::upsert_external(
        &pool,
        "off",
        barcode,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("quaker oats porridge oats 500 g value pack"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        p.name.as_deref(),
        Some("quaker oats porridge oats 500 g value pack")
    );
    assert_eq!(p.name_source.as_deref(), Some("off"));

    // A retailer listing arrives with a cleaner title. It does NOT silently take
    // over the canonical name — no source overwrites another behind your back.
    // The retailer's name is captured on its own listing as a candidate to
    // approve (that reconciliation is a later increment), and the canonical name
    // stays exactly what it was.
    let p = repo::upsert_external(
        &pool,
        "asda",
        "dettest-asda-1",
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Quaker Porridge Oats 500g"),
            brand: Some("Quaker"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        p.name.as_deref(),
        Some("quaker oats porridge oats 500 g value pack"),
        "a new source must not silently replace the canonical name"
    );
    assert_eq!(p.name_source.as_deref(), Some("off"));

    // Both the crowd title and the retailer's cleaner title are on record, each
    // on its own line — the raw material a diff-and-approve reconciliation needs.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let by_source = |s: &str| {
        listings
            .iter()
            .find(|l| l.source == s)
            .and_then(|l| l.raw_name.clone())
    };
    assert_eq!(
        by_source("off").as_deref(),
        Some("quaker oats porridge oats 500 g value pack")
    );
    assert_eq!(
        by_source("asda").as_deref(),
        Some("Quaker Porridge Oats 500g")
    );

    // A genuinely empty canonical name IS seeded from the best-ranked source
    // present — fill-if-empty still fills.
    sqlx::query("UPDATE products SET name = NULL, name_source = NULL WHERE id = ?")
        .bind(p.id)
        .execute(&pool)
        .await
        .unwrap();
    repo::refresh_canonical_name(&pool, p.id).await.unwrap();
    let seeded = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert!(
        seeded.name.is_some(),
        "an empty canonical name is filled from a listing"
    );
}

#[tokio::test]
async fn unranked_sources_keep_their_own_name() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping unranked-name DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let ext = "dettest-unranked-1";
    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(ext)
        .execute(&pool)
        .await
        .unwrap();

    // A source outside the name-preference order still names its own product —
    // the refresh leaves rows alone when no ranked candidate exists.
    let p = repo::upsert_external(
        &pool,
        "somefutureshop",
        ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Future Thing"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("Future Thing"));
    assert_eq!(p.name_source.as_deref(), Some("somefutureshop"));
}
