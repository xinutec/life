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
async fn canonical_name_prefers_retailers_over_crowd_names() {
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

    // Open Food Facts sees the barcode first: its crowd name is all we have.
    let p = repo::upsert_external(
        &pool,
        "off",
        barcode,
        Some(barcode),
        Some("quaker oats porridge oats 500 g value pack"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        p.name.as_deref(),
        Some("quaker oats porridge oats 500 g value pack")
    );
    assert_eq!(p.name_source.as_deref(), Some("off"));

    // A retailer listing arrives: its curated title takes over the canonical name.
    let p = repo::upsert_external(
        &pool,
        "asda",
        "dettest-asda-1",
        Some(barcode),
        Some("Quaker Porridge Oats 500g"),
        Some("Quaker"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("Quaker Porridge Oats 500g"));
    assert_eq!(p.name_source.as_deref(), Some("asda"));

    // Waitrose outranks Asda in the preference order.
    let p = repo::upsert_external(
        &pool,
        "waitrose",
        "dettest-wr-1",
        Some(barcode),
        Some("Waitrose Porridge Oats"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("Waitrose Porridge Oats"));
    assert_eq!(p.name_source.as_deref(), Some("waitrose"));

    // Preference is by rank, never recency: an OFF re-import updates the off
    // listing's raw name but cannot reclaim the canonical name.
    let p = repo::upsert_external(
        &pool,
        "off",
        barcode,
        Some(barcode),
        Some("renamed crowd entry"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("Waitrose Porridge Oats"));
    assert_eq!(p.name_source.as_deref(), Some("waitrose"));

    // The OFF lookup path re-caches via repo::upsert, which restates OFF's crowd
    // name on the row; the refresh that follows it (see the lookup route) must
    // put the preferred name back.
    repo::upsert(&pool, barcode, Some("crowd name again"), None, None, None)
        .await
        .unwrap();
    repo::refresh_canonical_name(&pool, p.id).await.unwrap();
    let healed = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(healed.name.as_deref(), Some("Waitrose Porridge Oats"));
    assert_eq!(healed.name_source.as_deref(), Some("waitrose"));
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
        Some("Future Thing"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(p.name.as_deref(), Some("Future Thing"));
    assert_eq!(p.name_source.as_deref(), Some("somefutureshop"));
}
