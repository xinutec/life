//! Persistence for locations and items. `position` is stored as JSON text and
//! parsed here, so it survives however MariaDB reports the JSON column type.

use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use sqlx::MySqlPool;

use super::consume::{self, Taken};
use super::types::{
    ExpiryPrecision, Item, ItemCategory, ItemEvent, ItemHistoryEntry, ItemNameSource, Location,
    LocationKind, NewItem, NewLocation,
};
use crate::products::ids::{Barcode, ProductId};

#[derive(sqlx::FromRow)]
struct LocationRow {
    id: u64,
    kind: String,
    name: String,
    parent_id: Option<u64>,
    sort_order: i32,
    position: Option<String>,
}

impl LocationRow {
    fn into_location(self) -> Result<Location> {
        let kind = LocationKind::from_str(&self.kind).map_err(|e| anyhow!(e))?;
        let position = match self.position {
            Some(s) => Some(serde_json::from_str(&s).context("parsing location.position")?),
            None => None,
        };
        Ok(Location {
            id: self.id,
            kind,
            name: self.name,
            parent_id: self.parent_id,
            sort_order: self.sort_order,
            position,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: u64,
    product_id: Option<ProductId>,
    name: String,
    brand: Option<String>,
    category: String,
    quantity: Option<f64>,
    unit: Option<String>,
    expiry: Option<NaiveDate>,
    expiry_precision: String,
    location_id: Option<u64>,
    barcode: Option<String>,
    // A boolean SQL expression decodes as an integer.
    has_image: i64,
}

impl ItemRow {
    fn into_item(self) -> Result<Item> {
        let category = ItemCategory::from_str(&self.category).map_err(|e| anyhow!(e))?;
        let expiry_precision =
            ExpiryPrecision::from_str(&self.expiry_precision).map_err(|e| anyhow!(e))?;
        Ok(Item {
            id: self.id,
            product_id: self.product_id,
            name: self.name,
            brand: self.brand,
            category,
            quantity: self.quantity,
            unit: self.unit,
            expiry: self.expiry,
            expiry_precision,
            location_id: self.location_id,
            barcode: self.barcode,
            has_image: self.has_image != 0,
        })
    }
}

/// The resolved item read: holding fields from `items`, display fields
/// (name/brand/barcode/image) resolved against the linked catalog product. A
/// macro (not a const) so it stays a compile-time literal — sqlx rejects
/// runtime-built query strings.
///
/// The name is resolved by PROVENANCE, not by precedence. This used to be a
/// plain `COALESCE(p.name, i.name, '')`, so the catalogue won whenever it had
/// anything at all — including an Open Food Facts record whose "name" is a
/// marketing sentence, which replaced a one-word name with a line of shouting
/// and could not be overruled. Preferring the item's name instead is wrong in
/// the same way, just less loudly: a hand-typed shorthand would outrank a proper
/// name with its brand and pack size. So `items.name_source` says which name was
/// MEANT, and only a 'user' one outranks the catalogue (migration 0042).
macro_rules! item_select {
    () => {
        "SELECT i.id AS id, i.product_id AS product_id, \
         CASE WHEN i.name_source = 'user' THEN COALESCE(i.name, p.name, '') \
              ELSE COALESCE(p.name, i.name, '') END AS name, \
         p.brand AS brand, i.category AS category, \
         i.quantity AS quantity, i.unit AS unit, i.expiry AS expiry, \
         i.expiry_precision AS expiry_precision, i.location_id AS location_id, \
         COALESCE(i.barcode, p.barcode) AS barcode, (p.image IS NOT NULL) AS has_image \
         FROM items i LEFT JOIN products p ON p.id = i.product_id"
    };
}

/// The `name_source` an UPDATE should write.
///
/// `None` from the client is not "product" — it is "no statement", and it must
/// leave an existing choice alone. Every caller that is not the item form sends
/// nothing here (sync, scripts, the Android app), and any of them re-saving an
/// item would otherwise silently strip a name its owner had chosen to keep.
async fn name_source_for_update(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    stated: Option<ItemNameSource>,
) -> Result<String> {
    if let Some(s) = stated {
        return Ok(s.to_string());
    }
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT name_source FROM items WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(existing.map_or_else(|| ItemNameSource::Product.to_string(), |(v,)| v))
}

/// The `expiry_precision` an UPDATE should write.
///
/// `None` is "no statement", not "day", and the difference is the whole point of
/// the column: an update that quietly wrote `day` would re-print the invented
/// 30th of a month-precision box as a real one, which is exactly the fault
/// migration 0045 exists to stop. Only the item form knows whether a person
/// picked a month or a day, so only it says.
async fn expiry_precision_for_update(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    stated: Option<ExpiryPrecision>,
) -> Result<String> {
    if let Some(p) = stated {
        return Ok(p.to_string());
    }
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT expiry_precision FROM items WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(existing.map_or_else(|| ExpiryPrecision::Day.to_string(), |(v,)| v))
}

/// The catalog link for a new/updated item: an explicit `product_id` wins (it's
/// the only route to a barcodeless shop product), else fall back to matching the
/// barcode against the cached catalog.
async fn resolve_product_id(pool: &MySqlPool, new: &NewItem) -> Result<Option<ProductId>> {
    if new.product_id.is_some() {
        return Ok(new.product_id);
    }
    product_id_for_barcode(pool, new.barcode.as_deref()).await
}

/// Resolve the catalog product id for a barcode, if one is cached.
///
/// An item's barcode is whatever was scanned or typed, so it is parsed rather
/// than queried directly: something that isn't a barcode matches no product, and
/// a blank one would otherwise match every barcodeless row in the catalog.
async fn product_id_for_barcode(
    pool: &MySqlPool,
    barcode: Option<&str>,
) -> Result<Option<ProductId>> {
    let Some(bc) = barcode.and_then(|b| b.parse::<Barcode>().ok()) else {
        return Ok(None);
    };
    let row: Option<(ProductId,)> = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
        .bind(bc)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

pub async fn list_locations(pool: &MySqlPool, user_id: &str) -> Result<Vec<Location>> {
    let rows: Vec<LocationRow> = sqlx::query_as(
        "SELECT id, kind, name, parent_id, sort_order, position FROM locations \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY parent_id, sort_order, id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(LocationRow::into_location).collect()
}

pub async fn create_location(
    pool: &MySqlPool,
    user_id: &str,
    new: NewLocation,
) -> Result<Location> {
    let position_str = match &new.position {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    let res = sqlx::query(
        "INSERT INTO locations (user_id, kind, name, parent_id, sort_order, position) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(new.kind.to_string())
    .bind(&new.name)
    .bind(new.parent_id)
    .bind(new.sort_order)
    .bind(&position_str)
    .execute(pool)
    .await?;
    Ok(Location {
        id: res.last_insert_id(),
        kind: new.kind,
        name: new.name,
        parent_id: new.parent_id,
        sort_order: new.sort_order,
        position: new.position,
    })
}

pub async fn list_items(pool: &MySqlPool, user_id: &str) -> Result<Vec<Item>> {
    let rows: Vec<ItemRow> = sqlx::query_as(concat!(
        item_select!(),
        " WHERE i.user_id = ? AND i.deleted_at IS NULL ORDER BY name"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(ItemRow::into_item).collect()
}

pub async fn get_item(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Item>> {
    let row: Option<ItemRow> = sqlx::query_as(concat!(
        item_select!(),
        " WHERE i.id = ? AND i.user_id = ? AND i.deleted_at IS NULL"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    row.map(ItemRow::into_item).transpose()
}

pub async fn create_item(pool: &MySqlPool, user_id: &str, new: NewItem) -> Result<Item> {
    // Prefer an explicit catalog link (the only way to reach a barcodeless shop
    // product); otherwise link by barcode when it's already known (scanned/looked up).
    let product_id = resolve_product_id(pool, &new).await?;
    // A name typed while ADDING is a scribble — you type "cheese" and then scan,
    // and the form fills the product's name in only if the box is still empty. So
    // the catalogue wins unless the client explicitly says the name is the
    // person's, which `tests/catalog_db.rs` has always required.
    let name_source = new
        .name_source
        .unwrap_or(ItemNameSource::Product)
        .to_string();
    let res = sqlx::query(
        "INSERT INTO items \
         (user_id, product_id, name, name_source, category, quantity, unit, expiry, \
          expiry_precision, location_id, barcode) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(product_id)
    .bind(&new.name)
    .bind(&name_source)
    .bind(new.category.to_string())
    .bind(new.quantity)
    .bind(&new.unit)
    .bind(new.expiry)
    .bind(
        new.expiry_precision
            .unwrap_or(ExpiryPrecision::Day)
            .to_string(),
    )
    .bind(new.location_id)
    .bind(&new.barcode)
    .execute(pool)
    .await?;
    let id = res.last_insert_id();
    record_history(
        pool,
        id,
        user_id,
        new.location_id,
        ItemEvent::Added,
        new.quantity,
    )
    .await?;
    get_item(pool, user_id, id)
        .await?
        .ok_or_else(|| anyhow!("created item {id} not found"))
}

/// Move an item to a new location (or `None` to detach). Returns the updated
/// item, or `None` if no such item belongs to this user.
pub async fn move_item(
    pool: &MySqlPool,
    user_id: &str,
    item_id: u64,
    new_location_id: Option<u64>,
) -> Result<Option<Item>> {
    if get_item(pool, user_id, item_id).await?.is_none() {
        return Ok(None);
    }
    sqlx::query("UPDATE items SET location_id = ? WHERE id = ? AND user_id = ?")
        .bind(new_location_id)
        .bind(item_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    record_history(
        pool,
        item_id,
        user_id,
        new_location_id,
        ItemEvent::Moved,
        None,
    )
    .await?;
    get_item(pool, user_id, item_id).await
}

/// Update every field of an item. Returns the updated item, or `None` if no
/// such item belongs to the user. Records a `moved` history row if the location
/// changed.
pub async fn update_item(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    new: NewItem,
) -> Result<Option<Item>> {
    let Some(existing) = get_item(pool, user_id, id).await? else {
        return Ok(None);
    };
    let product_id = resolve_product_id(pool, &new).await?;
    let name_source = name_source_for_update(pool, user_id, id, new.name_source).await?;
    let expiry_precision =
        expiry_precision_for_update(pool, user_id, id, new.expiry_precision).await?;
    sqlx::query(
        "UPDATE items SET product_id = ?, name = ?, name_source = ?, category = ?, quantity = ?, unit = ?, \
         expiry = ?, expiry_precision = ?, location_id = ?, barcode = ? \
         WHERE id = ? AND user_id = ?",
    )
    .bind(product_id)
    .bind(&new.name)
    .bind(name_source)
    .bind(new.category.to_string())
    .bind(new.quantity)
    .bind(&new.unit)
    .bind(new.expiry)
    .bind(expiry_precision)
    .bind(new.location_id)
    .bind(&new.barcode)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    if existing.location_id != new.location_id {
        record_history(
            pool,
            id,
            user_id,
            new.location_id,
            ItemEvent::Moved,
            new.quantity,
        )
        .await?;
    }
    get_item(pool, user_id, id).await
}

/// Take an amount out of a stock row: "I used 200g of flour."
///
/// The read and the write happen in one transaction, with the row locked, so
/// two phones cooking at once can't both read 950 and both write 750. The
/// decision itself is [`consume::take`] — pure, and the only place the rule
/// lives.
///
/// `Ok(None)` = no such (live) item for this user. The [`Taken`] outcome is
/// handed back rather than turned into an error here: the route is what knows
/// how to say "that's measured in jars" to a person.
pub async fn use_item(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    want: f64,
    want_unit: Option<&str>,
) -> Result<Option<(Taken, Option<Item>)>> {
    let mut tx = pool.begin().await?;
    // FOR UPDATE: the whole point of the transaction. Without it the subtraction
    // is a read-modify-write race and stock quietly drifts upward.
    let row: Option<(Option<f64>, Option<String>, Option<u64>)> = sqlx::query_as(
        "SELECT quantity, unit, location_id FROM items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((quantity, unit, location_id)) = row else {
        return Ok(None);
    };

    // `take` reads an Item; only these three fields matter to it, so build the
    // smallest honest one rather than re-reading the resolved row inside the
    // lock.
    let held = Item {
        id,
        product_id: None,
        name: String::new(),
        brand: None,
        category: ItemCategory::Other,
        quantity,
        unit,
        expiry: None,
        expiry_precision: ExpiryPrecision::Day,
        location_id,
        barcode: None,
        has_image: false,
    };
    let outcome = consume::take(&held, want, want_unit);
    let left = match outcome {
        Taken::Left(n) => n,
        Taken::Emptied { .. } => 0.0,
        // Nothing to write: the row keeps whatever it had.
        Taken::UnitMismatch | Taken::Untracked => {
            tx.rollback().await?;
            return Ok(Some((outcome, get_item(pool, user_id, id).await?)));
        }
    };
    sqlx::query("UPDATE items SET quantity = ? WHERE id = ? AND user_id = ?")
        .bind(left)
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    // The history row records the DELTA, not the new level — "200g went" is the
    // fact a consumption rate is later computed from, and it survives an edit
    // that resets the quantity by hand.
    let took = match outcome {
        Taken::Emptied { short } => want - short,
        _ => want,
    };
    sqlx::query(
        "INSERT INTO item_history (item_id, user_id, location_id, event, quantity) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(location_id)
    .bind(ItemEvent::Used)
    .bind(took)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some((outcome, get_item(pool, user_id, id).await?)))
}

/// Delete an item — a tombstone, restorable from the trash; history is kept.
/// Returns whether a row was tombstoned.
pub async fn delete_item(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE items SET deleted_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    let deleted = res.rows_affected() > 0;
    if deleted {
        record_history(pool, id, user_id, None, ItemEvent::Removed, None).await?;
    }
    Ok(deleted)
}

/// Restore a deleted item. Returns whether a tombstone was cleared.
pub async fn restore_item(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let res = sqlx::query(
        "UPDATE items SET deleted_at = NULL \
         WHERE id = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    let restored = res.rows_affected() > 0;
    if restored {
        record_history(pool, id, user_id, None, ItemEvent::Restored, None).await?;
    }
    Ok(restored)
}

/// Every location id in the subtree rooted at `root` (inclusive), computed from
/// ALL of the user's rows (deleted or not — parent links stay intact under
/// tombstoning). Empty if `root` isn't the user's.
async fn subtree_ids(pool: &MySqlPool, user_id: &str, root: u64) -> Result<Vec<u64>> {
    let rows: Vec<(u64, Option<u64>)> =
        sqlx::query_as("SELECT id, parent_id FROM locations WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    if !rows.iter().any(|(id, _)| *id == root) {
        return Ok(Vec::new());
    }
    let mut children: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    for (id, parent) in &rows {
        if let Some(p) = parent {
            children.entry(*p).or_default().push(*id);
        }
    }
    let mut ids = vec![root];
    let mut i = 0;
    while i < ids.len() {
        if let Some(kids) = children.get(&ids[i]) {
            ids.extend(kids);
        }
        i += 1;
    }
    Ok(ids)
}

/// Delete a location and its whole subtree — tombstones, restorable as one unit
/// (every row gets the SAME `deleted_at` stamp; restore keys on it). Items keep
/// their `location_id`: with the location hidden they read as unplaced, and a
/// restore puts them right back where they were. Returns whether the root was
/// tombstoned.
pub async fn delete_location(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let ids = subtree_ids(pool, user_id, id).await?;
    if ids.is_empty() {
        return Ok(false);
    }
    let mut qb =
        sqlx::QueryBuilder::new("UPDATE locations SET deleted_at = NOW() WHERE user_id = ");
    qb.push_bind(user_id);
    qb.push(" AND deleted_at IS NULL AND id IN (");
    let mut sep = qb.separated(", ");
    for i in &ids {
        sep.push_bind(i);
    }
    qb.push(")");
    let res = qb.build().execute(pool).await?;
    Ok(res.rows_affected() > 0)
}

/// Restore a deleted location together with the descendants that were deleted
/// in the same operation (same `deleted_at` stamp — descendants deleted
/// separately earlier stay in the trash as their own entries). Returns whether
/// anything was restored.
pub async fn restore_location(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let stamp: Option<(Option<chrono::NaiveDateTime>,)> =
        sqlx::query_as("SELECT deleted_at FROM locations WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((Some(stamp),)) = stamp else {
        return Ok(false); // unknown, someone else's, or not deleted
    };
    let ids = subtree_ids(pool, user_id, id).await?;
    let mut qb = sqlx::QueryBuilder::new("UPDATE locations SET deleted_at = NULL WHERE user_id = ");
    qb.push_bind(user_id);
    qb.push(" AND deleted_at = ");
    qb.push_bind(stamp);
    qb.push(" AND id IN (");
    let mut sep = qb.separated(", ");
    for i in &ids {
        sep.push_bind(i);
    }
    qb.push(")");
    let res = qb.build().execute(pool).await?;
    Ok(res.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct HistoryRow {
    id: u64,
    event: String,
    quantity: Option<f64>,
    location: Option<String>,
    at: chrono::NaiveDateTime,
}

/// Everything that has happened to one stock row, newest first.
///
/// Scoped on `h.user_id` and not merely on the item's: the history table
/// carries its own copy of who did it, and reading through the item would hand
/// back another user's rows if an item ever changed hands. Empty is a real
/// answer — a row added before the audit existed has no history, and so does
/// an id belonging to somebody else, which is the same thing as far as this
/// caller is allowed to know.
pub async fn item_history(
    pool: &MySqlPool,
    user_id: &str,
    item_id: u64,
) -> Result<Vec<ItemHistoryEntry>> {
    let rows: Vec<HistoryRow> = sqlx::query_as(
        "SELECT h.id, h.event, h.quantity, l.name AS location, h.at \
         FROM item_history h \
         LEFT JOIN locations l ON l.id = h.location_id \
         WHERE h.item_id = ? AND h.user_id = ? \
         ORDER BY h.at DESC, h.id DESC",
    )
    .bind(item_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(ItemHistoryEntry {
                id: r.id,
                // A stored event outside the enum fails the read loudly rather
                // than being dropped or shown as a blank line — the same rule
                // products::Source is read under.
                event: ItemEvent::from_str(&r.event).map_err(|e| anyhow!(e))?,
                quantity: r.quantity,
                location: r.location,
                at: r.at.and_utc().timestamp_millis(),
            })
        })
        .collect()
}

async fn record_history(
    pool: &MySqlPool,
    item_id: u64,
    user_id: &str,
    location_id: Option<u64>,
    event: ItemEvent,
    quantity: Option<f64>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO item_history (item_id, user_id, location_id, event, quantity) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(item_id)
    .bind(user_id)
    .bind(location_id)
    .bind(event)
    .bind(quantity)
    .execute(pool)
    .await?;
    Ok(())
}
