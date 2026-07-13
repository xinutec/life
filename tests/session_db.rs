//! Session lifetime against a real MariaDB: the expiry must slide forward as the
//! session is used, and a rejection must say why.
//!
//! The regression this pins: the expiry used to be written once at login and
//! never touched, so a session died exactly 7 days after login however much the
//! app was used in between (2026-07-13: signed out on the phone after a week of
//! daily use). Runs only when LIFE_TEST_DATABASE_URL is set (see
//! scripts/dev-db.sh); skips otherwise.

use chrono::{Duration, NaiveDateTime, Utc};
use life::db;
use life::session::{
    SessionLookup, SessionReject, UserSession, create_session, resolve_session, sign_value,
    sweep_expired, verify_value,
};
use sqlx::MySqlPool;

const SECRET: &str = "test-secret";
const USER: &str = "test-user-session";

async fn expiry_of(pool: &MySqlPool, signed: &str) -> NaiveDateTime {
    let id = verify_value(SECRET, signed).expect("signed cookie");
    let (expires,): (NaiveDateTime,) =
        sqlx::query_as("SELECT expires_at FROM sessions WHERE id = ?")
            .bind(&id)
            .fetch_one(pool)
            .await
            .expect("session row");
    expires
}

/// Move a session's expiry, to stand in for the passage of time.
async fn set_expiry(pool: &MySqlPool, signed: &str, at: NaiveDateTime) {
    let id = verify_value(SECRET, signed).expect("signed cookie");
    sqlx::query("UPDATE sessions SET expires_at = ? WHERE id = ?")
        .bind(at)
        .bind(&id)
        .execute(pool)
        .await
        .expect("update expiry");
}

async fn exists(pool: &MySqlPool, signed: &str) -> bool {
    let id = verify_value(SECRET, signed).expect("signed cookie");
    let row: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = ?")
        .bind(&id)
        .fetch_optional(pool)
        .await
        .expect("query");
    row.is_some()
}

#[tokio::test]
async fn session_lifetime_against_real_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping DB integration test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(USER)
        .execute(&pool)
        .await
        .unwrap();

    let user = UserSession {
        user_id: USER.into(),
        display_name: "Test".into(),
    };
    let signed = create_session(&pool, SECRET, &user).await.expect("create");

    // A fresh session resolves, and is NOT rewritten — the expiry is already as
    // far out as a slide would put it, so using the app must not cost a DB write
    // per request.
    let at_login = expiry_of(&pool, &signed).await;
    match resolve_session(&pool, SECRET, &signed)
        .await
        .expect("resolve")
    {
        SessionLookup::Valid(u) => assert_eq!(u.user_id, USER),
        SessionLookup::Rejected(r) => panic!("fresh session rejected: {r:?}"),
    }
    assert_eq!(
        expiry_of(&pool, &signed).await,
        at_login,
        "no needless write"
    );

    // THE REGRESSION. Six days in, with one day left on the clock: using the app
    // must push the expiry back out to a full week from now. Before the fix this
    // stayed put, and the session died on day 7 regardless of use.
    let nearly_up = Utc::now().naive_utc() + Duration::days(1);
    set_expiry(&pool, &signed, nearly_up).await;
    assert!(matches!(
        resolve_session(&pool, SECRET, &signed)
            .await
            .expect("resolve"),
        SessionLookup::Valid(_)
    ));
    let slid = expiry_of(&pool, &signed).await;
    assert!(
        slid > nearly_up + Duration::days(5),
        "expiry must slide forward on use: {nearly_up} -> {slid}"
    );

    // Idle past the window: rejected as expired, and the dead row is dropped.
    set_expiry(
        &pool,
        &signed,
        Utc::now().naive_utc() - Duration::minutes(1),
    )
    .await;
    assert_eq!(
        reject(&pool, &signed).await,
        SessionReject::Expired,
        "an idle session must expire"
    );
    assert!(!exists(&pool, &signed).await, "expired row deleted on use");

    // A rejection says which of the four stories it is — a bare 401 cannot tell a
    // timer running out from a device losing its cookie or a forged signature.
    let ghost = sign_value(SECRET, "0123456789abcdef");
    assert_eq!(reject(&pool, &ghost).await, SessionReject::Unknown);
    assert_eq!(
        reject(&pool, "some-id.deadbeef").await,
        SessionReject::BadSignature
    );
    let other_secret = sign_value("not-our-secret", "0123456789abcdef");
    assert_eq!(
        reject(&pool, &other_secret).await,
        SessionReject::BadSignature
    );

    // The sweeper still clears sessions that expire without ever being presented
    // again — sliding does not leave them to accumulate.
    let stale = create_session(&pool, SECRET, &user).await.expect("create");
    set_expiry(&pool, &stale, Utc::now().naive_utc() - Duration::days(1)).await;
    sweep_expired(&pool).await.expect("sweep");
    assert!(!exists(&pool, &stale).await, "swept");
}

async fn reject(pool: &MySqlPool, signed: &str) -> SessionReject {
    match resolve_session(pool, SECRET, signed)
        .await
        .expect("resolve")
    {
        SessionLookup::Valid(_) => panic!("expected rejection"),
        SessionLookup::Rejected(r) => r,
    }
}
