//! Persistence for the product catalogue.

mod facts;
mod ingest;
mod prices;
mod reconcile;

pub use facts::*;
pub use ingest::ingest;
pub use prices::*;
pub use reconcile::*;

use anyhow::Result;
use sqlx::MySqlPool;

use super::coverage::{AttachedListing, ListingPrice, RowPrice, Sighting};
use super::ids::{Barcode, ExternalId, ListingId, ProductId};
use super::ingest::{FactsUpdate, SourceAccount};
use super::packsize;
use super::prices::Currency;
use super::source::Source;
use super::types::Product;

#[derive(sqlx::FromRow)]
struct MetaRow {
    id: ProductId,
    barcode: Option<Barcode>,
    external_id: Option<ExternalId>,
    name: Option<String>,
    brand: Option<String>,
    quantity_label: Option<String>,
    source: Option<Source>,
    name_source: Option<Source>,
    image_source: Option<Source>,
}

impl From<MetaRow> for Product {
    fn from(r: MetaRow) -> Self {
        Product {
            id: r.id,
            barcode: r.barcode,
            name: r.name,
            brand: r.brand,
            // Every getter maps through here, so every product carries its pack.
            pack: r.quantity_label.as_deref().and_then(packsize::parse),
            quantity_label: r.quantity_label,
            source: r.source,
            external_id: r.external_id,
            name_source: r.name_source,
            has_image: r.image_source.is_some(),
            image_source: r.image_source,
        }
    }
}

/// The columns every getter selects, no image bytes; a macro so `concat!` can
/// append each WHERE. `get_by_source_external` joins and spells its own.
macro_rules! product_select {
    () => {
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         name_source, image_source FROM products"
    };
}

pub async fn get(pool: &MySqlPool, barcode: &Barcode) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(concat!(product_select!(), " WHERE barcode = ?"))
        .bind(barcode)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Product::from))
}

/// Name or brand search; `%`, `_` and `\` in the query match literally.
pub async fn search(pool: &MySqlPool, query: &str, limit: u64) -> Result<Vec<Product>> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let rows: Vec<MetaRow> = sqlx::query_as(concat!(
        product_select!(),
        " WHERE name LIKE ? OR brand LIKE ? ORDER BY name LIMIT ?"
    ))
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Product::from).collect())
}

pub async fn get_by_id(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    id: ProductId,
) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(concat!(product_select!(), " WHERE id = ?"))
        .bind(id)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(Product::from))
}

/// The product with a listing for (source, external_id).
pub async fn get_by_source_external(
    pool: &MySqlPool,
    source: Source,
    external_id: &ExternalId,
) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT p.id, p.barcode, p.external_id, p.name, p.brand, p.quantity_label, p.source, \
         p.name_source, p.image_source \
         FROM products p JOIN product_listings l ON l.product_id = p.id \
         WHERE l.source = ? AND l.external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// One source's listing; `raw_json` is left out, as it can be large.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Listing {
    pub source: Source,
    pub external_id: ExternalId,
    pub url: Option<String>,
    pub raw_name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    pub image_url: Option<String>,
}

/// Oldest first.
pub async fn listings_for(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    product_id: ProductId,
) -> Result<Vec<Listing>> {
    let rows = sqlx::query_as::<_, Listing>(
        "SELECT source, external_id, url, raw_name, brand, quantity_label, image_url \
         FROM product_listings WHERE product_id = ? ORDER BY created_at, id",
    )
    .bind(product_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// A source's own account, written whole on every pull, never merged with
/// another source's.
#[derive(Debug, Default, Clone)]
pub struct ListingFields<'a> {
    /// Verbatim; the canonical name is chosen separately ([`best_name`](crate::products::ingest::best_name)).
    pub raw_name: Option<&'a str>,
    pub brand: Option<&'a str>,
    pub quantity_label: Option<&'a str>,
    pub url: Option<&'a str>,
    /// A URL on the source's own CDN.
    pub image_url: Option<&'a str>,
    /// The whole record, verbatim, for whatever the columns miss.
    pub raw_json: Option<&'a str>,
}

/// Attach or refresh a listing, keyed on (source, external_id); a re-pull
/// overwrites this source's fields whole.
pub async fn upsert_listing(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    product_id: ProductId,
    source: Source,
    external_id: &ExternalId,
    fields: &ListingFields<'_>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_listings \
         (product_id, source, external_id, url, raw_name, brand, quantity_label, image_url, raw_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE product_id = VALUES(product_id), url = VALUES(url), \
         raw_name = VALUES(raw_name), brand = VALUES(brand), \
         quantity_label = VALUES(quantity_label), image_url = VALUES(image_url), \
         raw_json = VALUES(raw_json), last_seen_at = CURRENT_TIMESTAMP",
    )
    .bind(product_id)
    .bind(source)
    .bind(external_id)
    .bind(fields.url)
    .bind(fields.raw_name)
    .bind(fields.brand)
    .bind(fields.quantity_label)
    .bind(fields.image_url)
    .bind(fields.raw_json)
    .execute(conn)
    .await?;
    Ok(())
}

