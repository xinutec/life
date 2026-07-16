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

/// The canonical product carrying a listing for (source, external_id), or None.
/// Resolved through `product_listings`, so it finds a product via ANY of its
/// sources — not only the one it was first created from.
pub async fn get_by_source_external(
    pool: &MySqlPool,
    source: &str,
    external_id: &str,
) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT p.id, p.barcode, p.external_id, p.name, p.brand, p.quantity_label, p.source, \
         (p.image IS NOT NULL) AS has_image \
         FROM products p JOIN product_listings l ON l.product_id = p.id \
         WHERE l.source = ? AND l.external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// One source's listing of a canonical product.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Listing {
    pub source: String,
    pub external_id: String,
    pub url: Option<String>,
    pub raw_name: Option<String>,
}

/// Every source that lists a canonical product, oldest first.
pub async fn listings_for(pool: &MySqlPool, product_id: u64) -> Result<Vec<Listing>> {
    let rows = sqlx::query_as::<_, Listing>(
        "SELECT source, external_id, url, raw_name FROM product_listings \
         WHERE product_id = ? ORDER BY created_at, id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Attach (or refresh) a listing for (source, external_id) onto `product_id`.
/// Keyed on (source, external_id): re-importing the same source id updates the
/// same listing in place (and can re-point it if products were merged).
pub async fn upsert_listing(
    pool: &MySqlPool,
    product_id: u64,
    source: &str,
    external_id: &str,
    url: Option<&str>,
    raw_name: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_listings (product_id, source, external_id, url, raw_name) \
         VALUES (?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE product_id = VALUES(product_id), url = VALUES(url), \
         raw_name = VALUES(raw_name), last_seen_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(source)
    .bind(external_id)
    .bind(url)
    .bind(raw_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// The canonical product id an existing listing points at, if any.
async fn listing_product_id(
    pool: &MySqlPool,
    source: &str,
    external_id: &str,
) -> Result<Option<u64>> {
    let row: Option<(u64,)> = sqlx::query_as(
        "SELECT product_id FROM product_listings WHERE source = ? AND external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// Find the canonical product for `barcode`, creating a bare one if absent. On a
/// hit the existing canonical fields are left untouched (picking the cleanest
/// name across sources is a later increment); on create they're seeded from the
/// calling source. Returns the canonical id.
async fn find_or_create_by_barcode(
    pool: &MySqlPool,
    barcode: &str,
    name: Option<&str>,
    brand: Option<&str>,
    source: &str,
) -> Result<u64> {
    sqlx::query(
        "INSERT INTO products (barcode, name, brand, source, name_source) \
         VALUES (?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE barcode = barcode",
    )
    .bind(barcode)
    .bind(name)
    .bind(brand)
    .bind(source)
    .bind(source)
    .execute(pool)
    .await?;
    let (id,): (u64,) = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
        .bind(barcode)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

/// Import (or refresh) a catalog product from an external source, reconciled by
/// barcode: the canonical `products` row is keyed by EAN, so Asda and Open Food
/// Facts describing the same barcode land on ONE product with two listings. A
/// barcodeless source (Waitrose, by lineNumber) gets/keeps its own canonical
/// row, found via its existing listing. Returns the canonical product.
pub async fn upsert_external(
    pool: &MySqlPool,
    source: &str,
    external_id: &str,
    barcode: Option<&str>,
    name: Option<&str>,
    brand: Option<&str>,
    url: Option<&str>,
) -> Result<Product> {
    let product_id = if let Some(bc) = barcode {
        find_or_create_by_barcode(pool, bc, name, brand, source).await?
    } else if let Some(id) = listing_product_id(pool, source, external_id).await? {
        // A barcodeless product has a single owning source, so a re-import may
        // refresh its canonical name/brand (unlike the shared barcoded case).
        sqlx::query("UPDATE products SET name = ?, brand = ? WHERE id = ?")
            .bind(name)
            .bind(brand)
            .bind(id)
            .execute(pool)
            .await?;
        id
    } else {
        // First sighting of a barcodeless product → a fresh canonical row. The
        // origin source/external_id are kept on the row too (vestigial, for the
        // single-source case) so `Product` still reports them.
        sqlx::query(
            "INSERT INTO products (name, brand, source, name_source, external_id) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(brand)
        .bind(source)
        .bind(source)
        .bind(external_id)
        .execute(pool)
        .await?
        .last_insert_id()
    };
    upsert_listing(pool, product_id, source, external_id, url, name).await?;
    get_by_id(pool, product_id)
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
