//! Wellbeing check-ins against a real MariaDB. Runs only when
//! LIFE_TEST_DATABASE_URL is set; skips otherwise. Covers the sync pull/push
//! round-trip (offline insert → pull → update → stale-conflict → tombstone) plus
//! the trash restore.

use chrono::{TimeZone, Utc};
use life::db;
use life::sync::repo as sync_repo;
use life::sync::types::{PushEntry, WellbeingDoc};
use life::wellbeing::repo as wellbeing_repo;

/// `score` is in tenths (10..50, a half-point apart) — 20 is a 2, 35 a 3.5.
fn doc(ulid: &str, score: u8, rev: u64, deleted: bool) -> WellbeingDoc {
    WellbeingDoc {
        ulid: ulid.into(),
        id: None,
        recorded_at: Utc.with_ymd_and_hms(2026, 7, 3, 9, 30, 0).unwrap(),
        score_tenths: score,
        energy_tenths: Some(20),
        emotions: vec!["Withdrawn".into(), "Anxious".into()],
        note: Some("felt low".into()),
        deleted,
        rev,
    }
}

#[tokio::test]
async fn wellbeing_sync_and_restore_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping wellbeing DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-wellbeing";
    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let ulid = "0123456789ABCDEFGHJKMNPQRS";

    // Offline-created check-in lands on the server (no assumed master state).
    let conflicts = sync_repo::push_wellbeing(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(ulid, 20, 0, false),
            assumed_master_state: None,
        }],
    )
    .await
    .unwrap();
    assert!(conflicts.is_empty());

    // Pull surfaces it with a server id + rev; the timestamp round-trips as UTC.
    let pulled = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    let got = pulled
        .documents
        .iter()
        .find(|d| d.ulid == ulid)
        .expect("present");
    assert_eq!(got.score_tenths, 20); // a 2, in tenths
    assert_eq!(got.energy_tenths, Some(20));
    assert_eq!(got.emotions, vec!["Withdrawn", "Anxious"]);
    assert_eq!(
        got.recorded_at,
        Utc.with_ymd_and_hms(2026, 7, 3, 9, 30, 0).unwrap()
    );
    let server_rev = got.rev;

    // Update with the correct assumed rev is accepted; a stale one conflicts.
    let stale = sync_repo::push_wellbeing(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(ulid, 50, 0, false),
            assumed_master_state: Some(doc(ulid, 20, server_rev - 1, false)), // wrong rev
        }],
    )
    .await
    .unwrap();
    assert_eq!(stale.len(), 1, "stale push is rejected as a conflict");

    let ok = sync_repo::push_wellbeing(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(ulid, 50, 0, false),
            assumed_master_state: Some(doc(ulid, 20, server_rev, false)),
        }],
    )
    .await
    .unwrap();
    assert!(ok.is_empty());
    let after = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert_eq!(
        after
            .documents
            .iter()
            .find(|d| d.ulid == ulid)
            .unwrap()
            .score_tenths,
        50
    );

    // Tombstone via push, then the explicit trash restore brings it back.
    let cur = after.documents.iter().find(|d| d.ulid == ulid).unwrap().rev;
    sync_repo::push_wellbeing(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: doc(ulid, 50, 0, true),
            assumed_master_state: Some(doc(ulid, 50, cur, false)),
        }],
    )
    .await
    .unwrap();
    let deleted = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        deleted
            .documents
            .iter()
            .find(|d| d.ulid == ulid)
            .unwrap()
            .deleted
    );

    assert!(wellbeing_repo::restore(&pool, user, ulid).await.unwrap());
    let restored = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    assert!(
        !restored
            .documents
            .iter()
            .find(|d| d.ulid == ulid)
            .unwrap()
            .deleted
    );
}

/// A half-step survives the round-trip intact: 35 tenths in, 35 tenths out. The
/// whole point of the rescale — if this rounds anywhere on the way through, "4 but
/// a bit lower at the gym" silently becomes a plain 4 or a plain 3.
#[tokio::test]
async fn half_steps_round_trip_through_the_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping wellbeing DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-wellbeing-half";
    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    let ulid = "0123456789ABCDEFGHJKMNPQRH";
    let mut d = doc(ulid, 35, 0, false); // a 3.5
    d.energy_tenths = Some(45); // "energetic in some sense"
    sync_repo::push_wellbeing(
        &pool,
        user,
        vec![PushEntry {
            new_document_state: d,
            assumed_master_state: None,
        }],
    )
    .await
    .unwrap();

    let pulled = sync_repo::pull_wellbeing(&pool, user, 0, 100)
        .await
        .unwrap();
    let got = pulled
        .documents
        .iter()
        .find(|d| d.ulid == ulid)
        .expect("present");
    assert_eq!(got.score_tenths, 35, "a 3.5 stays a 3.5");
    assert_eq!(got.energy_tenths, Some(45), "a 4.5 stays a 4.5");

    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn corrupt_emotions_fails_the_pull_loudly() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping wellbeing DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-wellbeing-corrupt";
    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    // A row whose emotions column is not valid JSON (hand-edited, or a write
    // bug). The read must fail loudly — silently pulling it as "no emotions"
    // would replicate the data loss to every device.
    sqlx::query(
        "INSERT INTO wellbeing \
         (user_id, ulid, recorded_at, score_tenths, energy_tenths, emotions, note, deleted_at, rev, created_at, updated_at) \
         VALUES (?, ?, NOW(), 30, NULL, 'not-json', NULL, NULL, 1, NOW(), NOW())",
    )
    .bind(user)
    .bind("0123456789ABCDEFGHJKMNPQRC")
    .execute(&pool)
    .await
    .unwrap();

    let res = sync_repo::pull_wellbeing(&pool, user, 0, 100).await;
    assert!(
        res.is_err(),
        "corrupt emotions must error the pull, not read as empty"
    );

    sqlx::query("DELETE FROM wellbeing WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
}
