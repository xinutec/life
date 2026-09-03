//! Storage for item attachments. Every query is scoped on `user_id`, and the
//! listing never reads a blob.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::ItemFile;

/// Metadata for everything attached to one item, newest first. No blobs — see
/// [`ItemFile`] for why the list and the download are separate.
pub async fn for_item(pool: &MySqlPool, user_id: &str, item_id: u64) -> Result<Vec<ItemFile>> {
    let rows = sqlx::query_as::<_, ItemFile>(
        "SELECT id, item_id, purchase_id, name, mime, size_bytes, created_at \
         FROM item_files WHERE user_id = ? AND item_id = ? \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Attach a file. Returns its id.
///
/// `mime` is the SNIFFED type, not the declared one — the caller is responsible
/// for having established that, and the column is documented as holding it.
pub async fn add(
    pool: &MySqlPool,
    user_id: &str,
    item_id: u64,
    purchase_id: Option<u64>,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<u64> {
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
    Ok(res.last_insert_id())
}

/// The bytes and their mime, for serving one file back.
///
/// Scoped on `item_id` as well as `user_id`, like the purchase reads: the route
/// arrives through an item, and a file id belonging to a DIFFERENT item of
/// yours should 404 rather than be served from under the wrong thing.
pub async fn read(
    pool: &MySqlPool,
    user_id: &str,
    item_id: u64,
    id: u64,
) -> Result<Option<(String, String, Vec<u8>)>> {
    let row: Option<(String, String, Vec<u8>)> = sqlx::query_as(
        "SELECT name, mime, bytes FROM item_files \
         WHERE id = ? AND user_id = ? AND item_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Remove one attachment. Returns whether a row was removed.
///
/// Hard, and built in the same change as the upload rather than after it. A
/// write path with no inverse cannot be exercised against production without
/// leaving something behind — which is exactly how the purchase route shipped
/// verified by its refusals alone.
pub async fn remove(pool: &MySqlPool, user_id: &str, item_id: u64, id: u64) -> Result<bool> {
    let res = sqlx::query("DELETE FROM item_files WHERE id = ? AND user_id = ? AND item_id = ?")
        .bind(id)
        .bind(user_id)
        .bind(item_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
