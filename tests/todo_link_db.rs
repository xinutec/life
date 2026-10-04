//! To-do connections against a real MariaDB, through sync: the only way the
//! app writes or reads them.

mod common;

use life::db;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, TodoLinkDoc};

fn link(ulid: &str, from: &str, kind: &str, target_kind: &str, target_ref: &str) -> TodoLinkDoc {
    TodoLinkDoc {
        ulid: ulid.into(),
        id: None,
        from: from.into(),
        kind: kind.into(),
        target_kind: target_kind.into(),
        target_ref: target_ref.into(),
        deleted: false,
        rev: 0,
    }
}

fn fresh(doc: TodoLinkDoc) -> PushEntry<TodoLinkDoc> {
    PushEntry {
        new_document_state: doc,
        assumed_master_state: None,
    }
}

/// The links a device pulling from scratch would show.
async fn live(pool: &sqlx::MySqlPool, user: &str) -> Vec<TodoLinkDoc> {
    sync_repo::pull_todo_link(pool, user, 0, 100)
        .await
        .unwrap()
        .documents
        .into_iter()
        .filter(|d| !d.deleted)
        .collect()
}

#[tokio::test]
async fn links_are_added_removed_and_pulled_with_their_tombstones() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-todo-link";
    sqlx::query("DELETE FROM todo_links WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let from = "01TODOAAAAAAAAAAAAAAAAAAAA";
    // A depends-on link to another to-do, and a related link to an inventory item.
    let dep = link(
        "01LINKAAAAAAAAAAAAAAAAAAAA",
        from,
        "depends_on",
        "todo",
        "01TODOBBBBBBBBBBBBBBBBBBBB",
    );
    let rel = link("01LINKBBBBBBBBBBBBBBBBBBBB", from, "related", "item", "42");
    let conflicts = sync_repo::push_todo_link(&pool, user, vec![fresh(dep.clone()), fresh(rel)])
        .await
        .unwrap();
    assert!(conflicts.is_empty());
    assert_eq!(live(&pool, user).await.len(), 2);

    // Remove one: a tombstone, which the pull still carries.
    let cur = live(&pool, user)
        .await
        .into_iter()
        .find(|d| d.ulid == dep.ulid)
        .unwrap();
    let mut gone = cur.clone();
    gone.deleted = true;
    sync_repo::push_todo_link(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: gone,
            assumed_master_state: Some(cur),
        }],
    )
    .await
    .unwrap();
    let pulled = sync_repo::pull_todo_link(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        pulled
            .documents
            .iter()
            .any(|d| d.ulid == dep.ulid && d.deleted)
    );
    let left = live(&pool, user).await;
    assert_eq!(left.len(), 1);
    assert_eq!(
        (left[0].kind.as_str(), left[0].target_ref.as_str()),
        ("related", "42")
    );
}

/// Two offline devices adding the SAME connection (different ulids) must not
/// leave two live edges: the push-time twin guard lands the newcomer already
/// tombstoned, so the list shows one.
#[tokio::test]
async fn duplicate_edges_from_two_devices_are_deduped_on_push() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-todo-link-dupe";
    sqlx::query("DELETE FROM todo_links WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let mk = |ulid: &str| PushEntry {
        new_document_state: TodoLinkDoc {
            ulid: ulid.into(),
            id: None,
            from: "01TODODUPEAAAAAAAAAAAAAAAA".into(),
            kind: "depends_on".into(),
            target_kind: "todo".into(),
            target_ref: "01TODOTARGETBBBBBBBBBBBBBB".into(),
            deleted: false,
            rev: 0,
        },
        assumed_master_state: None,
    };

    // Device A then device B push the semantically identical edge.
    sync_repo::push_todo_link(&pool, user, vec![mk("01LINKDEVICEA0000000000000")])
        .await
        .unwrap();
    sync_repo::push_todo_link(&pool, user, vec![mk("01LINKDEVICEB0000000000000")])
        .await
        .unwrap();

    // Exactly one live edge remains; the later ulid is the tombstoned one.
    let edges = live(&pool, user).await;
    assert_eq!(
        edges.len(),
        1,
        "duplicate edge should be tombstoned on push"
    );

    // A boot-time dedupe pass leaves this user's single edge untouched (it runs
    // table-wide, so don't assert its global count under parallel tests).
    sync_repo::dedupe_todo_links(&pool).await.unwrap();
    assert_eq!(live(&pool, user).await.len(), 1);
}
