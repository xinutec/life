//! The catalog/holding split against a real MariaDB: an item linked to a
//! catalog product resolves its display name/brand/image from the product;
//! a barcode-less item stands alone on its own name. Gated on
//! LIFE_TEST_DATABASE_URL.

use life::db;
use life::inventory::repo as inv;
use life::inventory::types::{ItemCategory, NewItem};
use life::products::repo as prod;

fn new_item(name: &str, barcode: Option<&str>) -> NewItem {
    NewItem {
        name: name.to_string(),
        category: ItemCategory::Food,
        quantity: Some(1.0),
        unit: None,
        expiry: None,
        location_id: None,
        barcode: barcode.map(Into::into),
        product_id: None,
    }
}

#[tokio::test]
async fn item_resolves_through_catalog_product() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping catalog DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "catalog-test-user";
    let barcode = "cat-test-9999";
    // Clean any prior run.
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();

    // A catalog product (as the OFF lookup would cache it), with an image.
    prod::upsert(
        &pool,
        barcode,
        Some("Catalog Yoghurt"),
        Some("BrandY"),
        Some("950g"),
        Some((vec![1, 2, 3], "image/png".into())),
    )
    .await
    .unwrap();

    // An item scanned to that barcode — its own name is a scribble that the
    // product name should override on read.
    let linked = inv::create_item(&pool, user, new_item("scribble", Some(barcode)))
        .await
        .unwrap();
    assert!(
        linked.product_id.is_some(),
        "barcoded item links to the catalog product"
    );
    assert_eq!(
        linked.name, "Catalog Yoghurt",
        "display name comes from the product"
    );
    assert_eq!(linked.brand.as_deref(), Some("BrandY"));
    assert_eq!(linked.barcode.as_deref(), Some(barcode));
    assert!(linked.has_image, "product image surfaces on the item");

    // A barcode-less one-off stands alone on its own name.
    let loose = inv::create_item(&pool, user, new_item("Loose soup", None))
        .await
        .unwrap();
    assert!(loose.product_id.is_none());
    assert_eq!(loose.name, "Loose soup");
    assert!(!loose.has_image);

    // Both appear in the complete list, resolved.
    let all = inv::list_items(&pool, user).await.unwrap();
    let names: Vec<_> = all.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Catalog Yoghurt"));
    assert!(names.contains(&"Loose soup"));
}

#[tokio::test]
async fn item_links_to_barcodeless_shop_product_by_id() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping catalog DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "catalog-shoplink-user";
    let external_id = "cat-test-ln-777777";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM products WHERE source = 'waitrose' AND external_id = ?")
        .bind(external_id)
        .execute(&pool)
        .await
        .unwrap();

    // A shop product has no EAN barcode — only the shop's own external id.
    let product = prod::upsert_external(
        &pool,
        "waitrose",
        external_id,
        Some("Waitrose Cheddar"),
        Some("Waitrose"),
        None,
    )
    .await
    .unwrap();
    prod::set_image_by_id(&pool, product.id, &[9, 8, 7], "image/jpeg")
        .await
        .unwrap();

    // Linking is only reachable via an explicit product_id — there's no barcode.
    let mut req = new_item("my scribble", None);
    req.product_id = Some(product.id);
    let linked = inv::create_item(&pool, user, req).await.unwrap();
    assert_eq!(linked.product_id, Some(product.id), "explicit link honoured");
    assert_eq!(
        linked.name, "Waitrose Cheddar",
        "display name resolves from the shop product"
    );
    assert_eq!(linked.brand.as_deref(), Some("Waitrose"));
    assert!(
        linked.barcode.is_none(),
        "shop product carries no barcode to surface"
    );
    assert!(linked.has_image, "shop image surfaces on the linked item");

    // Re-linking via update keeps the association.
    let mut upd = new_item("my scribble", None);
    upd.product_id = Some(product.id);
    let updated = inv::update_item(&pool, user, linked.id, upd)
        .await
        .unwrap()
        .expect("item exists");
    assert_eq!(updated.product_id, Some(product.id));
    assert_eq!(updated.name, "Waitrose Cheddar");
}
