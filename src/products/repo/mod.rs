//! Persistence for the product catalog.

mod facts;
mod prices;
mod reconcile;

pub use facts::*;
pub use prices::*;
pub use reconcile::*;

use anyhow::Result;
use sqlx::MySqlPool;

use super::coverage::{AttachedListing, ListingPrice, RowPrice, Sighting};
use super::ids::{Barcode, ExternalId, ListingId, ProductId};
use super::packsize;
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
    has_image: i64,
}

impl From<MetaRow> for Product {
    fn from(r: MetaRow) -> Self {
        Product {
            id: r.id,
            barcode: r.barcode,
            name: r.name,
            brand: r.brand,
            // Every getter selects through this one mapping, so a product read
            // anywhere carries its pack size without each caller remembering to
            // ask for it.
            pack: r.quantity_label.as_deref().and_then(packsize::parse),
            quantity_label: r.quantity_label,
            source: r.source,
            external_id: r.external_id,
            name_source: r.name_source,
            image_source: r.image_source,
            has_image: r.has_image != 0,
        }
    }
}

/// The metadata columns every getter selects (no image bytes). A macro so
/// `concat!` can append each WHERE, since sqlx takes only `&'static str`.
///
/// ⚠ `get_by_source_external` can't use it: its join with `product_listings`
/// shares column names, so every column there is alias-qualified.
macro_rules! product_select {
    () => {
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         name_source, image_source, (image IS NOT NULL) AS has_image FROM products"
    };
}

/// Cached metadata for a barcode (no image bytes), or None if not cached.
pub async fn get(pool: &MySqlPool, barcode: &Barcode) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(concat!(product_select!(), " WHERE barcode = ?"))
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

/// Catalog row by surrogate id, or None.
pub async fn get_by_id(pool: &MySqlPool, id: ProductId) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(concat!(product_select!(), " WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Product::from))
}

/// The canonical product carrying a listing for (source, external_id), or None.
/// Resolved through `product_listings`, so it finds a product via ANY of its
/// sources — not only the one it was first created from.
///
/// Spells its columns out rather than using `product_select!` — see that macro:
/// the join makes half of them ambiguous, so each needs its alias.
pub async fn get_by_source_external(
    pool: &MySqlPool,
    source: Source,
    external_id: &ExternalId,
) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT p.id, p.barcode, p.external_id, p.name, p.brand, p.quantity_label, p.source, \
         p.name_source, p.image_source, (p.image IS NOT NULL) AS has_image \
         FROM products p JOIN product_listings l ON l.product_id = p.id \
         WHERE l.source = ? AND l.external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Product::from))
}

/// One source's listing of a canonical product — the source's own account of it.
/// `raw_json` is deliberately NOT selected here (it can be large; read it on the
/// paths that need the full record).
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

