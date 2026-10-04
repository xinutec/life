//! An item change and its history row commit together or not at all. Real
//! MariaDB, with a trigger that refuses this test user's history rows, so every
//! write below fails at its history step: whatever it changed before that must
//! roll back with it.

mod common;

use life::db;
use life::inventory::repo;
use life::inventory::types::{ItemCategory, NewItem};

const USER: &str = "test-user-history-refused";

/// Both tests create and drop triggers on `item_history`, and DDL on a table
/// deadlocks against open transactions on it (a metadata-lock deadlock, error
/// 1213 with nothing in the InnoDB report): 1 run in about 10 failed. Held by
/// each test for its whole run.
static DDL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn jar(location_id: Option<u64>) -> NewItem {
    NewItem {
        name: "Jar".into(),
        category: ItemCategory::Food,
        quantity: None,
        unit: None,
        expiry: None,
        expiry_precision: None,
        location_id,
        barcode: None,
        product_id: None,
        name_source: None,
    }
}

async fn location(pool: &sqlx::MySqlPool, name: &str) -> u64 {
    sqlx::query("INSERT INTO locations (user_id, kind, name) VALUES (?, 'room', ?)")
        .bind(USER)
        .bind(name)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_id()
}

async fn refuse_history(pool: &sqlx::MySqlPool, on: bool) {
    sqlx::query("DROP TRIGGER IF EXISTS refuse_test_history")
        .execute(pool)
        .await
        .unwrap();
    if on {
        sqlx::query(
            "CREATE TRIGGER refuse_test_history BEFORE INSERT ON item_history FOR EACH ROW \
             IF NEW.user_id = 'test-user-history-refused' THEN \
             SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'history refused by the test'; \
             END IF",
        )
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn live_items(pool: &sqlx::MySqlPool) -> Vec<(u64, Option<u64>)> {
    sqlx::query_as("SELECT id, location_id FROM items WHERE user_id = ? AND deleted_at IS NULL")
        .bind(USER)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn no_item_change_outlives_its_history_row() {
    let _ddl = DDL.lock().await;
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    for sql in [
        "DELETE FROM item_history WHERE user_id = ?",
        "DELETE FROM items WHERE user_id = ?",
        "DELETE FROM locations WHERE user_id = ?",
    ] {
        sqlx::query(sql).bind(USER).execute(&pool).await.unwrap();
    }
    let kitchen = location(&pool, "Kitchen").await;
    let hall = location(&pool, "Hall").await;
    refuse_history(&pool, false).await;
    let item = repo::create_item(&pool, USER, jar(Some(kitchen)))
        .await
        .expect("create while history is accepted");
    let gone = repo::create_item(&pool, USER, jar(None)).await.unwrap();
    assert!(repo::delete_item(&pool, USER, gone.id).await.unwrap());

    refuse_history(&pool, true).await;
    let created = repo::create_item(&pool, USER, jar(None)).await;
    let moved = repo::move_item(&pool, USER, item.id, Some(hall)).await;
    let edited = repo::update_item(&pool, USER, item.id, jar(Some(hall))).await;
    let deleted = repo::delete_item(&pool, USER, item.id).await;
    let restored = repo::restore_item(&pool, USER, gone.id).await;
    let low = repo::mark_low(&pool, USER, item.id).await;
    refuse_history(&pool, false).await;

    for (what, failed) in [
        ("create", created.is_err()),
        ("move", moved.is_err()),
        ("edit", edited.is_err()),
        ("delete", deleted.is_err()),
        ("restore", restored.is_err()),
        ("low", low.is_err()),
    ] {
        assert!(failed, "{what} must report the refused history row");
    }
    assert_eq!(
        live_items(&pool).await,
        [(item.id, Some(kitchen))],
        "nothing created, moved, deleted or restored without its history"
    );
    assert!(
        repo::move_item(&pool, USER, item.id, Some(kitchen))
            .await
            .unwrap()
            .is_some(),
        "moving a thing to where it already is is not \"no such item\""
    );
}

/// Buying a Buy-list row takes it off the list and puts it in the cupboard. If
/// the cupboard half fails, the row must still be on the list: otherwise the
/// thing is neither to buy nor owned.
#[tokio::test]
async fn a_buy_that_cannot_record_its_item_leaves_the_row_on_the_list() {
    let _ddl = DDL.lock().await;
    const BUYER: &str = "test-user-history-refused-buy";
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    for sql in [
        "DELETE FROM shopping_items WHERE user_id = ?",
        "DELETE FROM items WHERE user_id = ?",
    ] {
        sqlx::query(sql).bind(BUYER).execute(&pool).await.unwrap();
    }
    life::sync::repo::push_shopping(
        &pool,
        BUYER,
        vec![life::sync::types::PushEntry {
            new_document_state: life::sync::types::ShoppingDoc {
                ulid: "01BUYATOMICAAAAAAAAAAAAAAA".into(),
                id: None,
                name: "Milk".into(),
                quantity: None,
                unit: None,
                barcode: None,
                category: "food".into(),
                product_id: None,
                done: false,
                deleted: false,
                rev: 0,
            },
            assumed_master_state: None,
        }],
    )
    .await
    .unwrap();
    let row = life::shopping::repo::list(&pool, BUYER).await.unwrap()[0].id;

    sqlx::query("DROP TRIGGER IF EXISTS refuse_test_buy_history")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse_test_buy_history BEFORE INSERT ON item_history FOR EACH ROW \
         IF NEW.user_id = 'test-user-history-refused-buy' THEN \
         SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'history refused by the test'; \
         END IF",
    )
    .execute(&pool)
    .await
    .unwrap();
    let bought = life::shopping::repo::buy(&pool, BUYER, row).await;
    sqlx::query("DROP TRIGGER refuse_test_buy_history")
        .execute(&pool)
        .await
        .unwrap();

    assert!(bought.is_err(), "the refused history must fail the buy");
    let list = life::shopping::repo::list(&pool, BUYER).await.unwrap();
    assert_eq!(list.len(), 1, "the row is still to buy");
    let (items,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM items WHERE user_id = ?")
        .bind(BUYER)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 0, "and nothing half-arrived in the cupboard");

    // With history accepted, the same row buys once.
    let item = life::shopping::repo::buy(&pool, BUYER, row).await.unwrap();
    assert_eq!(item.expect("bought").name, "Milk");
    assert!(
        life::shopping::repo::buy(&pool, BUYER, row)
            .await
            .unwrap()
            .is_none()
    );
}
