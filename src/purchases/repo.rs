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

/// What a purchase works out to per kg / litre / item.
///
/// Reuses `packsize::parse` rather than reading "g" and "kg" again here: the
/// unit table is deliberately non-exhaustive and refuses what it does not know,
/// and a second copy of it would answer differently the first time one of them
/// learned a spelling.
///
/// Quoted per kg and per litre, not per gram, because that is the scale the
/// shop prices beside it already use ("£8.00/KG"), and a rate nobody can
/// compare is not worth showing.
fn per_unit(amount_minor: i64, quantity: Option<f64>, unit: Option<&str>) -> Option<(i64, String)> {
    let (q, u) = (quantity?, unit?);
    let pack = crate::products::packsize::parse(&format!("{q}{u}"))?;
    // parse() guarantees a finite value greater than zero, so this cannot divide
    // by zero — but the rate is still only meaningful for a positive amount.
    let (scale, measure) = match pack.unit {
        crate::products::packsize::PackUnit::Gram => (1000.0, "KG"),
        crate::products::packsize::PackUnit::Millilitre => (1000.0, "L"),
        crate::products::packsize::PackUnit::Count => (1.0, "each"),
    };
    // Same guard-then-cast shape as `products::asda`'s price parsing, and the
    // same reasoning: bound the value to a range that says something about the
    // DOMAIN, then let the cast be safe by construction rather than by hope.
    //
    // £1,000,000 in pence, for a rate as well as an amount. Not a technical
    // limit — i64 holds far more — but the point past which a per-kg price is
    // evidence the pack was misread rather than a very expensive spice.
    const MAX_PENCE: f64 = 100_000_000.0;
    let amount = i32::try_from(amount_minor).ok()?;
    let rate = (f64::from(amount) * scale / pack.value).round();
    if !rate.is_finite() || !(0.0..=MAX_PENCE).contains(&rate) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "guarded above: finite and within 0..=MAX_PENCE, which i64 holds exactly"
    )]
    Some((rate as i64, measure.to_string()))
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
    Ok(rows
        .into_iter()
        .map(|mut p| {
            if let Some((amount, measure)) = per_unit(p.amount_minor, p.quantity, p.unit.as_deref())
            {
                p.unit_amount_minor = Some(amount);
                p.unit_measure = Some(measure);
            }
            p
        })
        .collect())
}
