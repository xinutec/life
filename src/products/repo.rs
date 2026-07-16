//! Persistence for the product cache.

use anyhow::Result;
use sqlx::MySqlPool;

use super::types::Product;

#[derive(sqlx::FromRow)]
struct MetaRow {
    id: u64,
    barcode: Option<String>,
    external_id: Option<String>,
    name: Option<String>,
    brand: Option<String>,
    quantity_label: Option<String>,
    source: Option<String>,
    has_image: i64,
}

impl From<MetaRow> for Product {
    fn from(r: MetaRow) -> Self {
        Product {
            id: r.id,
            barcode: r.barcode,
            name: r.name,
            brand: r.brand,
            quantity_label: r.quantity_label,
            source: r.source,
            external_id: r.external_id,
            has_image: r.has_image != 0,
        }
    }
}

// The metadata columns every getter selects (no image bytes). Kept in sync by
// hand across the three getters below — sqlx 0.8 only accepts `&'static str`
// SQL (its injection guard), so this can't be a shared runtime `format!`.
// "SELECT id, barcode, external_id, name, brand, quantity_label, source,
//  (image IS NOT NULL) AS has_image FROM products WHERE …"

/// Cached metadata for a barcode (no image bytes), or None if not cached.
pub async fn get(pool: &MySqlPool, barcode: &str) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         (image IS NOT NULL) AS has_image FROM products WHERE barcode = ?",
    )
    .bind(barcode)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// Name/brand substring search over the catalog (the picker's catalog tier).
/// Case-insensitivity comes from the columns' utf8mb4 collation; `%`/`_`/`\`
/// in the query are escaped so they match literally.
pub async fn search(pool: &MySqlPool, query: &str, limit: u64) -> Result<Vec<Product>> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let rows: Vec<MetaRow> = sqlx::query_as(
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         (image IS NOT NULL) AS has_image FROM products \
         WHERE name LIKE ? OR brand LIKE ? ORDER BY name LIMIT ?",
    )
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Product::from).collect())
}

/// Catalog row by surrogate id, or None.
pub async fn get_by_id(pool: &MySqlPool, id: u64) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         (image IS NOT NULL) AS has_image FROM products WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// Catalog row by (source, external_id) — how a shop product is addressed, or None.
pub async fn get_by_source_external(
    pool: &MySqlPool,
    source: &str,
    external_id: &str,
) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         (image IS NOT NULL) AS has_image FROM products WHERE source = ? AND external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// Insert or refresh a catalog row imported from an external source, keyed on
/// (source, external_id). Metadata only — the image is fetched and stored
/// separately by id (a shop product may have no barcode). Returns the stored row.
pub async fn upsert_external(
    pool: &MySqlPool,
    source: &str,
    external_id: &str,
    name: Option<&str>,
    brand: Option<&str>,
    category: Option<&str>,
) -> Result<Product> {
    sqlx::query(
        "INSERT INTO products (source, external_id, name, brand, category) \
         VALUES (?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE name = VALUES(name), brand = VALUES(brand), \
         category = VALUES(category)",
    )
    .bind(source)
    .bind(external_id)
    .bind(name)
    .bind(brand)
    .bind(category)
    .execute(pool)
    .await?;
    get_by_source_external(pool, source, external_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("row vanished immediately after upsert"))
}

/// Cached image bytes + mime for a catalog id, if present.
pub async fn get_image_by_id(pool: &MySqlPool, id: u64) -> Result<Option<(Vec<u8>, String)>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT image, image_mime FROM products WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        Some((Some(bytes), mime)) => Some((bytes, mime.unwrap_or_else(|| "image/jpeg".into()))),
        _ => None,
    })
}

/// Replace the image bytes for a catalog row by id (leaving metadata as-is).
pub async fn set_image_by_id(pool: &MySqlPool, id: u64, bytes: &[u8], mime: &str) -> Result<()> {
    sqlx::query("UPDATE products SET image = ?, image_mime = ?, fetched_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(bytes)
        .bind(mime)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cached image bytes + mime for a barcode, if present.
pub async fn get_image(pool: &MySqlPool, barcode: &str) -> Result<Option<(Vec<u8>, String)>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT image, image_mime FROM products WHERE barcode = ?")
            .bind(barcode)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        Some((Some(bytes), mime)) => Some((bytes, mime.unwrap_or_else(|| "image/jpeg".into()))),
        _ => None,
    })
}

/// Replace just the image bytes for a barcode, leaving name/brand/quantity as
/// they are. Creates a bare catalog row if the barcode was never looked up (so
/// you can give an image to a product OFF has never heard of); `source='user'`
/// marks a hand-uploaded image, but only on insert — a later OFF metadata
/// refresh keeps its own `source`. The unique `barcode` key drives the upsert.
pub async fn set_image(pool: &MySqlPool, barcode: &str, bytes: &[u8], mime: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO products (barcode, image, image_mime, source) \
         VALUES (?, ?, ?, 'user') \
         ON DUPLICATE KEY UPDATE image = VALUES(image), \
         image_mime = VALUES(image_mime), fetched_at = CURRENT_TIMESTAMP",
    )
    .bind(barcode)
    .bind(bytes)
    .bind(mime)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert or refresh a cached product (with optional image).
pub async fn upsert(
    pool: &MySqlPool,
    barcode: &str,
    name: Option<&str>,
    brand: Option<&str>,
    quantity_label: Option<&str>,
    image: Option<(Vec<u8>, String)>,
) -> Result<()> {
    let (bytes, mime) = match image {
        Some((b, m)) => (Some(b), Some(m)),
        None => (None, None),
    };
    sqlx::query(
        "INSERT INTO products (barcode, name, brand, quantity_label, image, image_mime, source) \
         VALUES (?, ?, ?, ?, ?, ?, 'off') \
         ON DUPLICATE KEY UPDATE name = VALUES(name), brand = VALUES(brand), \
         quantity_label = VALUES(quantity_label), image = VALUES(image), \
         image_mime = VALUES(image_mime), fetched_at = CURRENT_TIMESTAMP",
    )
    .bind(barcode)
    .bind(name)
    .bind(brand)
    .bind(quantity_label)
    .bind(&bytes)
    .bind(&mime)
    .execute(pool)
    .await?;
    Ok(())
}
