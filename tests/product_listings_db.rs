//! The product/listing split against a real MariaDB: reconciling multiple
//! sources onto ONE canonical product by barcode — the thing the old flat
//! `products` table (with its UNIQUE(barcode)) made impossible. Runs only when
//! LIFE_TEST_DATABASE_URL is set.

use life::db;
use life::products::repo;

#[tokio::test]
async fn two_sources_one_barcode_become_one_product_two_listings() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping listings DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5740900404465";
    let asda_cin = "listtest-asda-7690049";
    let wr_ln = "listtest-wr-812345";
    // Clean slate: deleting the product cascades its listings (FK ON DELETE
    // CASCADE); also clear listings by our external ids in case a prior run left
    // them on some other product.
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [asda_cin, wr_ln] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Asda imports the product, carrying its EAN (Asda's IMAGE_ID).
    let a = repo::upsert_external(
        &pool,
        "asda",
        asda_cin,
        Some(barcode),
        Some("Lurpak Spreadable 400g"),
        Some("Lurpak"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(a.barcode.as_deref(), Some(barcode));

    // Waitrose imports the SAME physical product (same EAN) under its own id.
    let w = repo::upsert_external(
        &pool,
        "waitrose",
        wr_ln,
        Some(barcode),
        Some("Lurpak Spreadable"),
        Some("Lurpak"),
        None,
    )
    .await
    .unwrap();

    // One canonical product, reached from either shop's id.
    assert_eq!(a.id, w.id, "same barcode → same canonical product");

    let listings = repo::listings_for(&pool, a.id).await.unwrap();
    let mut sources: Vec<&str> = listings.iter().map(|l| l.source.as_str()).collect();
    sources.sort();
    assert_eq!(
        sources,
        vec!["asda", "waitrose"],
        "both sources on one product"
    );

    // get_by_source_external resolves the same product via EITHER listing.
    assert_eq!(
        repo::get_by_source_external(&pool, "asda", asda_cin)
            .await
            .unwrap()
            .unwrap()
            .id,
        a.id
    );
    assert_eq!(
        repo::get_by_source_external(&pool, "waitrose", wr_ln)
            .await
            .unwrap()
            .unwrap()
            .id,
        a.id
    );

    // Re-importing a source updates its listing in place, not the product count.
    repo::upsert_external(
        &pool,
        "asda",
        asda_cin,
        Some(barcode),
        Some("Lurpak Slightly Salted Spreadable 400g"),
        Some("Lurpak"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        repo::listings_for(&pool, a.id).await.unwrap().len(),
        2,
        "re-import refreshes, doesn't duplicate"
    );
}

#[tokio::test]
async fn barcodeless_sources_stay_separate_products() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping listings DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let (e1, e2) = ("listtest-bl-aaa", "listtest-bl-bbb");
    sqlx::query("DELETE FROM products WHERE external_id IN (?, ?)")
        .bind(e1)
        .bind(e2)
        .execute(&pool)
        .await
        .unwrap();

    // Two barcodeless Waitrose products are distinct catalog rows (nothing to
    // reconcile them on), each with its single listing.
    let p1 = repo::upsert_external(&pool, "waitrose", e1, None, Some("Thing A"), None, None)
        .await
        .unwrap();
    let p2 = repo::upsert_external(&pool, "waitrose", e2, None, Some("Thing B"), None, None)
        .await
        .unwrap();

    assert_ne!(p1.id, p2.id, "different barcodeless products stay separate");
    assert!(p1.barcode.is_none());
    assert_eq!(repo::listings_for(&pool, p1.id).await.unwrap().len(), 1);

    // Re-import of a barcodeless product refreshes its single-source name.
    let p1b = repo::upsert_external(&pool, "waitrose", e1, None, Some("Thing A v2"), None, None)
        .await
        .unwrap();
    assert_eq!(p1b.id, p1.id);
    assert_eq!(p1b.name.as_deref(), Some("Thing A v2"));
}