/// The listing id a price observation is recorded against.
pub async fn listing_id(
    pool: &MySqlPool,
    source: Source,
    external_id: &ExternalId,
) -> Result<Option<ListingId>> {
    let row: Option<(ListingId,)> =
        sqlx::query_as("SELECT id FROM product_listings WHERE source = ? AND external_id = ?")
            .bind(source)
            .bind(external_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| id))
}

/// Which shops hold a listing for each product: the attached half of
/// [[super::coverage]].
pub async fn shops_holding(pool: &MySqlPool, ids: &[ProductId]) -> Result<Vec<AttachedListing>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    // From `Source::is_shop`, so the SQL cannot fall out of step with the type.
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT product_id, source FROM product_listings WHERE source IN (",
    );
    let mut shops = qb.separated(", ");
    for shop in Source::shops() {
        shops.push_bind(shop);
    }
    qb.push(") AND product_id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let rows: Vec<(ProductId, Source)> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(product_id, source)| AttachedListing { product_id, source })
        .collect())
}

/// Each shop's latest price per product, as [`latest_prices`] does for one.
pub async fn latest_prices_for(pool: &MySqlPool, ids: &[ProductId]) -> Result<Vec<ListingPrice>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT l.product_id, l.source, po.amount_minor, po.currency \
         FROM price_observations po \
         JOIN product_listings l ON l.id = po.listing_id \
         WHERE po.id = (SELECT MAX(p2.id) FROM price_observations p2 WHERE p2.listing_id = po.listing_id) \
         AND l.product_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(") ORDER BY po.amount_minor, l.id");
    let rows: Vec<(ProductId, Source, i64, Currency)> = qb.build_query_as().fetch_all(pool).await?;
    let mut seen = std::collections::HashSet::new();
    Ok(rows
        .into_iter()
        .filter(|(product_id, source, ..)| seen.insert((*product_id, *source)))
        .map(
            |(product_id, source, amount_minor, currency)| ListingPrice {
                product_id,
                price: RowPrice {
                    source,
                    amount_minor,
                    currency,
                },
            },
        )
        .collect())
}

/// Which shops a past query showed carrying each barcode; not a stock check.
pub async fn shops_seen_carrying(pool: &MySqlPool, barcodes: &[Barcode]) -> Result<Vec<Sighting>> {
    if barcodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb =
        sqlx::QueryBuilder::new("SELECT barcode, source FROM shop_listings WHERE barcode IN (");
    let mut sep = qb.separated(", ");
    for barcode in barcodes {
        sep.push_bind(barcode);
    }
    qb.push(")");
    let rows: Vec<(Barcode, Source)> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(barcode, source)| Sighting { barcode, source })
        .collect())
}

/// A listing alone, with no price, facts or picture: [`ingest`] with nothing
/// more to say. A barcode lands it on the shared product.
pub async fn upsert_external(
    pool: &MySqlPool,
    source: Source,
    external_id: &ExternalId,
    barcode: Option<&Barcode>,
    fields: &ListingFields<'_>,
) -> Result<Product> {
    let owned = |v: Option<&str>| v.map(str::to_string);
    let account = SourceAccount {
        source,
        external_id: external_id.clone(),
        barcode: barcode.cloned(),
        name: owned(fields.raw_name),
        brand: owned(fields.brand),
        quantity_label: owned(fields.quantity_label),
        url: owned(fields.url),
        image_url: owned(fields.image_url),
        raw_json: owned(fields.raw_json),
        price: None,
        facts: FactsUpdate::None,
    };
    ingest(pool, &account, None).await
}

pub async fn get_image_by_id(pool: &MySqlPool, id: ProductId) -> Result<Option<(Vec<u8>, String)>> {
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

pub async fn get_image(pool: &MySqlPool, barcode: &Barcode) -> Result<Option<(Vec<u8>, String)>> {
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

/// Our own upload, creating a bare row if needed; `image_source = 'user'`, so
/// reconcile never offers to replace it.
pub async fn set_image(
    pool: &MySqlPool,
    barcode: &Barcode,
    bytes: &[u8],
    mime: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO products (barcode, image, image_mime, source, image_source) \
         VALUES (?, ?, ?, 'user', 'user') \
         ON DUPLICATE KEY UPDATE image = VALUES(image), \
         image_mime = VALUES(image_mime), image_source = 'user', \
         fetched_at = CURRENT_TIMESTAMP",
    )
    .bind(barcode)
    .bind(bytes)
    .bind(mime)
    .execute(pool)
    .await?;
    Ok(())
}
