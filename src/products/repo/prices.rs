//! Shelf-price observations.

use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use crate::products::ids::{ExternalId, ListingId, ProductId};
use crate::products::prices::{Currency, PriceInput, ShopPrice, UnitMeasure, UnitPrice};
use crate::products::source::Source;

/// Appended, never overwritten: the latest row is the current price.
pub async fn record_price(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    listing_id: ListingId,
    price: &PriceInput,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO price_observations \
         (listing_id, amount_minor, currency, unit_amount_minor, unit_measure) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(listing_id)
    .bind(price.amount_minor)
    .bind(&price.currency)
    .bind(price.unit_price.map(|u| u.amount_minor))
    .bind(price.unit_price.map(|u| u.measure))
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct ShopPriceRow {
    source: Source,
    external_id: ExternalId,
    amount_minor: i64,
    currency: Currency,
    unit_amount_minor: Option<i64>,
    unit_measure: Option<UnitMeasure>,
    observed_at: NaiveDateTime,
}

/// Cheapest shop first, one row per shop: each listing's newest observation, and
/// a shop listing the product twice collapses to its cheaper listing.
pub async fn latest_prices(pool: &MySqlPool, product_id: ProductId) -> Result<Vec<ShopPrice>> {
    let rows: Vec<ShopPriceRow> = sqlx::query_as(
        "SELECT l.source, l.external_id, po.amount_minor, po.currency, po.unit_amount_minor, \
         po.unit_measure, po.observed_at \
         FROM price_observations po \
         JOIN product_listings l ON l.id = po.listing_id \
         WHERE l.product_id = ? \
         AND po.id = (SELECT MAX(p2.id) FROM price_observations p2 WHERE p2.listing_id = po.listing_id) \
         ORDER BY po.amount_minor, l.id",
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;
    // Cheapest first, so a shop's first row is its best price.
    let mut seen = std::collections::HashSet::new();
    Ok(rows
        .into_iter()
        .filter(|r| seen.insert(r.source))
        .map(|r| ShopPrice {
            source: r.source,
            external_id: r.external_id,
            amount_minor: r.amount_minor,
            currency: r.currency,
            unit_price: r
                .unit_amount_minor
                .zip(r.unit_measure)
                .map(|(amount_minor, measure)| UnitPrice {
                    amount_minor,
                    measure,
                }),
            observed_at: r.observed_at.and_utc(),
        })
        .collect())
}