/// Every source that lists a canonical product, oldest first.
pub async fn listings_for(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<Listing>> {
    let rows = sqlx::query_as::<_, Listing>(
        "SELECT source, external_id, url, raw_name, brand, quantity_label, image_url \
         FROM product_listings WHERE product_id = ? ORDER BY created_at, id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Everything a source told us about a product, for its own listing line. Used
/// both when a source first lists a product and when a later pull refreshes it.
/// A source's line is its own account — never merged with another source's — so
/// a re-pull overwrites this source's fields with the fresh values.
#[derive(Debug, Default, Clone)]
pub struct ListingFields<'a> {
    /// The source's title, verbatim (the canonical display name is chosen among
    /// sources separately; see `refresh_canonical_name`).
    pub raw_name: Option<&'a str>,
    pub brand: Option<&'a str>,
    pub quantity_label: Option<&'a str>,
    /// Deep link to the source's product page.
    pub url: Option<&'a str>,
    /// The source's image on its own CDN (a URL, not bytes).
    pub image_url: Option<&'a str>,
    /// The source's ENTIRE record, serialized verbatim — the lossless backstop
    /// for anything the columns above don't model.
    pub raw_json: Option<&'a str>,
}

/// Attach (or refresh) a listing for (source, external_id) onto `product_id`,
/// storing the source's whole account of the product. Keyed on
/// (source, external_id): re-importing the same source id updates the same
/// listing in place (and can re-point it if products were merged). A source's
/// line is its own — never shared with another source — so a re-pull overwrites
/// this source's fields wholesale rather than COALESCE-ing them.
pub async fn upsert_listing(
    pool: &MySqlPool,
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
    .execute(pool)
    .await?;
    Ok(())
}

/// Seed an empty (or blank) canonical name from the listings: the best source
/// by `source::name_rank` with a non-blank name, oldest listing on a tie. Never
/// overwrites; a later disagreeing title surfaces as a divergence to approve.
/// Run it last on a listing-touching path, so a new product gets the best name.
pub async fn refresh_canonical_name(pool: &MySqlPool, product_id: ProductId) -> Result<()> {
    let current: Option<(Option<String>,)> =
        sqlx::query_as("SELECT name FROM products WHERE id = ?")
            .bind(product_id)
            .fetch_optional(pool)
            .await?;
    let has_name = current
        .and_then(|(n,)| n)
        .is_some_and(|n| !n.trim().is_empty());
    if has_name {
        return Ok(());
    }
    let listings = listings_for(pool, product_id).await?;
    let best = listings
        .iter()
        .filter_map(|l| {
            let name = l
                .raw_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())?;
            Some((l.source.name_rank()?, name, l.source))
        })
        .min_by_key(|(rank, ..)| *rank);
    if let Some((_, name, name_source)) = best {
        sqlx::query("UPDATE products SET name = ?, name_source = ? WHERE id = ?")
            .bind(name)
            .bind(name_source)
            .bind(product_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// The canonical product id an existing listing points at, if any.
async fn listing_product_id(
    pool: &MySqlPool,
    source: Source,
    external_id: &ExternalId,
) -> Result<Option<ProductId>> {
    let row: Option<(ProductId,)> = sqlx::query_as(
        "SELECT product_id FROM product_listings WHERE source = ? AND external_id = ?",
    )
    .bind(source)
    .bind(external_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

/// The listing id for (source, external_id) — the FK target a price observation
/// hangs off. Public: the import route records a price against the listing it
/// just upserted.
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

/// Which shops hold a listing for each of these products — the attached half of
/// [[super::coverage]]. Excludes 'off' and 'user': neither is somewhere you can
/// walk into. One query for the whole Buy list.
pub async fn shops_holding(pool: &MySqlPool, ids: &[ProductId]) -> Result<Vec<AttachedListing>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    // The exclusion is derived from the type, not spelled out in SQL: a source
    // that stops (or starts) being a shop changes this query by changing
    // `Source::is_shop`, and can't be forgotten here.
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

/// Each shop's latest price for each of these products — the priced half of
/// [[super::coverage]]. Per (product, shop): the newest observation of each
/// listing, then the cheapest listing, as [`latest_prices`] does for one product.
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
    let rows: Vec<(ProductId, Source, i64, String)> = qb.build_query_as().fetch_all(pool).await?;
    // Cheapest-first, so the first row per (product, shop) is that shop's price.
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

/// Which shops we've *seen* carry each of these barcodes, from the memory of our
/// own past shop queries (`shop_listings`). Weaker than a held listing and not a
/// stock check — see [[super::coverage]].
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

/// Find the canonical product for `barcode`, creating a bare one if absent. On a
/// hit the existing canonical fields are left untouched (see
/// `refresh_canonical_name`); on create they're seeded from the calling source.
/// Returns the canonical id.
async fn find_or_create_by_barcode(
    pool: &MySqlPool,
    barcode: &Barcode,
    name: Option<&str>,
    brand: Option<&str>,
    source: Source,
) -> Result<ProductId> {
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
    let (id,): (ProductId,) = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
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
    source: Source,
    external_id: &ExternalId,
    barcode: Option<&Barcode>,
    fields: &ListingFields<'_>,
) -> Result<Product> {
    // The source's own name/brand seed the canonical row (fill-if-empty for the
    // barcoded case; the sole authority for a barcodeless one) and are also kept
    // verbatim on the listing.
    let name = fields.raw_name;
    let brand = fields.brand;
    let product_id = if let Some(bc) = barcode {
        find_or_create_by_barcode(pool, bc, name, brand, source).await?
    } else if let Some(id) = listing_product_id(pool, source, external_id).await? {
        // A barcodeless product has a single owning source, so a re-import may
        // refresh its canonical name/brand (nothing else lists it to diverge) —
        // EXCEPT a value we've made our own, which a source refresh must never
        // clobber. Name and brand are guarded independently by their provenance.
        // `<=>` (null-safe equality) so an unset provenance reads as 0, not NULL.
        let (name_user, brand_user): (i64, i64) = sqlx::query_as(
            "SELECT (name_source <=> 'user'), (brand_source <=> 'user') FROM products WHERE id = ?",
        )
        .bind(id)
        .fetch_one(pool)
        .await?;
        match (name_user != 0, brand_user != 0) {
            (true, true) => {}
            (true, false) => {
                sqlx::query("UPDATE products SET brand = ? WHERE id = ?")
                    .bind(brand)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            (false, true) => {
                sqlx::query("UPDATE products SET name = ? WHERE id = ?")
                    .bind(name)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            (false, false) => {
                sqlx::query("UPDATE products SET name = ?, brand = ? WHERE id = ?")
                    .bind(name)
                    .bind(brand)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
        }
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
        .into()
    };
    upsert_listing(pool, product_id, source, external_id, fields).await?;
    refresh_canonical_name(pool, product_id).await?;
    get_by_id(pool, product_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("row vanished immediately after upsert"))
}

/// Cached image bytes + mime for a catalog id, if present.
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

/// Replace the image bytes for a catalog row by id (leaving metadata as-is).
pub async fn set_image_by_id(
    pool: &MySqlPool,
    id: ProductId,
    bytes: &[u8],
    mime: &str,
) -> Result<()> {
    sqlx::query("UPDATE products SET image = ?, image_mime = ?, fetched_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(bytes)
        .bind(mime)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cached image bytes + mime for a barcode, if present.
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

/// Replace just the image bytes for a barcode, leaving name/brand/quantity as
/// they are. Creates a bare catalog row if the barcode was never looked up (so
/// you can give an image to a product OFF has never heard of); `source='user'`
/// marks a hand-uploaded image, but only on insert — a later OFF metadata
/// refresh keeps its own `source`. `image_source='user'` is set on every write:
/// a hand upload is ours, so picture reconciliation never nags to replace it. The
/// unique `barcode` key drives the upsert.
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

/// Cache a product from an Open Food Facts lookup: insert it, or FILL THE GAPS
/// on a row that already exists. Never an overwrite.
///
/// The policy of every write here: fill-if-empty, and a disagreement becomes a
/// divergence to approve.
pub async fn upsert(
    pool: &MySqlPool,
    barcode: &Barcode,
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
        // `COALESCE(NULLIF(col, ''), VALUES(col))` = keep what's there unless it's
        // absent or blank. `image_mime` is assigned BEFORE `image` on purpose:
        // ON DUPLICATE KEY assignments evaluate left to right, so this is the one
        // ordering in which `image IS NULL` still describes the OLD image and the
        // mime can't be left describing bytes we didn't take.
        "INSERT INTO products \
         (barcode, name, brand, quantity_label, image, image_mime, source, name_source) \
         VALUES (?, ?, ?, ?, ?, ?, 'off', 'off') \
         ON DUPLICATE KEY UPDATE name = COALESCE(NULLIF(name, ''), VALUES(name)), \
         brand = COALESCE(NULLIF(brand, ''), VALUES(brand)), \
         quantity_label = COALESCE(NULLIF(quantity_label, ''), VALUES(quantity_label)), \
         image_mime = IF(image IS NULL, VALUES(image_mime), image_mime), \
         image = COALESCE(image, VALUES(image)), \
         fetched_at = CURRENT_TIMESTAMP",
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
