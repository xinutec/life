//! Persistence for the shopping list.
//!
//! Every write is sync-aware (see `crate::sync`): it allocates a global `rev` in
//! the same transaction, stamps `updated_at`, and *soft*-deletes (sets
//! `deleted_at`) so deletes propagate to offline clients as tombstones. Reads hide
//! tombstoned rows.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::ShoppingItem;
use crate::inventory::repo as inventory_repo;
use crate::inventory::types::{Item, NewItem};
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

/// Buy a row: take it off the list and put it in the cupboard as an unplaced
/// item, carrying its name, amount, `category` and `product_id`. `Ok(None)` =
/// no such row on the list, including one a second tap already bought.
///
/// One transaction, the row locked: the tombstone and the item commit together,
/// so a failure leaves the row to buy rather than neither to buy nor owned, and
/// a double tap waits for the first and then finds nothing.
pub async fn buy(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Item>> {
    let mut tx = pool.begin().await?;
    let row: Option<ShoppingItem> = sqlx::query_as(
        "SELECT id, name, quantity, unit, barcode, category, product_id, done \
         FROM shopping_items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(s) = row else {
        return Ok(None);
    };
    let rev = next_rev(&mut tx).await?;
    sqlx::query(
        "UPDATE shopping_items SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ?",
    )
    .bind(rev)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let item_id = inventory_repo::insert_item(
        &mut tx,
        user_id,
        &NewItem {
            name: s.name,
            category: s.category,
            quantity: s.quantity,
            unit: s.unit,
            expiry: None,
            // No expiry, so nothing to be precise about.
            expiry_precision: None,
            location_id: None,
            barcode: s.barcode,
            product_id: s.product_id,
            // A buy-list row's name is a note to self ("cheese"), not a naming
            // of the thing that comes home. The catalogue outranks it, which is
            // what `None` asks for.
            name_source: None,
        },
    )
    .await?;
    tx.commit().await?;
    inventory_repo::get_item(pool, user_id, item_id).await
}
