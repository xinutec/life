//! The set-only tombstone rule, per collection (B6): a sync push can NEVER
//! clear `deleted_at`, even with the correct assumed rev — the explicit trash
//! restore is the one undelete path. The shopping copy of this test lives in
//! `trash_db.rs`; with pull/push now shared over `SyncSpec`, these prove the
//! remaining collections ride the same implementation. One sequential test
//! (parallel tests contend on sync_rev and can deadlock). Runs only when
//! LIFE_TEST_DATABASE_URL is set; skips otherwise.

use chrono::{TimeZone, Utc};
use life::db;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, TodoDoc, TodoLinkDoc, WellbeingDoc};

const USER: &str = "test-user-tombstone";

fn todo_doc(ulid: &str, rev: u64, deleted: bool) -> TodoDoc {
    TodoDoc {
        ulid: ulid.into(),
        id: None,
        title: "zombie task".into(),
        todo_type: "task".into(),
        status: "open".into(),
        priority: None,
        notes: None,
        not_before: None,
        due: None,
        deleted,
        rev,
    }
}

fn link_doc(ulid: &str, rev: u64, deleted: bool) -> TodoLinkDoc {
    TodoLinkDoc {
        ulid: ulid.into(),
        id: None,
        from: "01TMBFROM00000000000000000".into(),
        kind: "related".into(),
        target_kind: "todo".into(),
        target_ref: "01TMBTARGET000000000000000".into(),
        deleted,
        rev,
    }
}

fn wellbeing_doc(ulid: &str, rev: u64, deleted: bool) -> WellbeingDoc {
    WellbeingDoc {
        ulid: ulid.into(),
        id: None,
        recorded_at: Utc.with_ymd_and_hms(2026, 7, 9, 7, 0, 0).unwrap(),
        score: 3,
        energy: None,
        emotions: vec![],
        note: None,
        deleted,
        rev,
    }
}

/// Create → tombstone → attempt an undelete push with the CORRECT rev →
/// assert the tombstone survived. `make(ulid, rev, deleted)` builds the doc.
macro_rules! assert_set_only {
    ($pool:expr, $push:path, $pull:path, $make:ident, $ulid:expr) => {{
        // Fresh insert, then read back the server rev.
        $push(
            $pool,
            USER,
            vec![PushEntry {
                new_document_state: $make($ulid, 0, false),
                assumed_master_state: None,
            }],
        )
        .await
        .unwrap();
        let rev = $pull($pool, USER, 0, 100)
            .await
            .unwrap()
            .documents
            .iter()
            .find(|d| d.ulid == $ulid)
            .unwrap()
            .rev;

        // Tombstone it (the normal delete-via-sync path).
        $push(
            $pool,
            USER,
            vec![PushEntry {
                new_document_state: $make($ulid, 0, true),
                assumed_master_state: Some($make($ulid, rev, false)),
            }],
        )
        .await
        .unwrap();
        let rev = $pull($pool, USER, 0, 100)
            .await
            .unwrap()
            .documents
            .iter()
            .find(|d| d.ulid == $ulid)
            .unwrap()
            .rev;

        // A push with the CORRECT assumed rev and deleted=false — a buggy or
        // stale client — is accepted (no conflict returned) but must NOT
        // clear the tombstone.
        let conflicts = $push(
            $pool,
            USER,
            vec![PushEntry {
                new_document_state: $make($ulid, 0, false),
                assumed_master_state: Some($make($ulid, rev, true)),
            }],
        )
        .await
        .unwrap();
        assert!(conflicts.is_empty(), "undelete push is not a rev conflict");
        let doc = $pull($pool, USER, 0, 100)
            .await
            .unwrap()
            .documents
            .iter()
            .find(|d| d.ulid == $ulid)
            .unwrap()
            .clone();
        assert!(doc.deleted, "tombstone must survive an undelete push");
    }};
}

#[tokio::test]
async fn tombstones_are_set_only_in_every_collection() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping tombstone DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    for table in ["todos", "todo_links", "wellbeing"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE user_id = ?"
        )))
        .bind(USER)
        .execute(&pool)
        .await
        .unwrap();
    }

    assert_set_only!(
        &pool,
        sync_repo::push_todo,
        sync_repo::pull_todo,
        todo_doc,
        "01TMB0000000000000000TODOA"
    );
    assert_set_only!(
        &pool,
        sync_repo::push_todo_link,
        sync_repo::pull_todo_link,
        link_doc,
        "01TMB0000000000000000LINKA"
    );
    assert_set_only!(
        &pool,
        sync_repo::push_wellbeing,
        sync_repo::pull_wellbeing,
        wellbeing_doc,
        "01TMB0000000000000000WELLA"
    );
}
