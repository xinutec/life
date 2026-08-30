//! Purchases against a real MariaDB: what was paid, where, and whether it can
//! still be found after the catalogue changes underneath it.

mod common;

use life::db;
use life::products::ids::ProductId;
use life::purchases::repo::{self, BoughtItem};
use life::purchases::types::NewPurchase;

async fn product(pool: &sqlx::MySqlPool, barcode: &str, name: &str) -> ProductId {
    sqlx::query(
        "INSERT INTO products (barcode, name) VALUES (?, ?) \
         ON DUPLICATE KEY UPDATE name = VALUES(name)",
    )
    .bind(barcode)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert product");
    let (id,): (u64,) = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
        .bind(barcode)
        .fetch_one(pool)
        .await
        .expect("product id");
    ProductId::from(id)
}

fn paid(shop: &str, amount_minor: i64) -> NewPurchase {
    NewPurchase {
        shop: shop.into(),
        amount_minor,
        currency: "GBP".into(),
    }
}

#[tokio::test]
async fn a_purchase_survives_being_relinked_to_a_different_product() {
    // The reason a purchase carries barcode AND product id AND name. An item can
    // be linked to the WRONG product with every barcode agreeing (found on the
    // live data 2026-08-30, #1281), so relinking is a normal correction — and it
    // must not orphan what was spent.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-purchases";
    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    let wrong = product(&pool, "T-BUY-1", "Organic Honey").await;
    let item = BoughtItem {
        product_id: Some(wrong),
        barcode: Some("T-BUY-1"),
        name: "Blue dragon Oyster Sauce",
        quantity: Some(150.0),
        unit: Some("ml"),
    };
    repo::record(&pool, user, &item, &paid("Waitrose", 330))
        .await
        .expect("record");

    let by_product = repo::history(&pool, user, Some(wrong), None)
        .await
        .expect("history");
    assert_eq!(by_product.len(), 1);
    assert_eq!(by_product[0].amount_minor, 330);
    assert_eq!(by_product[0].shop, "Waitrose");
    assert_eq!(
        by_product[0].name, "Blue dragon Oyster Sauce",
        "the name it was bought under is kept, not the catalogue's"
    );

    // The correction: the link was wrong, so it goes away.
    sqlx::query("UPDATE purchases SET product_id = NULL WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("unlink");

    let by_barcode = repo::history(&pool, user, None, Some("T-BUY-1"))
        .await
        .expect("history by barcode");
    assert_eq!(
        by_barcode.len(),
        1,
        "the barcode must still find a purchase whose product link was removed"
    );
    assert_eq!(by_barcode[0].amount_minor, 330);

    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}

#[tokio::test]
async fn deleting_a_product_keeps_the_purchase() {
    // ON DELETE SET NULL, never CASCADE. price_observations cascades because a
    // scrape series is meaningless without its listing; spending history is not,
    // and a catalogue tidy-up must never be able to erase it.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-purchases-cascade";
    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    let pid = product(&pool, "T-BUY-2", "Doomed Product").await;
    let item = BoughtItem {
        product_id: Some(pid),
        barcode: Some("T-BUY-2"),
        name: "Doomed Product",
        quantity: None,
        unit: None,
    };
    repo::record(&pool, user, &item, &paid("Asda", 199))
        .await
        .expect("record");

    sqlx::query("DELETE FROM products WHERE id = ?")
        .bind(pid)
        .execute(&pool)
        .await
        .expect("delete product");

    let left = repo::history(&pool, user, None, Some("T-BUY-2"))
        .await
        .expect("history");
    assert_eq!(left.len(), 1, "the purchase must outlive the product");
    assert!(
        left[0].product_id.is_none(),
        "its link is cleared, not cascaded"
    );

    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}

#[tokio::test]
async fn a_nonsense_price_is_refused_rather_than_stored() {
    // The whole value of this table is that its numbers are true. A stored
    // nonsense figure is indistinguishable from a real one afterwards.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-purchases-bad";
    let item = BoughtItem {
        product_id: None,
        barcode: None,
        name: "Something",
        quantity: None,
        unit: None,
    };

    assert!(
        repo::record(&pool, user, &item, &paid("Waitrose", -1))
            .await
            .is_err(),
        "a negative price is a bug or a fat finger, not a refund"
    );
    assert!(
        repo::record(&pool, user, &item, &paid("   ", 100))
            .await
            .is_err(),
        "a purchase with no shop cannot answer the question this table exists for"
    );
    let bad_currency = NewPurchase {
        shop: "Waitrose".into(),
        amount_minor: 100,
        currency: "pounds".into(),
    };
    assert!(
        repo::record(&pool, user, &item, &bad_currency)
            .await
            .is_err(),
        "currency must be ISO 4217, or amounts cannot be compared at all"
    );

    let none = repo::history(&pool, user, None, None)
        .await
        .expect("history");
    assert!(none.is_empty(), "nothing was stored");
}

#[tokio::test]
async fn a_purchase_is_only_its_owners() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let mine = "test-user-purchases-mine";
    let yours = "test-user-purchases-yours";
    for u in [mine, yours] {
        sqlx::query("DELETE FROM purchases WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .expect("clean");
    }

    let item = BoughtItem {
        product_id: None,
        barcode: Some("T-BUY-3"),
        name: "Tea",
        quantity: None,
        unit: None,
    };
    repo::record(&pool, mine, &item, &paid("Waitrose", 250))
        .await
        .expect("record");

    let theirs = repo::history(&pool, yours, None, Some("T-BUY-3"))
        .await
        .expect("history");
    assert!(
        theirs.is_empty(),
        "what someone paid is nobody else's business"
    );

    for u in [mine, yours] {
        sqlx::query("DELETE FROM purchases WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .expect("clean up");
    }
}
