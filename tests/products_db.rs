//! Product cache against a real MariaDB (no Open Food Facts call — pure cache
//! layer). Runs only when LIFE_TEST_DATABASE_URL is set.

use life::db;
use life::products::repo;

#[tokio::test]
async fn product_cache_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping products DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let bc = "test-barcode-0001";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(bc)
        .execute(&pool)
        .await
        .unwrap();

    // Miss.
    assert!(repo::get(&pool, bc).await.unwrap().is_none());

    // Cache with an image.
    repo::upsert(
        &pool,
        bc,
        Some("Test Yog"),
        Some("BrandX"),
        Some("950g"),
        Some((vec![1, 2, 3, 4], "image/png".into())),
    )
    .await
    .unwrap();
    let p = repo::get(&pool, bc).await.unwrap().expect("cached");
    assert_eq!(p.name.as_deref(), Some("Test Yog"));
    assert_eq!(p.quantity_label.as_deref(), Some("950g"));
    assert!(p.has_image);

    let (bytes, mime) = repo::get_image(&pool, bc).await.unwrap().expect("image");
    assert_eq!(bytes, vec![1, 2, 3, 4]);
    assert_eq!(mime, "image/png");

    // Re-cache without an image overwrites in place.
    repo::upsert(&pool, bc, Some("Test Yog 2"), None, None, None)
        .await
        .unwrap();
    let p2 = repo::get(&pool, bc).await.unwrap().expect("cached");
    assert_eq!(p2.name.as_deref(), Some("Test Yog 2"));
    assert!(!p2.has_image);

    // A user upload replaces ONLY the image, leaving metadata untouched.
    repo::set_image(&pool, bc, &[9, 8, 7], "image/webp")
        .await
        .unwrap();
    let p3 = repo::get(&pool, bc).await.unwrap().expect("cached");
    assert_eq!(p3.name.as_deref(), Some("Test Yog 2"), "name preserved");
    assert!(p3.has_image);
    let (bytes, mime) = repo::get_image(&pool, bc).await.unwrap().expect("image");
    assert_eq!(bytes, vec![9, 8, 7]);
    assert_eq!(mime, "image/webp");

    // set_image on an unknown barcode creates a bare catalog row with the image.
    let fresh = "test-barcode-upload-only";
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(fresh)
        .execute(&pool)
        .await
        .unwrap();
    repo::set_image(&pool, fresh, &[1], "image/png")
        .await
        .unwrap();
    let pf = repo::get(&pool, fresh).await.unwrap().expect("created");
    assert!(pf.name.is_none(), "no metadata, just an image");
    assert!(pf.has_image);
}

#[tokio::test]
async fn external_import_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping external-import DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let (source, ext) = ("waitrose", "TEST062593");
    sqlx::query("DELETE FROM products WHERE source = ? AND external_id = ?")
        .bind(source)
        .bind(ext)
        .execute(&pool)
        .await
        .unwrap();

    // Miss on the (source, external_id) key.
    assert!(
        repo::get_by_source_external(&pool, source, ext)
            .await
            .unwrap()
            .is_none()
    );

    // Import: a barcodeless shop product keyed by its external id.
    let p = repo::upsert_external(
        &pool,
        source,
        ext,
        Some("Cravendale Semi-Skimmed Milk"),
        Some("Cravendale"),
        Some("Milk"),
    )
    .await
    .unwrap();
    assert_eq!(p.source.as_deref(), Some("waitrose"));
    assert_eq!(p.external_id.as_deref(), Some(ext));
    assert_eq!(p.name.as_deref(), Some("Cravendale Semi-Skimmed Milk"));
    assert!(p.barcode.is_none(), "shop product has no barcode");
    assert!(!p.has_image);

    // Reachable by (source, external_id) and by surrogate id.
    assert_eq!(
        repo::get_by_source_external(&pool, source, ext)
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
        ext,
        Some("Cravendale Whole Milk"),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(p2.id, p.id, "same (source, external_id) → same row");
    assert_eq!(p2.name.as_deref(), Some("Cravendale Whole Milk"));
    assert!(p2.has_image, "re-import preserves the stored image");
}
