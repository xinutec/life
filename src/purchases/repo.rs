//! Purchases: append them, and read a thing's price history back.

use anyhow::{Result, bail};
use chrono::{DateTime, Months, NaiveTime, TimeZone, Utc};
use sqlx::MySqlPool;

use super::types::{NewPurchase, Purchase, PurchaseId};
use crate::inventory::types::ItemId;
use crate::products::ids::ProductId;
use crate::products::prices::{UnitMeasure, UnitPrice};

/// What the buy-list row knows, copied rather than typed.
pub struct BoughtItem<'a> {
    /// The one key that always exists.
    pub id: ItemId,
    pub product_id: Option<ProductId>,
    pub barcode: Option<&'a str>,
    pub name: &'a str,
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// Validates rather than coerces: a stored "-5" would pass for a real price.
pub async fn record(
    pool: &MySqlPool,
    user_id: &str,
    item: &BoughtItem<'_>,
    p: &NewPurchase,
) -> Result<PurchaseId> {
    let shop = p.shop.trim();
    if shop.is_empty() {
        bail!("a purchase needs a shop");
    }
    if p.amount_minor < 0 {
        bail!("a purchase cannot cost a negative amount");
    }
    let bought_at = bought_at_from(p.bought_on)?;
    if let Some(months) = p.warranty_months
        && !(1..=MAX_WARRANTY_MONTHS).contains(&months)
    {
        bail!("a warranty of {months} months is not a length anybody was given");
    }
    let res = sqlx::query(
        "INSERT INTO purchases \
         (user_id, item_id, product_id, barcode, name, shop, amount_minor, currency, quantity, unit, \
          bought_at, warranty_months) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(item.id)
    .bind(item.product_id)
    .bind(item.barcode)
    .bind(item.name)
    .bind(shop)
    .bind(p.amount_minor)
    .bind(&p.currency)
    .bind(item.quantity)
    .bind(item.unit)
    .bind(bought_at)
    .bind(p.warranty_months)
    .execute(pool)
    .await?;
    Ok(res.last_insert_id().into())
}

/// Past this, somebody typed years into the months box.
const MAX_WARRANTY_MONTHS: i32 = 600;

/// The stated day, or now. A day is stored at midday UTC, where no zone offset
/// moves it; a future day is refused as a typo.
fn bought_at_from(on: Option<chrono::NaiveDate>) -> Result<DateTime<Utc>> {
    let Some(day) = on else {
        return Ok(Utc::now());
    };
    let midday = NaiveTime::from_hms_opt(12, 0, 0).expect("12:00:00 is a time");
    let Some(at) = Utc.from_local_datetime(&day.and_time(midday)).single() else {
        bail!("{day} is not a real day");
    };
    if at > Utc::now() {
        bail!("a purchase cannot have happened in the future ({day})");
    }
    Ok(at)
}

/// One cupboard item's purchases, newest first: the only way to reach one made
/// from a hand-typed row, which has no product (0044).
pub async fn for_item(pool: &MySqlPool, user_id: &str, item_id: ItemId) -> Result<Vec<Purchase>> {
    let rows = sqlx::query_as::<_, Purchase>(
        "SELECT id, item_id, product_id, barcode, name, shop, amount_minor, currency, \
         quantity, unit, bought_at, warranty_months FROM purchases \
         WHERE user_id = ? AND item_id = ? AND deleted_at IS NULL \
         ORDER BY bought_at DESC, id DESC",
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(with_derived).collect())
}

/// To the trash (0048). Scoped on `item_id` too.
pub async fn remove(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
    id: PurchaseId,
) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE purchases SET deleted_at = NOW() \
         WHERE id = ? AND user_id = ? AND item_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn restore(pool: &MySqlPool, user_id: &str, id: PurchaseId) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE purchases SET deleted_at = NULL \
         WHERE id = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// The rate shops print ("£8.00/KG"), through `packsize::parse`'s one unit table.
fn per_unit(amount_minor: i64, quantity: Option<f64>, unit: Option<&str>) -> Option<UnitPrice> {
    let (q, u) = (quantity?, unit?);
    let pack = crate::products::packsize::parse(&format!("{q}{u}"))?;
    let (scale, measure) = match pack.unit {
        crate::products::packsize::PackUnit::Gram => (1000.0, UnitMeasure::Kg),
        crate::products::packsize::PackUnit::Millilitre => (1000.0, UnitMeasure::Litre),
        crate::products::packsize::PackUnit::Count => (1.0, UnitMeasure::Each),
    };
    // Past £1,000,000 the pack was misread.
    const MAX_PENCE: f64 = 100_000_000.0;
    let amount = i32::try_from(amount_minor).ok()?;
    let rate = (f64::from(amount) * scale / pack.value).round();
    if !rate.is_finite() || !(0.0..=MAX_PENCE).contains(&rate) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "guarded above: finite and within 0..=MAX_PENCE, which i64 holds exactly"
    )]
    Some(UnitPrice {
        amount_minor: rate as i64,
        measure,
    })
}

/// Everything paid for a thing, newest first, by product id or barcode: a relinked
/// item (0043), or a purchase from before the link, is found by its barcode.
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
        "SELECT id, item_id, product_id, barcode, name, shop, amount_minor, currency, \
         quantity, unit, bought_at, warranty_months FROM purchases \
         WHERE user_id = ? AND deleted_at IS NULL \
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
    Ok(rows.into_iter().map(with_derived).collect())
}

fn with_derived(mut p: Purchase) -> Purchase {
    p.unit_price = per_unit(p.amount_minor, p.quantity, p.unit.as_deref());
    // Calendar months: two years from 3 March runs to 3 March, as on the receipt.
    p.warranty_until = p
        .warranty_months
        .and_then(|m| u32::try_from(m).ok())
        .and_then(|m| p.bought_at.checked_add_months(Months::new(m)));
    p
}
