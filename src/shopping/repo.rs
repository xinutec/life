//! The Buy list, sync-aware: every write takes a `rev`, and deletes are tombstones.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::ShoppingItem;
use crate::inventory::repo as inventory_repo;
use crate::inventory::types::{Item, NewItem};
use crate::sync::repo::{next_rev, stamp};

/// Undone first, then by name.
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

/// The one undelete path; a fresh `rev` carries it to every device.
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

/// Take a row off the list and into the cupboard, in one transaction with the
/// row locked: a failure leaves it on the list, and a double tap finds nothing.
/// `Ok(None)`: no such row.
pub async fn buy(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Item>> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
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
            expiry_precision: None,
            location_id: None,
            barcode: s.barcode,
            product_id: s.product_id,
            // A list name is a note to self ("cheese"); the catalogue outranks it.
            name_source: None,
        },
    )
    .await?;
    tx.commit().await?;
    inventory_repo::get_item(pool, user_id, item_id).await
}
