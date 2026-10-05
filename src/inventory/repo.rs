//! Persistence for locations and items. `position` is JSON text, parsed here.

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use sqlx::MySqlPool;

use super::consume::{self, Held, Taken};
use super::types::{
    ExpiryPrecision, Item, ItemCategory, ItemEvent, ItemHistoryEntry, ItemId, ItemNameSource,
    Location, LocationId, LocationKind, NewItem, NewLocation,
};
use crate::products::ids::{Barcode, ProductId, barcode_hint};

#[derive(sqlx::FromRow)]
struct LocationRow {
    id: LocationId,
    kind: LocationKind,
    name: String,
    parent_id: Option<LocationId>,
    sort_order: i32,
    position: Option<String>,
}

impl LocationRow {
    fn into_location(self) -> Result<Location> {
        let position = match self.position {
            Some(s) => Some(serde_json::from_str(&s).context("parsing location.position")?),
            None => None,
        };
        Ok(Location {
            id: self.id,
            kind: self.kind,
            name: self.name,
            parent_id: self.parent_id,
            sort_order: self.sort_order,
            position,
        })
    }
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: ItemId,
    product_id: Option<ProductId>,
    name: String,
    brand: Option<String>,
    category: ItemCategory,
    quantity: Option<f64>,
    unit: Option<String>,
    expiry: Option<NaiveDate>,
    expiry_precision: ExpiryPrecision,
    location_id: Option<LocationId>,
    barcode: Option<String>,
    has_image: i64,
}

impl ItemRow {
    fn into_item(self) -> Item {
        Item {
            id: self.id,
            product_id: self.product_id,
            name: self.name,
            brand: self.brand,
            category: self.category,
            quantity: self.quantity,
            unit: self.unit,
            expiry: self.expiry,
            expiry_precision: self.expiry_precision,
            location_id: self.location_id,
            barcode: self.barcode,
            has_image: self.has_image != 0,
        }
    }
}

/// The item read, display fields resolved through the linked product; a macro to
/// stay a literal for sqlx. Only a `name_source = 'user'` name outranks the
/// catalogue's.
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

/// An explicit `product_id` wins (the only way to a barcodeless product), else the
/// barcode's cached product.
async fn resolve_product_id(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    new: &NewItem,
) -> Result<Option<ProductId>> {
    if new.product_id.is_some() {
        return Ok(new.product_id);
    }
    product_id_for_barcode(conn, new.barcode.as_deref()).await
}

