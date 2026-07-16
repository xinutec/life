//! Persistence for the shopping list.
//!
//! Every write is sync-aware (see `crate::sync`): it allocates a global `rev` in
//! the same transaction, stamps `updated_at`, and *soft*-deletes (sets
//! `deleted_at`) so deletes propagate to offline clients as tombstones. Reads hide
//! tombstoned rows.

use std::str::FromStr;

use anyhow::{Result, anyhow};
use sqlx::MySqlPool;
use ulid::Ulid;

use super::types::{NewShoppingItem, ShoppingItem, UpdateShoppingItem};
use crate::inventory::types::ItemCategory;
use crate::sync::repo::next_rev;

#[derive(sqlx::FromRow)]
struct Row {
    id: u64,
    name: String,
    quantity: Option<f64>,
    unit: Option<String>,
    barcode: Option<String>,
    category: String,
    product_id: Option<u64>,
    done: bool,
}

impl Row {
    /// `category` is validated at every write boundary (REST enum, sync push),
    /// so a parse failure here means the DB was edited out-of-band — error out
    /// rather than mask it.
    fn into_item(self) -> Result<ShoppingItem> {
        let category = ItemCategory::from_str(&self.category).map_err(|e| anyhow!(e))?;
        Ok(ShoppingItem {
            id: self.id,
            name: self.name,
            quantity: self.quantity,
            unit: self.unit,
            barcode: self.barcode,
            category,
            product_id: self.product_id,
            done: self.done,
        })
    }
}

/// To-buy items, undone first, then by name. Tombstoned rows are hidden.
pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<ShoppingItem>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, name, quantity, unit, barcode, category, product_id, done \
         FROM shopping_items \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY done, name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(Row::into_item).collect()
}

pub async fn get(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<ShoppingItem>> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, name, quantity, unit, barcode, category, product_id, done \
         FROM shopping_items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    row.map(Row::into_item).transpose()
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
    .bind(new.category.to_string())
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
    .bind(upd.category.to_string())
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
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE shopping_items SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(rev)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Restore a tombstoned row (trash/undo). The ONE deliberate undelete path —
/// sync pushes can never clear a tombstone. The fresh `rev` propagates the
/// resurrected row to every device through the normal pull.
pub async fn restore(pool: &MySqlPool, user_id: &str, ulid: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE shopping_items SET deleted_at = NULL, rev = ?, updated_at = NOW() \
         WHERE ulid = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(rev)
    .bind(ulid)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}
