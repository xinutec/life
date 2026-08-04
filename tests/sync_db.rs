//! Offline-first sync (shopping) against a real MariaDB. Runs only when
//! LIFE_TEST_DATABASE_URL is set; fails otherwise, because a skipped check on the SQL reads as a passing one.

mod common;

use life::db;
use life::inventory::types::ItemCategory;
use life::shopping::repo as shop;
use life::shopping::types::NewShoppingItem;
use life::sync::repo as sync;
use life::sync::types::{PushEntry, ShoppingDoc};
use ulid::Ulid;

fn doc(ulid: &str, name: &str, rev: u64) -> ShoppingDoc {
    ShoppingDoc {
        ulid: ulid.to_string(),
        id: None,
        name: name.to_string(),
        quantity: None,
        unit: None,
        barcode: None,
        category: "food".into(),
        product_id: None,
        done: false,
        deleted: false,
        rev,
    }
}

#[tokio::test]
async fn shopping_sync_pull_push_conflict_tombstone() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-sync";
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // A legacy create is rev-aware → it shows up in a full pull (since 0).
    let milk = shop::create(
        &pool,
        user,
        NewShoppingItem {
            name: "Milk".into(),
            quantity: Some(2.0),
            unit: Some("L".into()),
            barcode: None,
            category: ItemCategory::Food,
            product_id: None,
        },
    )
    .await
    .unwrap();

    let p1 = sync::pull_shopping(&pool, user, 0, 100).await.unwrap();
    assert_eq!(p1.documents.len(), 1);
    let m = p1.documents[0].clone();
    assert_eq!(m.name, "Milk");
    assert!(!m.deleted);
    assert!(m.rev > 0);
    assert_eq!(p1.checkpoint.rev, m.rev);

    // Pulling from the checkpoint yields nothing new.
    let p2 = sync::pull_shopping(&pool, user, p1.checkpoint.rev, 100)
        .await
        .unwrap();
    assert!(p2.documents.is_empty());
    assert_eq!(p2.checkpoint.rev, p1.checkpoint.rev);

    // Push a fresh client-created doc (no assumed master) → inserted, no conflict.
    let eggs_ulid = Ulid::new().to_string();
    let conflicts = sync::push_shopping(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(&eggs_ulid, "Eggs", 0),
            assumed_master_state: None,
        }],
    )
    .await
    .unwrap();
    assert!(conflicts.is_empty());
    let p3 = sync::pull_shopping(&pool, user, p2.checkpoint.rev, 100)
        .await
        .unwrap();
    assert!(p3.documents.iter().any(|d| d.name == "Eggs" && !d.deleted));

    // Stale update of Milk (wrong assumed rev) → rejected; the current master is
    // returned so the client can resolve.
    let stale = sync::push_shopping(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(&m.ulid, "Milk 2%", 0),
            assumed_master_state: Some(doc(&m.ulid, "Milk", m.rev - 1)),
        }],
    )
    .await
    .unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].ulid, m.ulid);
    assert_eq!(stale[0].rev, m.rev); // unchanged — the stale write was not applied
    assert_eq!(stale[0].name, "Milk");

    // Correct assumed rev → accepted.
    let ok = sync::push_shopping(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(&m.ulid, "Milk 2%", 0),
            assumed_master_state: Some(doc(&m.ulid, "Milk", m.rev)),
        }],
    )
    .await
    .unwrap();
    assert!(ok.is_empty());
    let after = sync::pull_shopping(&pool, user, 0, 100).await.unwrap();
    let m2 = after.documents.iter().find(|d| d.ulid == m.ulid).unwrap();
    assert_eq!(m2.name, "Milk 2%");
    assert!(m2.rev > m.rev); // a new revision was assigned

    // A legacy soft-delete surfaces as a tombstone in pull, and hides from list.
    assert!(shop::delete(&pool, user, milk.id).await.unwrap());
    let final_pull = sync::pull_shopping(&pool, user, 0, 100).await.unwrap();
    let tomb = final_pull
        .documents
        .iter()
        .find(|d| d.ulid == m.ulid)
        .unwrap();
    assert!(tomb.deleted);
    assert!(
        shop::list(&pool, user)
            .await
            .unwrap()
            .iter()
            .all(|s| s.id != milk.id)
    );
}

