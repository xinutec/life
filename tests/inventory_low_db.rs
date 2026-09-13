//! "I am running out of this" against a real MariaDB. Runs only when
//! LIFE_TEST_DATABASE_URL is set; fails otherwise, because a skipped check on
//! the SQL reads as a passing one.
//!
//! The event exists because the MEASUREMENT never arrives: `used` shipped
//! 2026-07-31 and had not been written once six weeks later. Nobody logs pouring
//! milk; everybody puts the empty thing on the shopping list. These assert the
//! judgement is recorded, that it changes nothing else, and that recording it
//! twice is allowed — a repeated one is signal, not an error.

mod common;

use life::db;
use life::inventory::repo;
use life::inventory::types::{ItemCategory, NewItem};

async fn connect() -> sqlx::MySqlPool {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

fn milk() -> NewItem {
    NewItem {
        name: "Milk (semi-skimmed)".into(),
        category: ItemCategory::Food,
        quantity: Some(1.0),
        unit: Some("bottle".into()),
        expiry: None,
        expiry_precision: None,
        location_id: None,
        barcode: None,
        product_id: None,
        name_source: None,
    }
}

/// A user of this file's own, cleaned first — the house pattern in the sibling
/// tests, which share one database across files.
async fn fresh(pool: &sqlx::MySqlPool, user: &str) {
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(pool)
        .await
        .expect("clean");
}

async fn lows(pool: &sqlx::MySqlPool, item_id: u64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM item_history WHERE item_id = ? AND event = 'low'")
        .bind(item_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn records_the_judgement_without_touching_the_stock() {
    let pool = connect().await;
    let user = "test-user-low";
    fresh(&pool, user).await;
    let item = repo::create_item(&pool, user, milk())
        .await
        .expect("create");

    assert!(repo::mark_low(&pool, user, item.id).await.expect("mark"));

    assert_eq!(lows(&pool, item.id).await, 1, "one low event");
    let after = repo::get_item(&pool, user, item.id)
        .await
        .expect("get")
        .expect("live");
    // The whole point: this is a decision, not a measurement. Nothing moved.
    assert_eq!(after.quantity, Some(1.0), "quantity untouched");
}

#[tokio::test]
async fn the_same_judgement_twice_is_allowed() {
    // The rhythm is read from the GAPS between these, so a second one is a data
    // point and not a mistake to reject. A unique constraint here would throw
    // away exactly the signal the event was added for.
    let pool = connect().await;
    let user = "test-user-low-twice";
    fresh(&pool, user).await;
    let item = repo::create_item(&pool, user, milk())
        .await
        .expect("create");

    assert!(repo::mark_low(&pool, user, item.id).await.expect("first"));
    assert!(repo::mark_low(&pool, user, item.id).await.expect("second"));

    assert_eq!(lows(&pool, item.id).await, 2);
}

#[tokio::test]
async fn an_unknown_item_says_so_rather_than_writing_a_row() {
    let pool = connect().await;
    let user = "test-user-low-missing";
    assert!(
        !repo::mark_low(&pool, user, 9_999_999).await.expect("query"),
        "no such item"
    );
}
