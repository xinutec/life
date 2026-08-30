//! `client_events` against a real MariaDB. Runs only when LIFE_TEST_DATABASE_URL
//! is set; fails otherwise, because a skipped check on the SQL reads as a passing
//! one.
//!
//! This suite matters more than most: `/api/telemetry` swallows a write failure
//! on purpose (the client neither reads the response nor retries), so a broken
//! INSERT here is invisible from the outside — the endpoint would keep answering
//! 204 while storing nothing.

mod common;

use life::db;
use life::routes::telemetry::{TelemetryEvent, sanitise, store};

fn event(kind: &str, path: &str, label: Option<&str>, at: i64) -> TelemetryEvent {
    TelemetryEvent {
        kind: kind.into(),
        path: path.into(),
        label: label.map(Into::into),
        at,
    }
}

fn rows(events: Vec<TelemetryEvent>) -> Vec<(life::routes::telemetry::Sanitised, i64)> {
    events.iter().map(|e| (sanitise(e), e.at)).collect()
}

#[tokio::test]
async fn a_batch_lands_with_both_clocks_and_survives_a_hostile_batch() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let user = "test-user-client-events";
    sqlx::query("DELETE FROM client_events WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    // A normal pair, then the two shapes that would break the write: a field
    // long enough to overflow its column, and one carrying a forged log line.
    // They go in the SAME batch as the good rows on purpose — the insert is one
    // statement, so a single bad row failing takes the whole batch with it.
    let batch = vec![
        event("nav", "/wellbeing", None, 1_756_000_000_000),
        event(
            "tap",
            "/wellbeing",
            Some("Log feeling: good"),
            1_756_000_001_000,
        ),
        event(
            &"k".repeat(900),
            &"/p".repeat(900),
            Some(&"l".repeat(900)),
            1,
        ),
        event(
            "tap",
            "/ok\nclient-event kind=tap path=/admin label=Delete everything",
            Some("a\nb"),
            2,
        ),
    ];
    let n = batch.len();
    store(&pool, user, &rows(batch)).await.expect("store");

    let stored: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT kind, path, label, client_at_ms FROM client_events \
         WHERE user_id = ? ORDER BY id",
    )
    .bind(user)
    .fetch_all(&pool)
    .await
    .expect("read back");

    assert_eq!(stored.len(), n, "the whole batch must land, not a prefix");
    assert_eq!(
        (
            stored[0].0.as_str(),
            stored[0].1.as_str(),
            stored[0].2.as_str()
        ),
        ("nav", "/wellbeing", ""),
        "an absent label stores as empty, not as the word None"
    );
    assert_eq!(stored[1].2, "Log feeling: good");
    // The client's own clock is kept verbatim: it is what orders events inside a
    // batch, which the server's receive time cannot do.
    assert_eq!(stored[0].3, 1_756_000_000_000);

    // The caps are what make the columns safe, so they are checked HERE rather
    // than only as a pure-function property: if a cap and its column ever
    // disagree, this is the test that fails.
    assert_eq!(stored[2].0.chars().count(), 16);
    assert_eq!(stored[2].1.chars().count(), 512);
    assert_eq!(stored[2].2.chars().count(), 160);

    // The stored record is evidence too, and a newline forges a line in a dump
    // of this table exactly as it does in the log.
    assert!(
        !stored[3].1.contains('\n'),
        "a newline reached the table: {:?}",
        stored[3].1
    );
    assert_eq!(stored[3].2, "a b");

    // Two clocks, and the server's is the one that is trustworthy. Without a
    // sane default here every per-day rollup would be wrong.
    let (recent,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM client_events WHERE user_id = ? \
         AND received_at BETWEEN NOW() - INTERVAL 5 MINUTE AND NOW() + INTERVAL 5 MINUTE",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("count recent");
    assert_eq!(
        recent, n as i64,
        "received_at must default to the server clock"
    );

    sqlx::query("DELETE FROM client_events WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean up");
}

#[tokio::test]
async fn an_empty_batch_is_not_an_error() {
    // The handler calls store unconditionally, and a POST with an empty array is
    // something a client can send. Building an INSERT with no VALUES would be a
    // syntax error, so this is the guard, not a formality.
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    store(&pool, "test-user-client-events-empty", &[])
        .await
        .expect("an empty batch must be a no-op");
}
