//! Offline-first sync over a global, commit-ordered revision counter
//! (docs/design/sync.md).

pub mod repo;
pub mod types;

use anyhow::Result;
use sqlx::MySqlPool;

/// One-time backfills, after migrations. Idempotent.
pub async fn backfill(pool: &MySqlPool) -> Result<()> {
    let n = repo::backfill_shopping(pool).await?;
    if n > 0 {
        tracing::info!("sync backfill: assigned ulid+rev to {n} shopping row(s)");
    }
    let n = repo::dedupe_todo_links(pool).await?;
    if n > 0 {
        tracing::info!("sync backfill: tombstoned {n} duplicate todo-link edge(s)");
    }
    Ok(())
}
