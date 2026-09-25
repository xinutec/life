//! MariaDB connection pool. life's own database — NC is never written to.

use anyhow::{Context, Result};
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;

pub async fn connect(database_url: &str) -> Result<MySqlPool> {
    let pool = MySqlPoolOptions::new()
        .max_connections(8)
        // Session zone UTC: DB-clock columns (`NOW()`, `DEFAULT
        // CURRENT_TIMESTAMP`) are read back with `.and_utc()`, which assumes it.
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET time_zone = '+00:00'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .context("connecting to MariaDB")?;
    Ok(pool)
}

/// Name of the advisory lock that serialises migrations across processes.
const MIGRATION_LOCK: &str = "life_migrations";
/// Long enough to outlast a real migration, short enough to fail loudly if a
/// previous holder wedged.
const MIGRATION_LOCK_TIMEOUT_SECS: i32 = 60;

/// Apply embedded migrations from `migrations/`. Idempotent; safe on every boot,
/// and safe when several processes boot **at the same time**.
///
/// sqlx takes no cross-connection lock on MySQL, so two processes migrating at
/// once (every DB test binary does, as would a second replica) both apply
/// version 1 and one dies on a duplicate key. A named lock, released if the
/// holder dies, serialises them.
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
    // 1 = acquired, 0 = timed out, NULL = error. Only 1 means we may migrate.
    if got != Some(1) {
        anyhow::bail!(
            "could not acquire the '{MIGRATION_LOCK}' lock within {MIGRATION_LOCK_TIMEOUT_SECS}s \
             (another process may be migrating, or holding it wedged)"
        );
    }

    // Migrate on the pool while `conn` holds the lock — the lock is what serialises
    // us, so the migrator itself can use whatever connection it likes. Run to
    // completion, then release whatever happened: an early `?` here would hold the
    // lock until the connection dropped and stall every other booter.
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
