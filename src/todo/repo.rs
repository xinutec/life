//! Persistence for the to-do list.
//!
//! Sync-aware exactly like `shopping::repo`: every write allocates a global `rev`
//! in its transaction, stamps `updated_at`, and *soft*-deletes (sets `deleted_at`)
//! so deletes propagate to offline clients as tombstones. Reads hide tombstones.
//! The enums are stored as their snake_case strings and parsed at this boundary.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::MySqlPool;
use ulid::Ulid;

use super::types::{NewTodo, Todo, TodoPriority, TodoStatus, TodoType, UpdateTodo};
use crate::sync::repo::next_rev;

#[derive(sqlx::FromRow)]
struct Row {
    id: u64,
    title: String,
    todo_type: String,
    status: String,
    priority: Option<String>,
    notes: Option<String>,
    not_before: Option<NaiveDate>,
    due: Option<NaiveDate>,
    shared: bool,
}

impl TryFrom<Row> for Todo {
    type Error = anyhow::Error;
    fn try_from(r: Row) -> Result<Self> {
        Ok(Todo {
            id: r.id,
            title: r.title,
            todo_type: r
                .todo_type
                .parse::<TodoType>()
                .map_err(anyhow::Error::msg)?,
            status: r.status.parse::<TodoStatus>().map_err(anyhow::Error::msg)?,
            priority: r
                .priority
                .map(|p| p.parse::<TodoPriority>())
                .transpose()
                .map_err(anyhow::Error::msg)?,
            notes: r.notes,
            not_before: r.not_before,
            due: r.due,
            shared: r.shared,
        })
    }
}

/// To-dos: open first, then by title. Tombstoned rows are hidden.
pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<Todo>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY status DESC, title",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(Todo::try_from).collect()
}

pub async fn get(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Todo>> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    row.map(Todo::try_from).transpose()
}

/// Same read as [`get`], but inside a caller's transaction and holding the row
/// (`FOR UPDATE`). A merging PATCH writes back fields the caller never sent, so it
/// must read under the same lock it writes under — a plain read on the pool is a
/// snapshot from *outside* the transaction, and a write landing in between would be
/// silently restored to its old value.
async fn get_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    id: u64,
) -> Result<Option<Todo>> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(Todo::try_from).transpose()
}

pub async fn create(pool: &MySqlPool, user_id: &str, new: NewTodo) -> Result<Todo> {
    let ulid = Ulid::new().to_string();
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "INSERT INTO todos (user_id, ulid, title, todo_type, status, priority, notes, \
         not_before, due, shared, rev, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, NOW(), NOW())",
    )
    .bind(user_id)
    .bind(&ulid)
    .bind(&new.title)
    .bind(new.todo_type.to_string())
    .bind(new.priority.map(|p| p.to_string()))
    .bind(&new.notes)
    .bind(new.not_before)
    .bind(new.due)
    .bind(new.shared)
    .bind(rev)
    .execute(&mut *tx)
    .await?;
    let id = res.last_insert_id();
    tx.commit().await?;
    Ok(Todo {
        id,
        title: new.title,
        todo_type: new.todo_type,
        status: TodoStatus::Open,
        priority: new.priority,
        notes: new.notes,
        not_before: new.not_before,
        due: new.due,
        shared: new.shared,
    })
}

pub async fn update(
    pool: &MySqlPool,
    user_id: &str,
    id: u64,
    upd: UpdateTodo,
) -> Result<Option<Todo>> {
    // PATCH is partial: merge onto the stored row, so an absent field keeps its
    // current value rather than being overwritten with a type default. The read has
    // to happen inside this transaction, with the row locked — see `get_for_update`.
    let mut tx = pool.begin().await?;
    let Some(cur) = get_for_update(&mut tx, user_id, id).await? else {
        return Ok(None);
    };
    let title = upd.title.unwrap_or(cur.title);
    let todo_type = upd.todo_type.unwrap_or(cur.todo_type);
    let status = upd.status.unwrap_or(cur.status);
    let priority = upd.priority.unwrap_or(cur.priority);
    let notes = upd.notes.unwrap_or(cur.notes);
    let not_before = upd.not_before.unwrap_or(cur.not_before);
    let due = upd.due.unwrap_or(cur.due);
    let shared = upd.shared.unwrap_or(cur.shared);

    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE todos SET title = ?, todo_type = ?, status = ?, priority = ?, notes = ?, \
         not_before = ?, due = ?, shared = ?, rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(&title)
    .bind(todo_type.to_string())
    .bind(status.to_string())
    .bind(priority.map(|p| p.to_string()))
    .bind(&notes)
    .bind(not_before)
    .bind(due)
    .bind(shared)
    .bind(rev)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    get(pool, user_id, id).await.context("reload after update")
}

/// Soft delete: set the tombstone + a fresh `rev` so the delete syncs.
pub async fn delete(pool: &MySqlPool, user_id: &str, id: u64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE todos SET deleted_at = NOW(), rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(rev)
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

/// Restore a tombstoned to-do (trash/undo). The ONE deliberate undelete path —
/// sync pushes can never clear a tombstone. The fresh `rev` propagates the
/// resurrected row to every device through the normal pull. (Links that were
/// removed alongside the to-do stay removed; reconnect by hand if needed.)
pub async fn restore(pool: &MySqlPool, user_id: &str, ulid: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
    let res = sqlx::query(
        "UPDATE todos SET deleted_at = NULL, rev = ?, updated_at = NOW() \
         WHERE ulid = ? AND user_id = ? AND deleted_at IS NOT NULL",
    )
    .bind(rev)
    .bind(ulid)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}
