//! The Buy list's REST reads against a real MariaDB. Rows arrive by sync, as
//! the app writes them; `sync_db.rs` covers the protocol itself.

mod common;

use life::db;
use life::shopping::repo;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, ShoppingDoc};

fn row(ulid: &str, name: &str, done: bool) -> PushEntry<ShoppingDoc> {
    PushEntry {
        new_document_state: ShoppingDoc {
            ulid: ulid.into(),
            id: None,
            name: name.into(),
            quantity: None,
            unit: None,
            barcode: None,
            category: "food".parse().unwrap(),
            product_id: None,
            done,
            deleted: false,
            rev: 0,
        },
        assumed_master_state: None,
    }
}

#[tokio::test]
async fn the_list_puts_what_is_still_to_buy_first_and_hides_what_went() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-shopping";
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Pushed in neither order the list uses.
    sync_repo::push_shopping(
        &pool,
        user,
        vec![
            row("01SHOPLISTAAAAAAAAAAAAAAAA", "Apples", true),
            row("01SHOPLISTBBBBBBBBBBBBBBBB", "Yoghurt", false),
            row("01SHOPLISTCCCCCCCCCCCCCCCC", "Bread", false),
        ],
    )
    .await
    .unwrap();
    let names = |list: &[life::shopping::types::ShoppingItem]| {
        list.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
    };
    let list = repo::list(&pool, user).await.unwrap();
    assert_eq!(names(&list), ["Bread", "Yoghurt", "Apples"]);

    // A removed row (the buy's first step) leaves the list and the reads.
    let bread = list[0].id;
    assert!(repo::delete(&pool, user, bread).await.unwrap());
    assert!(repo::get(&pool, user, bread).await.unwrap().is_none());
    assert_eq!(
        names(&repo::list(&pool, user).await.unwrap()),
        ["Yoghurt", "Apples"]
    );
    assert!(
        !repo::delete(&pool, user, bread).await.unwrap(),
        "only once"
    );
}
