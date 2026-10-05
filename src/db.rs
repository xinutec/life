//! The MariaDB pool. Nextcloud's database is never written to.

use anyhow::{Context, Result};
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;

pub async fn connect(database_url: &str) -> Result<MySqlPool> {
    let pool = MySqlPoolOptions::new()
        .max_connections(8)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // UTC, as `.and_utc()` assumes when reading `NOW()` columns back.
                sqlx::query("SET time_zone = '+00:00'")
                    .execute(&mut *conn)
                    .await?;
                // No gap locks: under REPEATABLE READ a delete of absent rows
                // locks a gap another product's insert needs, and deadlocks.
                // Correctness rests on row locks and the revision counter.
                sqlx::query("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .context("connecting to MariaDB")?;
    Ok(pool)
}

const MIGRATION_LOCK: &str = "life_migrations";
/// Longer than a real migration; short enough to fail loudly on a wedged holder.
const MIGRATION_LOCK_TIMEOUT_SECS: i32 = 60;

/// Idempotent, and safe when processes boot at once: sqlx takes no
/// cross-connection lock on MySQL, so a named lock serialises them.
pub async fn migrate(pool: &MySqlPool) -> Result<()> {
    let mut conn = pool
        .acquire()
        .await
        .context("acquiring migration lock conn")?;

    let got: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK(?, ?)")
        .bind(MIGRATION_LOCK)
        .bind(MIGRATION_LOCK_TIMEOUT_SECS)
        .fetch_one(&mut *conn)
        .await
        .context("taking the migration lock")?;
    // 1 = acquired, 0 = timed out, NULL = error.
    if got != Some(1) {
        anyhow::bail!(
            "could not acquire the '{MIGRATION_LOCK}' lock within {MIGRATION_LOCK_TIMEOUT_SECS}s \
             (another process may be migrating, or holding it wedged)"
        );
    }

    // Run to completion and then release, whatever happened: an early `?` would
    // hold the lock and stall every other booter.
    let migrated = sqlx::migrate!()
        .run(pool)
        .await
        .context("running migrations");

    let released = sqlx::query("SELECT RELEASE_LOCK(?)")
        .bind(MIGRATION_LOCK)
        .execute(&mut *conn)
        .await
        .context("releasing the migration lock");

    migrated?;
    released?;
    Ok(())
}
