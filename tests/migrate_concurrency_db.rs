//! Migrations must be safe to run concurrently.

mod common;

use life::db;

/// Two processes booting at once — several test binaries, or two replicas — must
/// not race: sqlx takes no cross-connection lock on MySQL, so without
/// `db::migrate`'s lock one dies on a duplicate `_sqlx_migrations` row.
#[tokio::test]
async fn concurrent_migrations_do_not_race() {
    let url = common::test_db_url();

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
