//! Shopping list against a real MariaDB.

mod common;

use life::db;
use life::inventory::types::ItemCategory;
use life::shopping::repo;
use life::shopping::types::{NewShoppingItem, UpdateShoppingItem};

#[tokio::test]
async fn shopping_crud_against_real_db() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-shopping";
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Add a couple of things to buy. Yoghurt takes the food default; the
    // batteries say what they are.
    let yog = repo::create(
        &pool,
        user,
        NewShoppingItem {
            name: "Yoghurt".into(),
            quantity: Some(1.0),
            unit: Some("kg".into()),
            barcode: None,
            category: ItemCategory::Food,
            product_id: None,
        },
    )
    .await
    .unwrap();
    let batteries = repo::create(
        &pool,
        user,
        NewShoppingItem {
            name: "Batteries".into(),
            quantity: None,
            unit: None,
            barcode: None,
            category: ItemCategory::Tool,
            product_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(repo::list(&pool, user).await.unwrap().len(), 2);
    assert!(!yog.done);
    assert_eq!(batteries.category, ItemCategory::Tool);

    // Tick one off (done toggle via full update).
    let toggled = repo::update(
        &pool,
        user,
        yog.id,
        UpdateShoppingItem {
            name: yog.name.clone(),
            quantity: yog.quantity,
            unit: yog.unit.clone(),
            barcode: None,
            category: yog.category,
            product_id: None,
            done: true,
        },
    )
    .await
    .unwrap()
    .expect("exists");
    assert!(toggled.done);

    // Delete leaves the other row alone. Buying is the route's (it deletes and
    // creates in one request), tested through it in signed_in_http_db.rs.
    assert!(repo::delete(&pool, user, yog.id).await.unwrap());
    assert!(repo::get(&pool, user, yog.id).await.unwrap().is_none());
    assert_eq!(repo::list(&pool, user).await.unwrap().len(), 1);
}
