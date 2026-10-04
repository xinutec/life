//! Persistence for locations and items. `position` is stored as JSON text and
//! parsed here, so it survives however MariaDB reports the JSON column type.

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
    // A boolean SQL expression decodes as an integer.
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

/// The resolved item read: holding fields from `items`, display fields resolved
/// against the linked catalog product. A macro so it stays a literal for sqlx.
///
/// Only a `name_source = 'user'` name outranks the catalogue's: either fixed
/// precedence is wrong for someone (a marketing-sentence OFF name, a hand-typed
/// shorthand).
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

/// The catalog link for a new/updated item: an explicit `product_id` wins (it's
/// the only route to a barcodeless shop product), else fall back to matching the
/// barcode against the cached catalog.
async fn resolve_product_id(
    conn: impl sqlx::Executor<'_, Database = sqlx::MySql>,
    new: &NewItem,
) -> Result<Option<ProductId>> {
    if new.product_id.is_some() {
        return Ok(new.product_id);
    }
    product_id_for_barcode(conn, new.barcode.as_deref()).await
}

/// Resolve the catalog product id for a barcode, if one is cached.
///
/// An item's barcode is whatever was scanned or typed, so it is parsed rather
/// than queried directly: something that isn't a barcode matches no product, and
/// a blank one would otherwise match every barcodeless row in the catalog.
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
    // The row and its `added` history commit together, as every write here does.
    let mut tx = pool.begin().await?;
    let id = insert_item(&mut tx, user_id, &new).await?;
    tx.commit().await?;
    get_item(pool, user_id, id)
        .await?
        .ok_or_else(|| anyhow!("created item {id} not found"))
}

/// Insert an item and its `added` history row on the caller's connection, so a
/// caller with more to do (the Buy list's buy) commits it all as one.
pub(crate) async fn insert_item(
    conn: &mut sqlx::MySqlConnection,
    user_id: &str,
    new: &NewItem,
) -> Result<ItemId> {
    // Prefer an explicit catalog link (the only way to reach a barcodeless shop
    // product); otherwise link by barcode when it's already known (scanned/looked up).
    let product_id = resolve_product_id(&mut *conn, new).await?;
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

/// Move an item to a new location (or `None` to detach). Returns the updated
/// item, or `None` if no such item belongs to this user.
pub async fn move_item(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
    new_location_id: Option<LocationId>,
) -> Result<Option<Item>> {
    let mut tx = pool.begin().await?;
    // sqlx connects with CLIENT_FOUND_ROWS, so a move to where the thing already
    // is still counts its row: zero means no such live item.
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

/// Update every field of an item. Returns the updated item, or `None` if no
/// such item belongs to the user. Records a `moved` history row if the location
/// changed.
///
/// One transaction with the row locked: the update keeps fields the caller did
/// not state, so a value read outside the lock would be written back over a
/// concurrent change.
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
    // `None` for name_source or expiry_precision is "no statement", not a value:
    // every caller but the item form sends nothing, and must leave a chosen name
    // or a month-only expiry (migration 0045) as it is.
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

/// Record `low` against whatever stocked item a Buy row names, if any.
///
/// ⚠ Matched here, not client-side: the Buy screen never loads the inventory
/// catalogue, so a client match reads an empty store. Identity is the list's own
/// rule — catalog link, barcode, then case-insensitive name. `Ok(false)` = a
/// one-off purchase, which is ordinary.
pub async fn mark_low_matching(
    pool: &MySqlPool,
    user_id: &str,
    name: &str,
    barcode: Option<&str>,
    product_id: Option<ProductId>,
) -> Result<bool> {
    let barcode = barcode.map(barcode_hint);
    // Strongest key first, so a renamed row still resolves by barcode or link.
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

/// Record that a stock row was judged to be running low.
///
/// A decision, not a measurement: nothing moves, so no transaction. Repeats are
/// allowed — the rhythm is the gaps between them. `Ok(false)` = no such live
/// item for this user.
pub async fn mark_low(pool: &MySqlPool, user_id: &str, id: ItemId) -> Result<bool> {
    // The location rides along so the history reads the same as every other
    // event, and so "ran out of the one in the fridge" stays answerable. One
    // statement: the row it reads is the row it records.
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

/// Take an amount out of a stock row ("I used 200g of flour"), reading and
/// writing in one transaction with the row locked, so two phones can't both
/// take from 950. The rule is [`consume::take`]. `Ok(None)` = no live item; the
/// [`Taken`] outcome goes back to the route, which knows how to phrase it.
pub async fn use_item(
    pool: &MySqlPool,
    user_id: &str,
    id: ItemId,
    want: f64,
    want_unit: Option<&str>,
) -> Result<Option<(Taken, Option<Item>)>> {
    let mut tx = pool.begin().await?;
    // FOR UPDATE: the whole point of the transaction. Without it the subtraction
    // is a read-modify-write race and stock quietly drifts upward.
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

/// Restore a deleted item. Returns whether a tombstone was cleared.
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

/// Whether `id` names one of this user's live locations. `None`, "nowhere",
/// always does.
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

/// Every location id in the subtree rooted at `root` (inclusive), computed from
/// ALL of the user's rows (deleted or not — parent links stay intact under
/// tombstoning). Empty if `root` isn't the user's.
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

/// Delete a location and its whole subtree — tombstones, restorable as one unit
/// (every row gets the SAME `deleted_at` stamp; restore keys on it). Items keep
/// their `location_id`: with the location hidden they read as unplaced, and a
/// restore puts them right back where they were. Returns whether the root was
/// tombstoned.
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

/// Restore a deleted location together with the descendants that were deleted
/// in the same operation (same `deleted_at` stamp — descendants deleted
/// separately earlier stay in the trash as their own entries). Returns whether
/// anything was restored.
pub async fn restore_location(pool: &MySqlPool, user_id: &str, id: LocationId) -> Result<bool> {
    // The stamp, the tree and the restore in one transaction, the root locked, so
    // the stamp cannot change between being read and being matched.
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

/// Everything that has happened to one stock row, newest first.
///
/// Scoped on `h.user_id`, the history's own record of who did it, not through
/// the item, which could change hands. Empty is a real answer: a row older than
/// the audit has none, and so, as far as this caller may know, does another
/// user's id.
pub async fn item_history(
    pool: &MySqlPool,
    user_id: &str,
    item_id: ItemId,
) -> Result<Vec<ItemHistoryEntry>> {
    // A stored event outside the enum fails the query rather than being
    // dropped or shown blank, as `products::Source` is read.
    Ok(sqlx::query_as(
        "SELECT h.id, h.event, h.quantity, l.name AS location, \
         CAST(UNIX_TIMESTAMP(h.at) * 1000 AS SIGNED) AS at \
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
