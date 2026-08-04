//! "I used some of this" against a real MariaDB — the decrement, the audit row
//! it leaves, and the cases that must leave the cupboard alone. Runs only when
//! LIFE_TEST_DATABASE_URL is set; fails otherwise, because a skipped check on the SQL reads as a passing one.

mod common;

use life::db;
use life::inventory::consume::Taken;
use life::inventory::repo;
use life::inventory::types::{ItemCategory, NewItem};

async fn connect() -> sqlx::MySqlPool {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    pool
}

fn flour(quantity: Option<f64>, unit: Option<&str>) -> NewItem {
    NewItem {
        name: "Plain flour".into(),
        category: ItemCategory::Food,
        quantity,
        unit: unit.map(Into::into),
        expiry: None,
        location_id: None,
        barcode: None,
        product_id: None,
    }
}

/// How much this item has been recorded as using, and over how many events.
async fn used(pool: &sqlx::MySqlPool, item_id: u64) -> (i64, Option<f64>) {
    sqlx::query_as(
        "SELECT COUNT(*), SUM(quantity) FROM item_history WHERE item_id = ? AND event = 'used'",
    )
    .bind(item_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn using_some_decrements_the_row_and_records_what_went() {
    let pool = connect().await;
    let user = "test-user-use";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let item = repo::create_item(&pool, user, flour(Some(950.0), Some("g")))
        .await
        .unwrap();

    let (outcome, after) = repo::use_item(&pool, user, item.id, 200.0, Some("g"))
        .await
        .unwrap()
        .expect("the item exists");
    assert_eq!(outcome, Taken::Left(750.0));
    assert_eq!(after.expect("still there").quantity, Some(750.0));

    // The audit records the DELTA, not the new level: "200g went" is the fact a
    // consumption rate is computed from later, and it survives a hand edit that
    // resets the quantity.
    assert_eq!(used(&pool, item.id).await, (1, Some(200.0)));

    // Twice over, so the history accumulates rather than replacing.
    repo::use_item(&pool, user, item.id, 50.0, Some("g"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(used(&pool, item.id).await, (2, Some(250.0)));
    assert_eq!(
        repo::get_item(&pool, user, item.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Some(700.0)
    );
}

#[tokio::test]
async fn using_the_last_of_it_leaves_a_zero_row_not_a_hole() {
    let pool = connect().await;
    let user = "test-user-use-empty";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let item = repo::create_item(&pool, user, flour(Some(200.0), Some("g")))
        .await
        .unwrap();
    let (outcome, after) = repo::use_item(&pool, user, item.id, 500.0, Some("g"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome, Taken::Emptied { short: 300.0 });

    let after = after.expect("the row is KEPT — 'we have none' is worth knowing");
    assert_eq!(after.quantity, Some(0.0));
    // Only what was actually there is recorded as used; the 300g that wasn't
    // there was never in the cupboard to be consumed.
    assert_eq!(used(&pool, item.id).await, (1, Some(200.0)));
}

#[tokio::test]
async fn a_mismatched_unit_leaves_the_cupboard_exactly_as_it_was() {
    let pool = connect().await;
    let user = "test-user-use-mismatch";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let item = repo::create_item(&pool, user, flour(Some(1.0), Some("jar")))
        .await
        .unwrap();
    let (outcome, after) = repo::use_item(&pool, user, item.id, 200.0, Some("g"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome, Taken::UnitMismatch);
    assert_eq!(after.unwrap().quantity, Some(1.0), "untouched");
    assert_eq!(used(&pool, item.id).await, (0, None), "and no audit row");

    // Same for a row that tracks no quantity at all.
    let vague = repo::create_item(&pool, user, flour(None, None))
        .await
        .unwrap();
    let (outcome, _) = repo::use_item(&pool, user, vague.id, 1.0, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(outcome, Taken::Untracked);
    assert_eq!(used(&pool, vague.id).await, (0, None));
}

#[tokio::test]
async fn you_cannot_use_someone_elses_stock() {
    let pool = connect().await;
    let (mine, theirs) = ("test-user-use-mine", "test-user-use-theirs");
    for u in [mine, theirs] {
        sqlx::query("DELETE FROM items WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .unwrap();
    }
    let item = repo::create_item(&pool, theirs, flour(Some(950.0), Some("g")))
        .await
        .unwrap();
    assert!(
        repo::use_item(&pool, mine, item.id, 200.0, Some("g"))
            .await
            .unwrap()
            .is_none(),
        "another user's row is invisible, not merely unwritable"
    );
    assert_eq!(
        repo::get_item(&pool, theirs, item.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Some(950.0)
    );
}
