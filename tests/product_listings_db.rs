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

    // Asda imports the product, carrying its EAN (Asda's IMAGE_ID) and its whole
    // record (brand, pack, image URL, raw payload) on its own listing line.
    let a = repo::upsert_external(
        &pool,
        "asda",
        asda_cin,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Lurpak Spreadable 400g"),
            brand: Some("Lurpak"),
            quantity_label: Some("400G"),
            image_url: Some("https://asdagroceries.scene7.com/is/image/x?$ProdList$"),
            raw_json: Some(r#"{"CIN":"7690049","PACK_SIZE":"400G"}"#),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(a.barcode.as_deref(), Some(barcode));

    // The listing kept Asda's own account, structured fields and all.
    let asda_listing = repo::listings_for(&pool, a.id)
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.source == "asda")
        .expect("asda listing");
    assert_eq!(asda_listing.brand.as_deref(), Some("Lurpak"));
    assert_eq!(asda_listing.quantity_label.as_deref(), Some("400G"));
    assert!(asda_listing.image_url.is_some());

    // Waitrose imports the SAME physical product (same EAN) under its own id,
    // with a DIFFERENT name.
    let w = repo::upsert_external(
        &pool,
        "waitrose",
        wr_ln,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Lurpak Spreadable"),
            brand: Some("Lurpak"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Fill-if-empty, never silent-overwrite: the canonical name stays what the
    // first source seeded it to; Waitrose's differing name does NOT clobber it
    // (it becomes a divergence to approve, not an automatic switch).
    assert_eq!(
        w.name.as_deref(),
        Some("Lurpak Spreadable 400g"),
        "a second source's differing name must not overwrite the canonical name"
    );

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
        &repo::ListingFields {
            raw_name: Some("Lurpak Slightly Salted Spreadable 400g"),
            brand: Some("Lurpak"),
            ..Default::default()
        },
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
    let name_only = |n| repo::ListingFields {
        raw_name: Some(n),
        ..Default::default()
    };
    let p1 = repo::upsert_external(&pool, "waitrose", e1, None, &name_only("Thing A"))
        .await
        .unwrap();
    let p2 = repo::upsert_external(&pool, "waitrose", e2, None, &name_only("Thing B"))
        .await
        .unwrap();

    assert_ne!(p1.id, p2.id, "different barcodeless products stay separate");
    assert!(p1.barcode.is_none());
    assert_eq!(repo::listings_for(&pool, p1.id).await.unwrap().len(), 1);

    // Re-import of a barcodeless product refreshes its single-source name: it's
    // the sole authority, so nothing can diverge from it.
    let p1b = repo::upsert_external(&pool, "waitrose", e1, None, &name_only("Thing A v2"))
        .await
        .unwrap();
    assert_eq!(p1b.id, p1.id);
    assert_eq!(p1b.name.as_deref(), Some("Thing A v2"));
}
