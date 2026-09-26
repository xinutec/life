//! Shelf-price observations.

use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use crate::products::ids::{ExternalId, ListingId, ProductId};
use crate::products::prices::{PriceInput, ShopPrice};
use crate::products::source::Source;

/// Append a price observation to a listing's history. Prices are a time series —
/// never overwritten — so "current price" is the latest row, and history is all
/// of them.
pub async fn record_price(
    pool: &MySqlPool,
    listing_id: ListingId,
    price: &PriceInput,
) -> Result<()> {
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
    source: Source,
    external_id: ExternalId,
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
pub async fn latest_prices(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<ShopPrice>> {
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
        .filter(|r| seen.insert(r.source))
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
