//! Item attachments, scoped on `user_id`; the listing never reads a blob.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::{FileId, ItemFile};
use crate::inventory::types::ItemId;
use crate::purchases::types::PurchaseId;

/// Newest first, no blobs.
pub async fn for_item(pool: &MySqlPool, user_id: &str, item_id: ItemId) -> Result<Vec<ItemFile>> {
    let rows = sqlx::query_as::<_, ItemFile>(
        "SELECT id, item_id, purchase_id, name, mime, size_bytes, created_at \
         FROM item_files WHERE user_id = ? AND item_id = ? AND deleted_at IS NULL \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `mime` must be the sniffed type.
pub async fn add(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
    purchase_id: Option<PurchaseId>,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<FileId> {
    let res = sqlx::query(
        "INSERT INTO item_files (user_id, item_id, purchase_id, name, mime, size_bytes, bytes) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(purchase_id)
    .bind(name)
    .bind(mime)
    .bind(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
    .bind(bytes)
    .execute(pool)
    .await?;
    Ok(res.last_insert_id().into())
}

/// Scoped on `item_id` too, so another item's file id 404s.
pub async fn read(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
    id: FileId,
) -> Result<Option<(String, String, Vec<u8>)>> {
    let row: Option<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT name, mime, bytes FROM item_files \
         WHERE id = ? AND user_id = ? AND item_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// To the trash.
pub async fn remove(pool: &MySqlPool, user_id: &str, item_id: ItemId, id: FileId) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE item_files SET deleted_at = NOW() \
         WHERE id = ? AND user_id = ? AND item_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn restore(pool: &MySqlPool, user_id: &str, id: FileId) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE item_files SET deleted_at = NULL \
         WHERE id = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
