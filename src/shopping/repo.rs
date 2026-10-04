//! Persistence for the shopping list.
//!
//! Every write is sync-aware (see `crate::sync`): it allocates a global `rev` in
//! the same transaction, stamps `updated_at`, and *soft*-deletes (sets
//! `deleted_at`) so deletes propagate to offline clients as tombstones. Reads hide
//! tombstoned rows.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::ShoppingItem;
use crate::sync::repo::stamp;

/// To-buy items, undone first, then by name. Tombstoned rows are hidden.
pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<ShoppingItem>> {
    let rows: Vec<ShoppingItem> = sqlx::query_as(
        "SELECT id, name, quantity, unit, barcode, category, product_id, done \
         FROM shopping_items \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY done, name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<ShoppingItem>> {
    let row: Option<ShoppingItem> = sqlx::query_as(
        "SELECT id, name, quantity, unit, barcode, category, product_id, done \
         FROM shopping_items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Soft delete: set the tombstone + a fresh `rev` so the delete syncs.
pub async fn delete(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    stamp(pool, |rev| {
        sqlx::query(
            "UPDATE shopping_items SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
             WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
        )
        .bind(rev)
        .bind(id)
        .bind(user_id)
    })
    .await
}

/// Restore a tombstoned row (trash/undo). The ONE deliberate undelete path —
/// sync pushes can never clear a tombstone. The fresh `rev` propagates the
/// resurrected row to every device through the normal pull.
pub async fn restore(pool: &MySqlPool, user_id: &str, ulid: &str) -> Result<bool> {
    stamp(pool, |rev| {
        sqlx::query(
            "UPDATE shopping_items SET deleted_at = NULL, rev = ?, updated_at = NOW() \
             WHERE ulid = ? AND user_id = ? AND deleted_at IS NOT NULL",
        )
        .bind(rev)
        .bind(ulid)
        .bind(user_id)
    })
    .await
}
