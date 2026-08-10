//! Product cache against a real MariaDB (no Open Food Facts call — pure cache
//! layer). Runs only when LIFE_TEST_DATABASE_URL is set.

mod common;

use life::db;
use life::products::ids::{Barcode, ExternalId, ProductId};
use life::products::packsize::{PackSize, PackUnit};
use life::products::repo;
use life::products::source::Source;

#[tokio::test]
async fn product_cache_against_real_db() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    // A real EAN shape: `Barcode` is the type of a catalogue key now, so a
    // test can no longer stand one in that the catalogue could never hold.
    let bc: Barcode = "9990000000001".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&bc)
        .execute(&pool)
        .await
        .unwrap();

    // Miss.
    assert!(repo::get(&pool, &bc).await.unwrap().is_none());

    // Cache with an image.
    repo::upsert(
        &pool,
        &bc,
        Some("Test Yog"),
        Some("BrandX"),
        Some("950g"),
        Some((vec![1, 2, 3, 4], "image/png".into())),
    )
    .await
    .unwrap();
    let p = repo::get(&pool, &bc).await.unwrap().expect("cached");
    assert_eq!(p.name.as_deref(), Some("Test Yog"));
    assert_eq!(p.quantity_label.as_deref(), Some("950g"));
    // The label read as an amount, on the way out of the DB. Derived in the one
    // row→Product mapping every getter goes through, so this holds for a product
    // read anywhere — which is the only reason callers need not ask for it.
    assert_eq!(
        p.pack,
        Some(PackSize {
            value: 950.0,
            unit: PackUnit::Gram
        })
    );
    assert!(p.has_image);

    let (bytes, mime) = repo::get_image(&pool, &bc).await.unwrap().expect("image");
    assert_eq!(bytes, vec![1, 2, 3, 4]);
    assert_eq!(mime, "image/png");

    // A second OFF lookup FILLS GAPS, it does not overwrite: the cached name and
    // image stand, because a source disagreeing with what we hold is a divergence
    // to approve (see repo::divergences), not something to apply behind your back.
    repo::upsert(&pool, &bc, Some("Test Yog 2"), None, None, None)
        .await
        .unwrap();
    let p2 = repo::get(&pool, &bc).await.unwrap().expect("cached");
    assert_eq!(p2.name.as_deref(), Some("Test Yog"), "the held name stands");
    assert!(p2.has_image, "and so does the held image");

    // A gap, though, is filled — nothing is lost by learning what we didn't know.
    sqlx::query("UPDATE products SET brand = NULL WHERE barcode = ?")
        .bind(&bc)
        .execute(&pool)
        .await
        .unwrap();
    repo::upsert(&pool, &bc, None, Some("BrandY"), None, None)
        .await
        .unwrap();
    let p2b = repo::get(&pool, &bc).await.unwrap().expect("cached");
    assert_eq!(p2b.brand.as_deref(), Some("BrandY"), "an empty field fills");

    // A user upload replaces ONLY the image, leaving metadata untouched.
    repo::set_image(&pool, &bc, &[9, 8, 7], "image/webp")
        .await
        .unwrap();
    let p3 = repo::get(&pool, &bc).await.unwrap().expect("cached");
    assert_eq!(p3.name.as_deref(), Some("Test Yog"), "name preserved");
    assert!(p3.has_image);
    let (bytes, mime) = repo::get_image(&pool, &bc).await.unwrap().expect("image");
    assert_eq!(bytes, vec![9, 8, 7]);
    assert_eq!(mime, "image/webp");

    // set_image on an unknown barcode creates a bare catalog row with the image.
    let fresh: Barcode = "9990000000002".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&fresh)
        .execute(&pool)
        .await
        .unwrap();
    repo::set_image(&pool, &fresh, &[1], "image/png")
        .await
        .unwrap();
    let pf = repo::get(&pool, &fresh).await.unwrap().expect("created");
    assert!(pf.name.is_none(), "no metadata, just an image");
    assert!(pf.has_image);
}

#[tokio::test]
async fn catalog_search_against_real_db() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    // Fixture rows, isolated by a prefix no other test uses.
    sqlx::query("DELETE FROM products WHERE barcode LIKE '99911%'")
        .execute(&pool)
        .await
        .unwrap();
    // Names/brands carry a token no real catalog row would ('yoghurtzz'), so
    // the assertions hold whatever else the shared DB contains.
    for (bc, name, brand) in [
        ("9991100000001", "Greek Style Yoghurtzz", "Fage"),
        ("9991100000002", "Natural Yoghurtzz 950g", "Yeo Valley"),
        ("9991100000003", "Oat Milk", "Oatlyzz"),
    ] {
        let bc: Barcode = bc.parse().unwrap();
        repo::upsert(&pool, &bc, Some(name), Some(brand), None, None)
            .await
            .unwrap();
    }

    // Name substring, case-insensitive (utf8mb4 collation), name-ordered.
    let yog = repo::search(&pool, "YOGHURTZZ", 20).await.unwrap();
    let names: Vec<_> = yog.iter().map(|p| p.name.as_deref().unwrap()).collect();
    assert_eq!(names, ["Greek Style Yoghurtzz", "Natural Yoghurtzz 950g"]);

    // Brand matches too.
    let oatly = repo::search(&pool, "oatlyzz", 20).await.unwrap();
    assert!(
        oatly.iter().any(|p| p.name.as_deref() == Some("Oat Milk")),
        "brand substring finds the row"
    );

    // LIKE metacharacters match literally, not as wildcards.
    assert!(
        repo::search(&pool, "yoghurt%z", 20)
            .await
            .unwrap()
            .is_empty(),
        "% is a literal, not a wildcard"
    );
    assert!(
        repo::search(&pool, "yoghurt_z", 20)
            .await
            .unwrap()
            .is_empty(),
        "_ is a literal, not a wildcard"
    );
}

