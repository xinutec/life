//! Persistence for the product cache.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::MySqlPool;
use sqlx::types::Json;

use super::nutrition::{Allergen, DietaryFlag, Nutrition, ProductFacts};
use super::prices::{PriceInput, ShopPrice};
use super::source;
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
    name_source: Option<String>,
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
            name_source: r.name_source,
            has_image: r.has_image != 0,
        }
    }
}

// The metadata columns every getter selects (no image bytes). Kept in sync by
// hand across the getters below — sqlx 0.8 only accepts `&'static str`
// SQL (its injection guard), so this can't be a shared runtime `format!`.
// "SELECT id, barcode, external_id, name, brand, quantity_label, source,
//  name_source, (image IS NOT NULL) AS has_image FROM products WHERE …"

/// Cached metadata for a barcode (no image bytes), or None if not cached.
pub async fn get(pool: &MySqlPool, barcode: &str) -> Result<Option<Product>> {
    let row: Option<MetaRow> = sqlx::query_as(
        "SELECT id, barcode, external_id, name, brand, quantity_label, source, \
         name_source, (image IS NOT NULL) AS has_image FROM products WHERE barcode = ?",
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
         name_source, (image IS NOT NULL) AS has_image FROM products \
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
         name_source, (image IS NOT NULL) AS has_image FROM products WHERE id = ?",
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
         p.name_source, (p.image IS NOT NULL) AS has_image \
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

/// Recompute the canonical display name from the product's listings: the
/// highest-preference source (see source::name_rank) with a non-blank raw name
/// wins — deterministically, so a re-import can never flip the name on recency,
/// only on source quality. Ties within a source go to its oldest listing
/// (listings_for order). No ranked candidate → the row is left untouched, so a
/// product named by an unranked path keeps its name.
///
/// Other paths do write `name` first — `upsert_external` seeds it for a
/// barcodeless product, `upsert` restates OFF's crowd name on a cache fill —
/// so this must run LAST on any path that touches a listing, and have the final
/// say. That ordering is what keeps a retailer's title from being clobbered by
/// a later Open Food Facts lookup of the same barcode.
pub async fn refresh_canonical_name(pool: &MySqlPool, product_id: u64) -> Result<()> {
    let listings = listings_for(pool, product_id).await?;
    let best = listings
        .iter()
        .filter_map(|l| {
            let name = l
                .raw_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())?;
            Some((source::name_rank(&l.source)?, name, l.source.as_str()))
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

/// The listing id for (source, external_id) — the FK target a price observation
/// hangs off. Public: the import route records a price against the listing it
/// just upserted.
pub async fn listing_id(pool: &MySqlPool, source: &str, external_id: &str) -> Result<Option<u64>> {
    let row: Option<(u64,)> =
        sqlx::query_as("SELECT id FROM product_listings WHERE source = ? AND external_id = ?")
            .bind(source)
            .bind(external_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(id,)| id))
}

/// Append a price observation to a listing's history. Prices are a time series —
/// never overwritten — so "current price" is the latest row, and history is all
/// of them.
pub async fn record_price(pool: &MySqlPool, listing_id: u64, price: &PriceInput) -> Result<()> {
    sqlx::query(
        "INSERT INTO price_observations \
         (listing_id, amount_minor, currency, region, unit_amount_minor, unit_measure) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(listing_id)
    .bind(price.amount_minor)
    .bind(&price.currency)
    .bind(price.region.as_deref())
    .bind(price.unit_amount_minor)
    .bind(price.unit_measure.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct ShopPriceRow {
    source: String,
    external_id: String,
    amount_minor: i64,
    currency: String,
    unit_amount_minor: Option<i64>,
    unit_measure: Option<String>,
    region: Option<String>,
    observed_at: NaiveDateTime,
}

/// What each shop currently charges for this product, cheapest shop first —
/// feeds the "available at Asda £X · Waitrose £Y" view.
///
/// Each listing contributes its most recent observation (prices are a time
/// series; the newest row is "current"). A shop listing the product twice —
/// two Asda CINs on one EAN — collapses to its cheapest listing, so the result
/// holds exactly one row per source, as `ShopPrice` promises.
pub async fn latest_prices(pool: &MySqlPool, product_id: u64) -> Result<Vec<ShopPrice>> {
    let rows: Vec<ShopPriceRow> = sqlx::query_as(
        "SELECT l.source, l.external_id, po.amount_minor, po.currency, po.unit_amount_minor, \
         po.unit_measure, po.region, po.observed_at \
         FROM price_observations po \
         JOIN product_listings l ON l.id = po.listing_id \
         WHERE l.product_id = ? \
         AND po.id = (SELECT MAX(p2.id) FROM price_observations p2 WHERE p2.listing_id = po.listing_id) \
         ORDER BY po.amount_minor, l.id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    // Cheapest-first already, so the first row for a source IS that shop's best
    // price; later rows from the same shop are its dearer listings.
    let mut seen = std::collections::HashSet::new();
    Ok(rows
        .into_iter()
        .filter(|r| seen.insert(r.source.clone()))
        .map(|r| ShopPrice {
            source: r.source,
            external_id: r.external_id,
            amount_minor: r.amount_minor,
            currency: r.currency,
            unit_amount_minor: r.unit_amount_minor,
            unit_measure: r.unit_measure,
            region: r.region,
            observed_at: r.observed_at.and_utc().timestamp_millis(),
        })
        .collect())
}

// --- Product facts: nutrition, ingredients, allergens, dietary flags ---
//
// Facts describe the physical product, so they hang off `products.id` and are
// reconciled by barcode like every other enrichment. They come from a single
// authority per fact (Open Food Facts today), so writes are whole-product
// REPLACE: re-looking-up a barcode restates the product's facts in full.

#[derive(sqlx::FromRow)]
struct NutritionRow {
    basis: String,
    serving_size: Option<String>,
    energy_kj: Option<f64>,
    energy_kcal: Option<f64>,
    fat_g: Option<f64>,
    saturates_g: Option<f64>,
    carbohydrate_g: Option<f64>,
    sugars_g: Option<f64>,
    fibre_g: Option<f64>,
    protein_g: Option<f64>,
    salt_g: Option<f64>,
    extra: Option<Json<BTreeMap<String, f64>>>,
}

/// Upsert the 1:1 nutrition panel for a product.
pub async fn upsert_nutrition(
    pool: &MySqlPool,
    product_id: u64,
    n: &Nutrition,
    source: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO product_nutrition \
         (product_id, basis, serving_size, energy_kj, energy_kcal, fat_g, saturates_g, \
          carbohydrate_g, sugars_g, fibre_g, protein_g, salt_g, extra, source) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE basis = VALUES(basis), serving_size = VALUES(serving_size), \
          energy_kj = VALUES(energy_kj), energy_kcal = VALUES(energy_kcal), fat_g = VALUES(fat_g), \
          saturates_g = VALUES(saturates_g), carbohydrate_g = VALUES(carbohydrate_g), \
          sugars_g = VALUES(sugars_g), fibre_g = VALUES(fibre_g), protein_g = VALUES(protein_g), \
          salt_g = VALUES(salt_g), extra = VALUES(extra), source = VALUES(source)",
    )
    .bind(product_id)
    .bind(&n.basis)
    .bind(&n.serving_size)
    .bind(n.energy_kj)
    .bind(n.energy_kcal)
    .bind(n.fat_g)
    .bind(n.saturates_g)
    .bind(n.carbohydrate_g)
    .bind(n.sugars_g)
    .bind(n.fibre_g)
    .bind(n.protein_g)
    .bind(n.salt_g)
    .bind(Json(&n.extra))
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set the product's ingredients text (a single block, source-tracked).
pub async fn set_ingredients(
    pool: &MySqlPool,
    product_id: u64,
    text: &str,
    source: &str,
) -> Result<()> {
    sqlx::query("UPDATE products SET ingredients_text = ?, ingredients_source = ? WHERE id = ?")
        .bind(text)
        .bind(source)
        .bind(product_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Replace the product's allergen set. Whole-product replace: the incoming set
/// is authoritative (an empty set clears the product's allergens).
pub async fn replace_allergens(
    pool: &MySqlPool,
    product_id: u64,
    allergens: &[Allergen],
    source: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM product_allergens WHERE product_id = ?")
        .bind(product_id)
        .execute(pool)
        .await?;
    for a in allergens {
        sqlx::query(
            "INSERT INTO product_allergens (product_id, allergen, presence, source) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(product_id)
        .bind(&a.allergen)
        .bind(&a.presence)
        .bind(source)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Replace the product's dietary flags. Whole-product replace, like allergens.
pub async fn replace_dietary(
    pool: &MySqlPool,
    product_id: u64,
    flags: &[DietaryFlag],
    source: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM product_dietary_flags WHERE product_id = ?")
        .bind(product_id)
        .execute(pool)
        .await?;
    for f in flags {
        sqlx::query(
            "INSERT INTO product_dietary_flags (product_id, flag, value, source) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(product_id)
        .bind(&f.flag)
        .bind(&f.value)
        .bind(source)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Persist a product's full fact set from one source, each part restated. Skips
/// nutrition/ingredients the source didn't provide (leaving any existing rows);
/// allergens and dietary flags always replace (their absence is meaningful).
pub async fn store_facts(
    pool: &MySqlPool,
    product_id: u64,
    facts: &ProductFacts,
    source: &str,
) -> Result<()> {
    if let Some(n) = &facts.nutrition {
        upsert_nutrition(pool, product_id, n, source).await?;
    }
    if let Some(ing) = &facts.ingredients {
        set_ingredients(pool, product_id, ing, source).await?;
    }
    replace_allergens(pool, product_id, &facts.allergens, source).await?;
    replace_dietary(pool, product_id, &facts.dietary, source).await?;
    Ok(())
}

/// Read back everything we know about a product beyond its identity. Feeds
/// the product detail (GET /api/products/id/{id}) — the rich product page.
pub async fn facts_for(pool: &MySqlPool, product_id: u64) -> Result<ProductFacts> {
    let nrow: Option<NutritionRow> = sqlx::query_as(
        "SELECT basis, serving_size, energy_kj, energy_kcal, fat_g, saturates_g, \
         carbohydrate_g, sugars_g, fibre_g, protein_g, salt_g, extra \
         FROM product_nutrition WHERE product_id = ?",
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await?;
    let nutrition = nrow.map(|r| Nutrition {
        basis: r.basis,
        serving_size: r.serving_size,
        energy_kj: r.energy_kj,
        energy_kcal: r.energy_kcal,
        fat_g: r.fat_g,
        saturates_g: r.saturates_g,
        carbohydrate_g: r.carbohydrate_g,
        sugars_g: r.sugars_g,
        fibre_g: r.fibre_g,
        protein_g: r.protein_g,
        salt_g: r.salt_g,
        extra: r.extra.map(|j| j.0).unwrap_or_default(),
    });

    // `ingredients_text` is NULLable → the column decodes as Option<String>; the
    // outer Option is row presence. (query_as tuple form, not query_scalar, so the
    // nullable column type stays visible to DL-SQLX-ROW-TYPES.)
    let ingredients: Option<(Option<String>,)> =
        sqlx::query_as("SELECT ingredients_text FROM products WHERE id = ?")
            .bind(product_id)
            .fetch_optional(pool)
            .await?;
    let ingredients = ingredients.and_then(|(t,)| t);

    let allergens: Vec<(String, String)> = sqlx::query_as(
        "SELECT allergen, presence FROM product_allergens WHERE product_id = ? ORDER BY allergen",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    let allergens = allergens
        .into_iter()
        .map(|(allergen, presence)| Allergen { allergen, presence })
        .collect();

    let dietary: Vec<(String, String)> = sqlx::query_as(
        "SELECT flag, value FROM product_dietary_flags WHERE product_id = ? ORDER BY flag",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    let dietary = dietary
        .into_iter()
        .map(|(flag, value)| DietaryFlag { flag, value })
        .collect();

    Ok(ProductFacts {
        nutrition,
        ingredients,
        allergens,
        dietary,
    })
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
    refresh_canonical_name(pool, product_id).await?;
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
