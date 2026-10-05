//! The trash: listing unions every kind's tombstones; restores go to each kind's
//! own repo, which keeps its semantics.

use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::MySqlPool;

use super::{TrashEntry, TrashKind};
use crate::files::repo as files_repo;
use crate::inventory::repo as inventory_repo;
use crate::purchases::repo as purchases_repo;
use crate::recipes::repo as recipes_repo;
use crate::shopping::repo as shopping_repo;
use crate::todo::repo as todo_repo;
use crate::wellbeing::repo as wellbeing_repo;

#[derive(sqlx::FromRow)]
struct Row {
    ref_: String,
    name: String,
    deleted_at: NaiveDateTime,
}

impl Row {
    fn into_entry(self, kind: TrashKind) -> TrashEntry {
        TrashEntry {
            kind,
            ref_: self.ref_,
            name: self.name,
            deleted_at: self.deleted_at.and_utc(),
        }
    }
}

/// Newest deletion first.
pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<TrashEntry>> {
    let queries: [(TrashKind, &str); 8] = [
        (
            TrashKind::Item,
            // Named as the cupboard names it (`item_select!`), so you find it by
            // the name you last saw.
            "SELECT CAST(i.id AS CHAR) AS ref_, \
             CASE WHEN i.name_source = 'user' THEN COALESCE(i.name, p.name, '') \
                  ELSE COALESCE(p.name, i.name, '') END AS name, \
             i.deleted_at AS deleted_at FROM items i \
             LEFT JOIN products p ON p.id = i.product_id \
             WHERE i.user_id = ? AND i.deleted_at IS NOT NULL",
        ),
        (
            TrashKind::Location,
            "SELECT CAST(id AS CHAR) AS ref_, name, deleted_at FROM locations \
             WHERE user_id = ? AND deleted_at IS NOT NULL",
        ),
        (
            TrashKind::Recipe,
            "SELECT CAST(id AS CHAR) AS ref_, name, deleted_at FROM recipes \
             WHERE user_id = ? AND deleted_at IS NOT NULL",
        ),
        (
            TrashKind::Shopping,
            // Rows without a ulid cannot be restored by ref.
            "SELECT ulid AS ref_, name, deleted_at FROM shopping_items \
             WHERE user_id = ? AND deleted_at IS NOT NULL AND ulid IS NOT NULL",
        ),
        (
            TrashKind::Todo,
            "SELECT ulid AS ref_, title AS name, deleted_at FROM todos \
             WHERE user_id = ? AND deleted_at IS NOT NULL AND ulid IS NOT NULL",
        ),
        (
            TrashKind::Wellbeing,
            // Tenths undone; TRIM leaves "4" a 4 and a half-step "3.5".
            "SELECT ulid AS ref_, CONCAT('Check-in (', \
                 TRIM(TRAILING '.0' FROM FORMAT(score_tenths / 10, 1)), '/5)') AS name, \
             deleted_at \
             FROM wellbeing WHERE user_id = ? AND deleted_at IS NOT NULL AND ulid IS NOT NULL",
        ),
        (
            TrashKind::Purchase,
            "SELECT CAST(id AS CHAR) AS ref_, CONCAT(name, ' at ', shop) AS name, deleted_at \
             FROM purchases WHERE user_id = ? AND deleted_at IS NOT NULL",
        ),
        (
            TrashKind::File,
            "SELECT CAST(id AS CHAR) AS ref_, name, deleted_at \
             FROM item_files WHERE user_id = ? AND deleted_at IS NOT NULL",
        ),
    ];

    let mut entries = Vec::new();
    for (kind, sql) in queries {
        let rows: Vec<Row> = sqlx::query_as(sql).bind(user_id).fetch_all(pool).await?;
        entries.extend(rows.into_iter().map(|r| r.into_entry(kind)));
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.deleted_at));
    Ok(entries)
}

/// False for an unknown ref, a row not deleted, or a malformed id.
pub async fn restore(pool: &MySqlPool, user_id: &str, kind: TrashKind, r: &str) -> Result<bool> {
    match kind {
        TrashKind::Item => match r.parse::<u64>() {
            Ok(id) => inventory_repo::restore_item(pool, user_id, id.into()).await,
            Err(_) => Ok(false),
        },
        TrashKind::Location => match r.parse::<u64>() {
            Ok(id) => inventory_repo::restore_location(pool, user_id, id.into()).await,
            Err(_) => Ok(false),
        },
        TrashKind::Recipe => match r.parse::<u64>() {
            Ok(id) => recipes_repo::restore_recipe(pool, user_id, id).await,
            Err(_) => Ok(false),
        },
        TrashKind::Shopping => shopping_repo::restore(pool, user_id, r).await,
        TrashKind::Todo => todo_repo::restore(pool, user_id, r).await,
        TrashKind::Wellbeing => wellbeing_repo::restore(pool, user_id, r).await,
        TrashKind::Purchase => match r.parse::<u64>() {
            Ok(id) => purchases_repo::restore(pool, user_id, id.into()).await,
            Err(_) => Ok(false),
        },
        TrashKind::File => match r.parse::<u64>() {
            Ok(id) => files_repo::restore(pool, user_id, id.into()).await,
            Err(_) => Ok(false),
        },
    }
}
