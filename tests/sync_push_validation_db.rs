//! Push-boundary validation against a real MariaDB: a doc the typed REST
//! boundary could not read back is rejected with a 400-class error and nothing
//! is stored — accepted-then-500-on-read is the inverse of fail-loudly.
//!
//! An unknown enum string cannot reach this far: the docs are typed, so it is
//! refused when the push is decoded (`signed_in_http_db.rs`). What the types
//! cannot rule out is checked here: a reading's range and step.

mod common;

use chrono::{TimeZone, Utc};
use life::db;
use life::sync::repo::{self as sync_repo, PushError};
use life::sync::types::{PushEntry, WellbeingDoc};

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
async fn invalid_readings_are_rejected_and_nothing_is_stored() {
    let url = common::test_db_url();
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-push-validation";
    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // Documented 10..=50 tenths (1.0..=5.0) — reject, not clamp. One half-step
    // past each end, so only the range can refuse them.
    for (ulid, score, energy, what) in [
        (
            "01VAL0000000000000000WELLA",
            5,
            None,
            "score 0.5, one half-step under",
        ),
        (
            "01VAL0000000000000000WELLB",
            55,
            None,
            "score 5.5, one half-step over",
        ),
        ("01VAL0000000000000000WELLC", 30, Some(90), "energy 9.0"),
        // The scale holds tenths but the app only records HALF-points, and a
        // reading off that grid is a bug somewhere — a client sending 3.7 gets
        // told so, rather than having it quietly rounded into a reading never given.
        (
            "01VAL0000000000000000WELLD",
            37,
            None,
            "score 3.7, not a half-step",
        ),
        (
            "01VAL0000000000000000WELLE",
            30,
            Some(43),
            "energy 4.3, not a half-step",
        ),
    ] {
        assert_invalid(
            sync_repo::push_wellbeing(&pool, user, vec![entry(wellbeing(ulid, score, energy))])
                .await,
            what,
        );
    }
    // (That half-steps and both ends are ACCEPTED is asserted in wellbeing_db.rs.)

    // A batch with one invalid doc must reject the whole request BEFORE any
    // write — each entry commits its own transaction, so a mid-loop rejection
    // would otherwise partially apply the push.
    assert_invalid(
        sync_repo::push_wellbeing(
            &pool,
            user,
            vec![
                entry(wellbeing("01VAL0000000000000000GOODA", 30, None)),
                entry(wellbeing("01VAL0000000000000000BADDA", 37, None)),
            ],
        )
        .await,
        "mixed batch",
    );

    let stored = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        stored.documents.is_empty(),
        "nothing stored: {:?}",
        stored.documents
    );

    // A valid reading still lands (the gate rejects bad input, not all input).
    sync_repo::push_wellbeing(
        &pool,
        user,
        vec![entry(wellbeing("01VAL0000000000000000GOODB", 35, Some(20)))],
    )
    .await
    .unwrap();
    let stored = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert_eq!(stored.documents.len(), 1);
}