#[tokio::test]
async fn external_import_against_real_db() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let source = Source::Waitrose;
    let ext: ExternalId = "TEST062593".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE source = ? AND external_id = ?")
        .bind(source)
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();

    // Miss on the (source, external_id) key.
    assert!(
        repo::get_by_source_external(&pool, source, &ext)
            .await
            .unwrap()
            .is_none()
    );

    // Import: a barcodeless shop product keyed by its external id.
    let p = repo::upsert_external(
        &pool,
        source,
        &ext,
        None, // no EAN — keyed by the shop's external id
        &repo::ListingFields {
            raw_name: Some("Cravendale Semi-Skimmed Milk"),
            brand: Some("Cravendale"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p.source, Some(Source::Waitrose));
    assert_eq!(
        p.external_id.as_ref().map(ExternalId::as_str),
        Some(ext.as_str())
    );
    assert_eq!(p.name.as_deref(), Some("Cravendale Semi-Skimmed Milk"));
    assert!(p.barcode.is_none(), "shop product has no barcode");
    assert!(!p.has_image);

    // Reachable by (source, external_id) and by surrogate id.
    assert_eq!(
        repo::get_by_source_external(&pool, source, &ext)
            .await
            .unwrap()
            .unwrap()
            .id,
        p.id
    );
    assert_eq!(
        repo::get_by_id(&pool, p.id).await.unwrap().unwrap().id,
        p.id
    );

    // A shop's pack size, promoted onto the canonical row and read back as an
    // amount. This composition is what POST /api/products/import performs: set
    // the label, then RE-READ, because the row it handed back a moment earlier
    // still says `pack: None` — and a caller that links stock from the returned
    // product would fill a form from that stale answer.
    assert!(p.pack.is_none(), "nothing was imported with a pack size");
    repo::set_quantity_label(&pool, p.id, "400G").await.unwrap();
    let measured = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(measured.quantity_label.as_deref(), Some("400G"));
    assert_eq!(
        measured.pack,
        Some(PackSize {
            value: 400.0,
            unit: PackUnit::Gram
        })
    );

    // Image is stored/served by id (there's no barcode to key it on).
    assert!(repo::get_image_by_id(&pool, p.id).await.unwrap().is_none());
    repo::set_image_by_id(&pool, p.id, &[7, 7, 7], "image/jpeg")
        .await
        .unwrap();
    let (bytes, mime) = repo::get_image_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(bytes, vec![7, 7, 7]);
    assert_eq!(mime, "image/jpeg");
    assert!(
        repo::get_by_id(&pool, p.id)
            .await
            .unwrap()
            .unwrap()
            .has_image
    );

    // Re-import is idempotent on the key and refreshes metadata.
    let p2 = repo::upsert_external(
        &pool,
        source,
        &ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Cravendale Whole Milk"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(p2.id, p.id, "same (source, external_id) → same row");
    assert_eq!(p2.name.as_deref(), Some("Cravendale Whole Milk"));
    assert!(p2.has_image, "re-import preserves the stored image");
}

#[tokio::test]
async fn a_stored_source_outside_the_enum_fails_the_read_loudly() {
    // `Source` decodes by parsing, so a `source` column holding something that
    // isn't a source is an error on the read rather than a value the rest of the
    // code has to second-guess. The row stays findable and repairable instead of
    // arriving as a silent default — the same policy the facts columns follow.
    //
    // This also guards the mapping itself: `#[derive(sqlx::Type)]` would declare
    // these columns as SQL `ENUM` while they are `VARCHAR`, which failed every
    // read of a real row.
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let ext: ExternalId = "TESTUNKNOWNSRC".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO products (source, external_id, name) VALUES ('tesco', ?, 'Ghost')")
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();

    let err = repo::get_by_source_external(&pool, Source::Waitrose, &ext).await;
    // Reading it by its own (bogus) source isn't expressible — that is the
    // point — so read the row the way the catalogue does, by id.
    assert!(err.is_ok(), "the miss on a real source is just a miss");
    let id: (u64,) = sqlx::query_as("SELECT id FROM products WHERE external_id = ?")
        .bind(&ext)
        .fetch_one(&pool)
        .await
        .unwrap();
    let read = repo::get_by_id(&pool, ProductId(id.0)).await;
    assert!(read.is_err(), "an unknown source must not decode silently");
    let msg = read.unwrap_err().to_string();
    assert!(
        msg.contains("tesco"),
        "the error names the bad value: {msg}"
    );

    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();
}
