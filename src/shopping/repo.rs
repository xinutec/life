//! Persistence for the shopping list.
//!
//! Every write is sync-aware (see `crate::sync`): it allocates a global `rev` in
//! the same transaction, stamps `updated_at`, and *soft*-deletes (sets
//! `deleted_at`) so deletes propagate to offline clients as tombstones. Reads hide
//! tombstoned rows.

use anyhow::Result;
use sqlx::MySqlPool;
use ulid::Ulid;

use super::types::{NewShoppingItem, ShoppingItem, UpdateShoppingItem};
use crate::sync::repo::{next_rev, stamp};

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

pub async fn create(pool: &MySqlPool, user_id: &str, new: NewShoppingItem) -> Result<ShoppingItem> {
    let ulid = Ulid::new().to_string();
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "INSERT INTO shopping_items (user_id, ulid, name, quantity, unit, barcode, category, \
         product_id, rev, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(&ulid)
    .bind(&new.name)
    .bind(new.quantity)
    .bind(&new.unit)
    .bind(&new.barcode)
    .bind(new.category)
    .bind(new.product_id)
    .bind(rev)
    .execute(&mut *tx)
    .await?;
    let id = res.last_insert_id();
    tx.commit().await?;
    Ok(ShoppingItem {
        id,
        name: new.name,
        quantity: new.quantity,
        unit: new.unit,
        barcode: new.barcode,
        category: new.category,
        product_id: new.product_id,
        done: false,
    })
}

pub async fn update(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    upd: UpdateShoppingItem,
) -> Result<Option<ShoppingItem>> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE shopping_items SET name = ?, quantity = ?, unit = ?, barcode = ?, category = ?, \
         product_id = ?, done = ?, rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(&upd.name)
    .bind(upd.quantity)
    .bind(&upd.unit)
    .bind(&upd.barcode)
    .bind(upd.category)
    .bind(upd.product_id)
    .bind(upd.done)
    .bind(rev)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    get(pool, user_id, id).await
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
