//! The trash restore: no sync push can clear a tombstone.

use anyhow::Result;
use sqlx::MySqlPool;

use crate::sync::repo::stamp;

/// A fresh `rev` carries the restored row to every device.
pub async fn restore(pool: &MySqlPool, user_id: &str, ulid: &str) -> Result<bool> {
    stamp(pool, |rev| {
        sqlx::query(
            "UPDATE wellbeing SET deleted_at = NULL, rev = ?, updated_at = NOW() \
             WHERE ulid = ? AND user_id = ? AND deleted_at IS NOT NULL",
        )
        .bind(rev)
        .bind(ulid)
        .bind(user_id)
    })
    .await
}
