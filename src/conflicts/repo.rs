//! The sync-conflict log.

use anyhow::Result;
use sqlx::MySqlPool;

use super::{ConflictEntry, NewConflict};

/// Verbatim: truncating would corrupt the JSON. TEXT holds far more than any field.
pub async fn create(pool: &MySqlPool, user_id: &str, new: NewConflict) -> Result<u64> {
    let res = sqlx::query(
        "INSERT INTO sync_conflicts (user_id, kind, ulid, field, label, mine, theirs) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(new.kind)
    .bind(&new.ulid)
    .bind(&new.field)
    .bind(&new.label)
    .bind(&new.mine)
    .bind(&new.theirs)
    .execute(pool)
    .await?;
    Ok(res.last_insert_id())
}

pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<ConflictEntry>> {
    Ok(sqlx::query_as(
        "SELECT id, kind, ulid, field, label, mine, theirs, \
         created_at \
         FROM sync_conflicts WHERE user_id = ? AND resolved_at IS NULL \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

/// Stamped, not deleted.
pub async fn resolve(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE sync_conflicts SET resolved_at = NOW() \
         WHERE id = ? AND user_id = ? AND resolved_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
