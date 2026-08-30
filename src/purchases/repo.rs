//! Storage for purchases: append them, and read a thing's price history back.

use anyhow::{Result, bail};
use sqlx::MySqlPool;

use super::types::{NewPurchase, Purchase};
use crate::products::ids::ProductId;

/// What the buy-list row already knows about the thing being bought. Passed
/// separately from [`NewPurchase`] because none of it is typed by the person —
/// it is copied from the row, so the capture step asks only for shop and price.
pub struct BoughtItem<'a> {
    pub product_id: Option<ProductId>,
    pub barcode: Option<&'a str>,
    pub name: &'a str,
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// Longest ISO 4217 code, and the column width.
const CURRENCY_LEN: usize = 3;

/// Record a purchase.
///
/// Validates rather than coerces. A price of "-5" or a currency of "pounds" is a
/// client bug or a fat finger, and silently storing either would put a number
/// into the spending history that nobody can later tell apart from a real one —
/// the whole value of this table is that its numbers are true.
pub async fn record(
    pool: &MySqlPool,
    user_id: &str,
    item: &BoughtItem<'_>,
    p: &NewPurchase,
) -> Result<u64> {
    let shop = p.shop.trim();
    if shop.is_empty() {
        bail!("a purchase needs a shop");
    }
    if p.amount_minor < 0 {
        bail!("a purchase cannot cost a negative amount");
    }
    let currency = p.currency.trim().to_ascii_uppercase();
    if currency.len() != CURRENCY_LEN || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
        bail!(
            "currency must be a 3-letter ISO 4217 code, got {:?}",
            p.currency
        );
    }
    let res = sqlx::query(
        "INSERT INTO purchases \
         (user_id, product_id, barcode, name, shop, amount_minor, currency, quantity, unit) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(item.product_id)
    .bind(item.barcode)
    .bind(item.name)
    .bind(shop)
    .bind(p.amount_minor)
    .bind(&currency)
    .bind(item.quantity)
    .bind(item.unit)
    .execute(pool)
    .await?;
    Ok(res.last_insert_id())
}

/// Everything this person has paid for a thing, newest first.
///
/// Matched on product id OR barcode, not on product id alone. An item can be
/// linked to the wrong product and later relinked (see migration 0043); the
/// barcode is what re-attaches the history when that happens, and a purchase
/// made before the link existed has only the barcode to be found by.
pub async fn history(
    pool: &MySqlPool,
    user_id: &str,
    product_id: Option<ProductId>,
    barcode: Option<&str>,
) -> Result<Vec<Purchase>> {
    if product_id.is_none() && barcode.is_none() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<_, Purchase>(
        "SELECT id, product_id, barcode, name, shop, amount_minor, currency, \
         quantity, unit, bought_at FROM purchases \
         WHERE user_id = ? \
           AND ((? IS NOT NULL AND product_id = ?) OR (? IS NOT NULL AND barcode = ?)) \
         ORDER BY bought_at DESC, id DESC",
    )
    .bind(user_id)
    .bind(product_id)
    .bind(product_id)
    .bind(barcode)
    .bind(barcode)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
