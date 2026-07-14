//! Migrations must be safe to run concurrently. Runs only when
//! LIFE_TEST_DATABASE_URL is set; skips otherwise.

use life::db;

/// Two processes booting at once — or, in practice, several test binaries that each
/// call `migrate()` against the same database — used to race: sqlx applies migrations
/// without a cross-connection lock on MySQL, so both would insert the same
/// `_sqlx_migrations` row and one would die with `1062 Duplicate entry '1' for key
/// 'PRIMARY'`. That made the whole suite flaky (a different DB test failed on each
/// run) and would bite for real the moment the backend ran with more than one replica.
#[tokio::test]
async fn concurrent_migrations_do_not_race() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping migration-race test");
        return;
    };

    // Fresh pools, as separate processes would have — a shared pool would serialise
    // on its own and hide the race.
    let runs: Vec<_> = (0..8)
        .map(|_| {
            let url = url.clone();
            tokio::spawn(async move {
                let pool = db::connect(&url).await.expect("connect");
                db::migrate(&pool).await
            })
        })
        .collect();

    for (i, handle) in runs.into_iter().enumerate() {
        handle
            .await
            .expect("task panicked")
            .unwrap_or_else(|e| panic!("concurrent migrate #{i} failed: {e:#}"));
    }
}
