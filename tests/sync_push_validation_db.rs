//! Push-boundary validation against a real MariaDB (B5): a doc the typed REST
//! boundary could not read back is rejected with a 400-class error and nothing
//! is stored — accepted-then-500-on-read is the inverse of fail-loudly. Runs
//! only when LIFE_TEST_DATABASE_URL is set; skips otherwise.

use chrono::{TimeZone, Utc};
use life::db;
use life::sync::repo::{self as sync_repo, PushError};
use life::sync::types::{PushEntry, ShoppingDoc, TodoDoc, TodoLinkDoc, WellbeingDoc};

fn todo(ulid: &str, status: &str, todo_type: &str, priority: Option<&str>) -> TodoDoc {
    TodoDoc {
        ulid: ulid.into(),
        id: None,
        title: "validate me".into(),
        todo_type: todo_type.into(),
        status: status.into(),
        priority: priority.map(Into::into),
        notes: None,
        not_before: None,
        due: None,
        shared: false,
        deleted: false,
        rev: 0,
    }
}

/// Both readings are in tenths (10..50, half-points): 20 is a 2, 35 a 3.5.
fn wellbeing(ulid: &str, score_tenths: u8, energy_tenths: Option<u8>) -> WellbeingDoc {
    WellbeingDoc {
        ulid: ulid.into(),
        id: None,
        recorded_at: Utc.with_ymd_and_hms(2026, 7, 9, 9, 0, 0).unwrap(),
        score_tenths,
        energy_tenths,
        emotions: vec![],
        note: None,
        deleted: false,
        rev: 0,
    }
}

fn link(ulid: &str, kind: &str, target_kind: &str) -> TodoLinkDoc {
    TodoLinkDoc {
        ulid: ulid.into(),
        id: None,
        from: "01ISOVALFROMAAAAAAAAAAAAAA".into(),
        kind: kind.into(),
        target_kind: target_kind.into(),
        target_ref: "01ISOVALTARGETAAAAAAAAAAAA".into(),
        deleted: false,
        rev: 0,
    }
}

fn shopping(ulid: &str, category: &str) -> ShoppingDoc {
    ShoppingDoc {
        ulid: ulid.into(),
        id: None,
        name: "validate me".into(),
        quantity: None,
        unit: None,
        barcode: None,
        category: category.into(),
        product_id: None,
        done: false,
        deleted: false,
        rev: 0,
    }
}

fn entry<D>(doc: D) -> PushEntry<D> {
    PushEntry {
        new_document_state: doc,
        assumed_master_state: None,
    }
}

fn assert_invalid<T: std::fmt::Debug>(res: Result<T, PushError>, what: &str) {
    match res {
        Err(PushError::Invalid(_)) => {}
        other => panic!("{what}: expected PushError::Invalid, got {other:?}"),
    }
}

#[tokio::test]
async fn invalid_docs_are_rejected_and_nothing_is_stored() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping push-validation DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-push-validation";
    for table in ["todos", "todo_links", "wellbeing", "shopping_items"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE user_id = ?"
        )))
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Unknown enum strings: each would 500 the whole list at the typed read.
    assert_invalid(
        sync_repo::push_todo(
            &pool,
            user,
            vec![entry(todo(
                "01VAL0000000000000000TODOA",
                "banana",
                "task",
                None,
            ))],
        )
        .await,
        "todo status",
    );
    assert_invalid(
        sync_repo::push_todo(
            &pool,
            user,
            vec![entry(todo(
                "01VAL0000000000000000TODOB",
                "open",
                "chore",
                None,
            ))],
        )
        .await,
        "todo type",
    );
    assert_invalid(
        sync_repo::push_todo(
            &pool,
            user,
            vec![entry(todo(
                "01VAL0000000000000000TODOC",
                "open",
                "task",
                Some("urgent"),
            ))],
        )
        .await,
        "todo priority",
    );
    assert_invalid(
        sync_repo::push_todo_link(
            &pool,
            user,
            vec![entry(link("01VAL0000000000000000LINKA", "sibling", "todo"))],
        )
        .await,
        "link kind",
    );
    assert_invalid(
        sync_repo::push_todo_link(
            &pool,
            user,
            vec![entry(link(
                "01VAL0000000000000000LINKB",
                "related",
                "person",
            ))],
        )
        .await,
        "link target kind",
    );
    // A shopping category the buy→inventory conversion could not re-parse.
    assert_invalid(
        sync_repo::push_shopping(
            &pool,
            user,
            vec![entry(shopping("01VAL0000000000000000SHOPA", "groceries"))],
        )
        .await,
        "shopping category",
    );
    // Documented 10..=50 tenths (1.0..=5.0) — reject, not clamp.
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![entry(wellbeing("01VAL0000000000000000WELLA", 0, None))],
        )
        .await,
        "wellbeing score 0 tenths",
    );
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![entry(wellbeing("01VAL0000000000000000WELLB", 255, None))],
        )
        .await,
        "wellbeing score 255 tenths",
    );
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![entry(wellbeing("01VAL0000000000000000WELLC", 30, Some(90)))],
        )
        .await,
        "wellbeing energy 90 tenths",
    );
    // The scale holds tenths but the app only records HALF-points, and a reading
    // off that grid is a bug somewhere — a client sending 3.7 gets told so, rather
    // than having it quietly rounded into a reading he never gave.
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![entry(wellbeing("01VAL0000000000000000WELLD", 37, None))],
        )
        .await,
        "wellbeing score 3.7 (not a half-step)",
    );
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![entry(wellbeing("01VAL0000000000000000WELLE", 30, Some(43)))],
        )
        .await,
        "wellbeing energy 4.3 (not a half-step)",
    );
    // (That half-steps are ACCEPTED is asserted in wellbeing_db.rs — this test
    // proves the gate rejects, and ends by asserting nothing at all was stored.)

    // A batch with one invalid doc must reject the whole request BEFORE any
    // write — each entry commits its own transaction, so a mid-loop rejection
    // would otherwise partially apply the push.
    assert_invalid(
        sync_repo::push_todo(
            &pool,
            user,
            vec![
                entry(todo("01VAL0000000000000000GOODA", "open", "task", None)),
                entry(todo("01VAL0000000000000000BADDA", "banana", "task", None)),
            ],
        )
        .await,
        "mixed batch",
    );

    // Nothing above may have been stored.
    let todos = sync_repo::pull_todo(&pool, user, 0, 100).await.unwrap();
    assert!(
        todos.documents.is_empty(),
        "no todo stored: {:?}",
        todos.documents
    );
    let links = sync_repo::pull_todo_link(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        links.documents.is_empty(),
        "no link stored: {:?}",
        links.documents
    );
    let well = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        well.documents.is_empty(),
        "no wellbeing stored: {:?}",
        well.documents
    );
    let shop = sync_repo::pull_shopping(&pool, user, 0, 100).await.unwrap();
    assert!(
        shop.documents.is_empty(),
        "no shopping row stored: {:?}",
        shop.documents
    );

    // A valid doc still lands (the gate rejects bad input, not all input).
    sync_repo::push_todo(
        &pool,
        user,
        vec![entry(todo(
            "01VAL0000000000000000GOODB",
            "open",
            "task",
            Some("high"),
        ))],
    )
    .await
    .unwrap();
    let todos = sync_repo::pull_todo(&pool, user, 0, 100).await.unwrap();
    assert_eq!(todos.documents.len(), 1);
}
