//! Reading a stock row's history — the audit that has had three writers and no
//! reader since migration 0002 ("cheap now, impossible to backfill").
//!
//! Against a real MariaDB: the ordering, the location join and the
//! event-outside-the-enum failure are all SQL and none of them can be checked
//! without one.

mod common;

use life::db;
use life::inventory::repo;
use life::inventory::types::{ItemCategory, ItemEvent, LocationKind, NewItem, NewLocation};

fn loc(kind: LocationKind, name: &str, parent: Option<u64>) -> NewLocation {
    NewLocation {
        kind,
        name: name.into(),
        parent_id: parent,
        sort_order: 0,
        position: None,
    }
}

#[tokio::test]
async fn an_items_history_reads_back_newest_first_and_says_where() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-item-history";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM locations WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let house = repo::create_location(&pool, user, loc(LocationKind::House, "Home", None))
        .await
        .unwrap();
    let cupboard = repo::create_location(
        &pool,
        user,
        loc(LocationKind::Cupboard, "Cupboard", Some(house.id)),
    )
    .await
    .unwrap();
    let fridge = repo::create_location(
        &pool,
        user,
        loc(LocationKind::Fridge, "Fridge", Some(house.id)),
    )
    .await
    .unwrap();

    let item = repo::create_item(
        &pool,
        user,
        NewItem {
            name: "Greek Yoghurt".into(),
            category: ItemCategory::Food,
            quantity: Some(950.0),
            unit: Some("g".into()),
            expiry: None,
            location_id: Some(cupboard.id),
            barcode: None,
            product_id: None,
        },
    )
    .await
    .unwrap();

    repo::move_item(&pool, user, item.id, Some(fridge.id))
        .await
        .unwrap()
        .expect("moved");
    repo::use_item(&pool, user, item.id, 200.0, Some("g"))
        .await
        .unwrap()
        .expect("used");

    let history = repo::item_history(&pool, user, item.id).await.unwrap();

    // Newest first: the order you read a history in, and the order the three
    // things happened in reversed.
    assert_eq!(
        history.iter().map(|h| h.event).collect::<Vec<_>>(),
        vec![ItemEvent::Used, ItemEvent::Moved, ItemEvent::Added]
    );

    // The location is NAMED. "Moved" on its own says nothing worth reading, and
    // an id says nothing a person can read at all.
    assert_eq!(history[1].location.as_deref(), Some("Fridge"));
    assert_eq!(history[2].location.as_deref(), Some("Cupboard"));

    // A use is the one event carrying a DELTA — 200 went, it does not hold 200.
    assert_eq!(history[0].quantity, Some(200.0));
    assert_eq!(history[0].location.as_deref(), Some("Fridge"), "used where");

    // Strictly increasing ids down the list, which is what makes the tie-break
    // in the ORDER BY worth having: all three of these land inside the same
    // second, so `at` alone would leave the order to the engine.
    assert!(
        history[0].id > history[1].id && history[1].id > history[2].id,
        "same-second events still come back in the order they happened"
    );
    assert!(history[0].at >= history[2].at, "stamped, not zero");
}

#[tokio::test]
async fn a_history_is_only_ever_its_owners() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let owner = "test-user-history-owner";
    let other = "test-user-history-other";
    for u in [owner, other] {
        sqlx::query("DELETE FROM items WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .unwrap();
    }

    let item = repo::create_item(
        &pool,
        owner,
        NewItem {
            name: "Cumin".into(),
            category: ItemCategory::Food,
            quantity: Some(1.0),
            unit: Some("jar".into()),
            expiry: None,
            location_id: None,
            barcode: None,
            product_id: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        repo::item_history(&pool, owner, item.id)
            .await
            .unwrap()
            .len(),
        1
    );
    // Scoped on the history row's OWN user_id, not reached through the item —
    // so asking about somebody else's id is empty rather than a leak, and is
    // indistinguishable from an item that simply has no history.
    assert!(
        repo::item_history(&pool, other, item.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        repo::item_history(&pool, owner, 9_999_999)
            .await
            .unwrap()
            .is_empty(),
        "an id that does not exist is an empty history, not an error"
    );
}

#[tokio::test]
async fn a_stored_event_outside_the_enum_fails_the_read_loudly() {
    // The mirror of products_db's test for `source`. A VARCHAR(16) can hold
    // anything; a row we cannot name is a bug to see, not a blank line to
    // scroll past in a list whose whole job is to account for what happened.
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-history-bad-event";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let item = repo::create_item(
        &pool,
        user,
        NewItem {
            name: "Cumin".into(),
            category: ItemCategory::Food,
            quantity: None,
            unit: None,
            expiry: None,
            location_id: None,
            barcode: None,
            product_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE item_history SET event = 'teleported' WHERE item_id = ?")
        .bind(item.id)
        .execute(&pool)
        .await
        .unwrap();

    let err = repo::item_history(&pool, user, item.id)
        .await
        .expect_err("an unknown event must not read as anything");
    assert!(
        err.to_string().contains("teleported"),
        "the error says which value it could not read: {err}"
    );
}