/// Parsed, not queried as typed: a blank barcode would match every barcodeless
/// product.
async fn product_id_for_barcode(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    barcode: Option<&str>,
) -> Result<Option<ProductId>> {
    let Some(bc) = barcode.and_then(|b| b.parse::<Barcode>().ok()) else {
        return Ok(None);
    };
    let row: Option<(ProductId,)> = sqlx::query_as("SELECT id FROM products WHERE barcode = ?")
        .bind(bc)
        .fetch_optional(conn)
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
    .bind(new.kind)
    .bind(&new.name)
    .bind(new.parent_id)
    .bind(new.sort_order)
    .bind(&position_str)
    .execute(pool)
    .await?;
    Ok(Location {
        id: res.last_insert_id().into(),
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
    Ok(rows.into_iter().map(ItemRow::into_item).collect())
}

pub async fn get_item(pool: &MySqlPool, user_id: &str, id: ItemId) -> Result<Option<Item>> {
    let row: Option<ItemRow> = sqlx::query_as(concat!(
        item_select!(),
        " WHERE i.id = ? AND i.user_id = ? AND i.deleted_at IS NULL"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(ItemRow::into_item))
}

pub async fn create_item(pool: &MySqlPool, user_id: &str, new: NewItem) -> Result<Item> {
    let mut tx = pool.begin().await?;
    let id = insert_item(&mut tx, user_id, &new).await?;
    tx.commit().await?;
    get_item(pool, user_id, id)
        .await?
        .ok_or_else(|| anyhow!("created item {id} not found"))
}

/// On the caller's connection, so the buy commits it all as one.
pub(crate) async fn insert_item(
    conn: &mut sqlx::MySqlConnection,
    user_id: &str,
    new: &NewItem,
) -> Result<ItemId> {
    let product_id = resolve_product_id(&mut *conn, new).await?;
    // A name typed while adding is a scribble the scan fills in: the catalogue
    // wins unless the client says the name is the person's.
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
    .bind(new.category)
    .bind(new.quantity)
    .bind(&new.unit)
    .bind(new.expiry)
    .bind(
        new.expiry_precision
            .unwrap_or(ExpiryPrecision::Day)
            .to_string(),
    )
    .bind(new.location_id)
    .bind(new.barcode.as_deref().map(barcode_hint))
    .execute(&mut *conn)
    .await?;
    let id = ItemId::from(res.last_insert_id());
    record_history(
        &mut *conn,
        id,
        user_id,
        new.location_id,
        ItemEvent::Added,
        new.quantity,
    )
    .await?;
    Ok(id)
}

/// `None` detaches; returns `None` if no such item is this user's.
pub async fn move_item(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
    new_location_id: Option<LocationId>,
) -> Result<Option<Item>> {
    let mut tx = pool.begin().await?;
    // CLIENT_FOUND_ROWS: a move to the same place still counts its row.
    let moved = sqlx::query(
        "UPDATE items SET location_id = ? WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(new_location_id)
    .bind(item_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    if moved.rows_affected() == 0 {
        return Ok(None);
    }
    record_history(
        &mut *tx,
        item_id,
        user_id,
        new_location_id,
        ItemEvent::Moved,
        None,
    )
    .await?;
    tx.commit().await?;
    get_item(pool, user_id, item_id).await
}

/// Update an item, recording `moved` if the location changed. The row is locked,
/// since fields the caller did not state are kept and must not be read stale.
pub async fn update_item(
    pool: &MySqlPool,
    user_id: &str,
    id: ItemId,
    new: NewItem,
) -> Result<Option<Item>> {
    let product_id = resolve_product_id(pool, &new).await?;
    let mut tx = pool.begin().await?;
    let held: Option<(Option<LocationId>,)> = sqlx::query_as(
        "SELECT location_id FROM items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((was_at,)) = held else {
        return Ok(None);
    };
    // `None` here is "no statement": a chosen name or a month-only expiry stays.
    sqlx::query(
        "UPDATE items SET product_id = ?, name = ?, name_source = COALESCE(?, name_source), \
         category = ?, quantity = ?, unit = ?, expiry = ?, \
         expiry_precision = COALESCE(?, expiry_precision), location_id = ?, barcode = ? \
         WHERE id = ? AND user_id = ?",
    )
    .bind(product_id)
    .bind(&new.name)
    .bind(new.name_source)
    .bind(new.category)
    .bind(new.quantity)
    .bind(&new.unit)
    .bind(new.expiry)
    .bind(new.expiry_precision)
    .bind(new.location_id)
    .bind(new.barcode.as_deref().map(barcode_hint))
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    if was_at != new.location_id {
        record_history(
            &mut *tx,
            id,
            user_id,
            new.location_id,
            ItemEvent::Moved,
            new.quantity,
        )
        .await?;
    }
    tx.commit().await?;
    get_item(pool, user_id, id).await
}

/// Record `low` against whatever stocked item a Buy row names. Matched here, as
/// the Buy screen never loads the inventory. `Ok(false)`: a one-off purchase.
pub async fn mark_low_matching(
    pool: &MySqlPool,
    user_id: &str,
    name: &str,
    barcode: Option<&str>,
    product_id: Option<ProductId>,
) -> Result<bool> {
    let barcode = barcode.map(barcode_hint);
    // Strongest key first, so a renamed row still resolves.
    let row: Option<(ItemId,)> = sqlx::query_as(
        "SELECT id FROM items \
         WHERE user_id = ? AND deleted_at IS NULL \
           AND (  (? IS NOT NULL AND product_id = ?) \
               OR (? IS NOT NULL AND barcode = ?) \
               OR LOWER(name) = LOWER(?)) \
         ORDER BY (product_id <=> ?) DESC, (barcode <=> ?) DESC \
         LIMIT 1",
    )
    .bind(user_id)
    .bind(product_id)
    .bind(product_id)
    .bind(&barcode)
    .bind(&barcode)
    .bind(name.trim())
    .bind(product_id)
    .bind(&barcode)
    .fetch_optional(pool)
    .await?;
    let Some((id,)) = row else {
        return Ok(false);
    };
    mark_low(pool, user_id, id).await
}

/// A judgement, not a measurement: nothing moves. Repeats are the signal.
pub async fn mark_low(pool: &MySqlPool, user_id: &str, id: ItemId) -> Result<bool> {
    // With the location, so "the one in the fridge" stays answerable.
    let res = sqlx::query(
        "INSERT INTO item_history (item_id, user_id, location_id, event, quantity) \
         SELECT id, user_id, location_id, ?, NULL FROM items \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(ItemEvent::Low)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// "I used 200 g": the row locked, so two phones cannot both take from 950. The
/// rule is [`consume::take`]; `Ok(None)` is no live item.
pub async fn use_item(
    pool: &MySqlPool,
    user_id: &str,
    id: ItemId,
    want: f64,
    want_unit: Option<&str>,
) -> Result<Option<(Taken, Option<Item>)>> {
    let mut tx = pool.begin().await?;
    // Without the lock, stock drifts upward in a read-modify-write race.
    let row: Option<(Option<f64>, Option<String>, Option<LocationId>)> = sqlx::query_as(
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

    let held = Held {
        quantity,
        unit: unit.as_deref(),
    };
    let outcome = consume::take(held, want, want_unit);
    let left = match outcome {
        Taken::Left(n) => n,
        Taken::Emptied { .. } => 0.0,
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
    // The delta, not the new level: it is what a consumption rate needs, and it
    // survives a hand edit of the quantity.
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

/// A tombstone, restorable; history is kept.
pub async fn delete_item(pool: &MySqlPool, user_id: &str, id: ItemId) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "UPDATE items SET deleted_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let deleted = res.rows_affected() > 0;
    if deleted {
        record_history(&mut *tx, id, user_id, None, ItemEvent::Removed, None).await?;
    }
    tx.commit().await?;
    Ok(deleted)
}

pub async fn restore_item(pool: &MySqlPool, user_id: &str, id: ItemId) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "UPDATE items SET deleted_at = NULL \
         WHERE id = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    let restored = res.rows_affected() > 0;
    if restored {
        record_history(&mut *tx, id, user_id, None, ItemEvent::Restored, None).await?;
    }
    tx.commit().await?;
    Ok(restored)
}

/// `None`, nowhere, always is.
pub async fn is_own_location(
    pool: &MySqlPool,
    user_id: &str,
    id: Option<LocationId>,
) -> Result<bool> {
    let Some(id) = id else {
        return Ok(true);
    };
    let row: Option<(LocationId,)> = sqlx::query_as(
        "SELECT id FROM locations WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

/// The subtree from all the user's rows, deleted or not: tombstones keep their
/// parent links. Empty if `root` is not theirs.
async fn subtree_ids(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    user_id: &str,
    root: LocationId,
) -> Result<Vec<LocationId>> {
    let rows: Vec<(LocationId, Option<LocationId>)> =
        sqlx::query_as("SELECT id, parent_id FROM locations WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(conn)
            .await?;
    Ok(super::tree::subtree(&rows, root))
}

/// Tombstone a location and its subtree with one shared stamp, which restore
/// keys on. Items keep their `location_id` and come back with it.
pub async fn delete_location(pool: &MySqlPool, user_id: &str, id: LocationId) -> Result<bool> {
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

/// Restores the subtree deleted in the same stamp; ones deleted earlier stay in
/// the trash as their own entries.
pub async fn restore_location(pool: &MySqlPool, user_id: &str, id: LocationId) -> Result<bool> {
    // One transaction, the root locked, so the stamp cannot change under us.
    let mut tx = pool.begin().await?;
    let stamp: Option<(Option<chrono::NaiveDateTime>,)> =
        sqlx::query_as("SELECT deleted_at FROM locations WHERE id = ? AND user_id = ? FOR UPDATE")
            .bind(id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((Some(stamp),)) = stamp else {
        return Ok(false); // unknown, someone else's, or not deleted
    };
    let ids = subtree_ids(&mut *tx, user_id, id).await?;
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
    let res = qb.build().execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Newest first, scoped on the history's own `user_id`. Empty for a row older
/// than the audit, and for somebody else's id.
pub async fn item_history(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
) -> Result<Vec<ItemHistoryEntry>> {
    // An unknown event fails the query rather than showing blank.
    Ok(sqlx::query_as(
        "SELECT h.id, h.event, h.quantity, l.name AS location, \
         h.at \
         FROM item_history h \
         LEFT JOIN locations l ON l.id = h.location_id AND l.user_id = h.user_id \
         WHERE h.item_id = ? AND h.user_id = ? \
         ORDER BY h.at DESC, h.id DESC",
    )
    .bind(item_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

async fn record_history(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    item_id: ItemId,
    user_id: &str,
    location_id: Option<LocationId>,
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
    .execute(conn)
    .await?;
    Ok(())
}