/// The boot-time backfills — `sync::backfill`, run on **every** start, and until
/// now the only sync path no test walked. Both halves are "fix rows that predate
/// a rule", so both are tested the only way that means anything: create the
/// pre-rule shape by raw SQL, run the real entry point, and check the rows the
/// clients would then pull.
#[tokio::test]
async fn boot_backfill_gives_pre_sync_rows_an_identity() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-backfill";
    sqlx::query("DELETE FROM shopping_items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // A row as it looked before sync existed: no ulid, no rev. A client can't
    // see it at all until the backfill gives it an identity.
    sqlx::query(
        "INSERT INTO shopping_items (user_id, name, category, done, rev, created_at, updated_at) \
         VALUES (?, 'Pre-sync oats', 'food', 0, 0, NOW(), NOW())",
    )
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        life::sync::backfill(&pool).await.is_ok(),
        "backfill runs at boot; it must never fail on a legacy row"
    );

    let pulled = sync::pull_shopping(&pool, user, 0, 100).await.unwrap();
    let row = pulled
        .documents
        .iter()
        .find(|d| d.name == "Pre-sync oats")
        .expect("the backfilled row is now pullable");
    assert_eq!(row.ulid.len(), 26, "a real ULID, not a placeholder");
    assert!(row.rev > 0, "and a revision, so it propagates");

    // Idempotent: it runs on every boot, so a second pass must not churn revs
    // (which would re-push every row to every device, forever).
    let before = row.rev;
    life::sync::backfill(&pool).await.unwrap();
    let again = sync::pull_shopping(&pool, user, 0, 100).await.unwrap();
    let same = again
        .documents
        .iter()
        .find(|d| d.name == "Pre-sync oats")
        .unwrap();
    assert_eq!(same.ulid, row.ulid, "identity is assigned once");
    assert_eq!(same.rev, before, "a clean second pass is a no-op");
}

/// The other half of the boot backfill: duplicate live edges (made before the
/// push-time twin guard, or by a race) are tombstoned down to one. Deliberately
/// asserts WHICH survives — "the lowest id" is the rule that makes the cleanup
/// deterministic across devices.
#[tokio::test]
async fn boot_backfill_tombstones_duplicate_todo_links() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-link-dedupe";
    sqlx::query("DELETE FROM todo_links WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Two live edges saying the same thing under different ulids.
    let from = Ulid::new().to_string();
    let (first, second) = (Ulid::new().to_string(), Ulid::new().to_string());
    for ulid in [&first, &second] {
        sqlx::query(
            "INSERT INTO todo_links \
             (user_id, from_ulid, kind, target_kind, target_ref, ulid, rev, created_at) \
             VALUES (?, ?, 'blocks', 'todo', 'target-ulid', ?, 1, NOW())",
        )
        .bind(user)
        .bind(&from)
        .bind(ulid)
        .execute(&pool)
        .await
        .unwrap();
    }

    life::sync::backfill(&pool).await.unwrap();

    let pulled = sync::pull_todo_link(&pool, user, 0, 100).await.unwrap();
    let live: Vec<&str> = pulled
        .documents
        .iter()
        .filter(|d| !d.deleted)
        .map(|d| d.ulid.as_str())
        .collect();
    assert_eq!(
        live,
        [first.as_str()],
        "the older edge is the one that stays"
    );
    let tombstoned = pulled
        .documents
        .iter()
        .find(|d| d.ulid == second)
        .expect("the duplicate is tombstoned, not deleted outright");
    assert!(tombstoned.deleted);
    assert!(
        tombstoned.rev > 1,
        "a fresh rev, so the delete reaches every device"
    );
}
