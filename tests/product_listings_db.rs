//! The product/listing split against a real MariaDB: reconciling multiple
//! sources onto ONE canonical product by barcode — the thing the old flat
//! `products` table (with its UNIQUE(barcode)) made impossible. Runs only when
//! LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId};
use life::products::repo;
use life::products::source::Source;

#[tokio::test]
async fn two_sources_one_barcode_become_one_product_two_listings() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode: Barcode = "5740900404465".parse().unwrap();
    let asda_cin: ExternalId = "listtest-asda-7690049".parse().unwrap();
    let wr_ln: ExternalId = "listtest-wr-812345".parse().unwrap();
    // Clean slate: deleting the product cascades its listings (FK ON DELETE
    // CASCADE); also clear listings by our external ids in case a prior run left
    // them on some other product.
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [&asda_cin, &wr_ln] {
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
        Source::Asda,
        &asda_cin,
        Some(&barcode),
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
    assert_eq!(
        a.barcode.as_ref().map(Barcode::as_str),
        Some(barcode.as_str())
    );

    // The listing kept Asda's own account, structured fields and all.
    let asda_listing = repo::listings_for(&pool, a.id)
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.source == Source::Asda)
        .expect("asda listing");
    assert_eq!(asda_listing.brand.as_deref(), Some("Lurpak"));
    assert_eq!(asda_listing.quantity_label.as_deref(), Some("400G"));
    assert!(asda_listing.image_url.is_some());

    // Waitrose imports the SAME physical product (same EAN) under its own id,
    // with a DIFFERENT name.
    let w = repo::upsert_external(
        &pool,
        Source::Waitrose,
        &wr_ln,
        Some(&barcode),
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
    let mut sources: Vec<Source> = listings.iter().map(|l| l.source).collect();
    sources.sort();
    assert_eq!(
        sources,
        vec![Source::Asda, Source::Waitrose],
        "both sources on one product"
    );

    // get_by_source_external resolves the same product via EITHER listing.
    assert_eq!(
        repo::get_by_source_external(&pool, Source::Asda, &asda_cin)
            .await
            .unwrap()
            .unwrap()
            .id,
        a.id
    );
    assert_eq!(
        repo::get_by_source_external(&pool, Source::Waitrose, &wr_ln)
            .await
            .unwrap()
            .unwrap()
            .id,
        a.id
    );

    // Re-importing a source updates its listing in place, not the product count.
    repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_cin,
        Some(&barcode),
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
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let (e1, e2): (ExternalId, ExternalId) = (
        "listtest-bl-aaa".parse().unwrap(),
        "listtest-bl-bbb".parse().unwrap(),
    );
    sqlx::query("DELETE FROM products WHERE external_id IN (?, ?)")
        .bind(&e1)
        .bind(&e2)
        .execute(&pool)
        .await
        .unwrap();

    // Two barcodeless Waitrose products are distinct catalog rows (nothing to
    // reconcile them on), each with its single listing.
    let name_only = |n| repo::ListingFields {
        raw_name: Some(n),
        ..Default::default()
    };
    let p1 = repo::upsert_external(&pool, Source::Waitrose, &e1, None, &name_only("Thing A"))
        .await
        .unwrap();
    let p2 = repo::upsert_external(&pool, Source::Waitrose, &e2, None, &name_only("Thing B"))
        .await
        .unwrap();

    assert_ne!(p1.id, p2.id, "different barcodeless products stay separate");
    assert!(p1.barcode.is_none());
    assert_eq!(repo::listings_for(&pool, p1.id).await.unwrap().len(), 1);

    // Re-import of a barcodeless product refreshes its single-source name: it's
    // the sole authority, so nothing can diverge from it.
    let p1b = repo::upsert_external(&pool, Source::Waitrose, &e1, None, &name_only("Thing A v2"))
        .await
        .unwrap();
    assert_eq!(p1b.id, p1.id);
    assert_eq!(p1b.name.as_deref(), Some("Thing A v2"));
}

/// The pack-size label a shop supplies. `sync_listing` writes it only when the
/// product has none — Open Food Facts' `quantity` is the product's own ("500g"),
/// while Asda's `PACK_SIZE` describes the pack it happens to sell ("22x27G"), so
/// a shop may fill the gap but never overrule what we already hold.
#[tokio::test]
async fn a_shop_pack_size_fills_the_gap_and_survives_a_reread() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode: Barcode = "9993300000001".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();

    let product = repo::upsert_external(
        &pool,
        Source::Asda,
        &"packsize-cin-1".parse::<ExternalId>().unwrap(),
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Fruit Bar Multipack"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(product.quantity_label, None, "nothing has supplied one yet");

    repo::set_quantity_label(&pool, product.id, "22x27G")
        .await
        .unwrap();
    let read = repo::get_by_id(&pool, product.id)
        .await
        .unwrap()
        .expect("product");
    assert_eq!(read.quantity_label.as_deref(), Some("22x27G"));

    // A later re-pull of the same listing must not wipe it: the label lives on
    // the canonical row, and re-importing only rewrites the source's own line.
    repo::upsert_external(
        &pool,
        Source::Asda,
        &"packsize-cin-1".parse::<ExternalId>().unwrap(),
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Fruit Bar Multipack 22 pack"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after = repo::get_by_id(&pool, product.id)
        .await
        .unwrap()
        .expect("product");
    assert_eq!(after.quantity_label.as_deref(), Some("22x27G"));
}
