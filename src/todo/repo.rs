//! The to-do list, sync-aware like `shopping::repo`: every write takes a `rev`,
//! and deletes are tombstones.

use anyhow::{Context, Result};
use sqlx::MySqlPool;
use ulid::Ulid;

use super::types::{NewTodo, Todo, TodoStatus, UpdateTodo};
use crate::sync::repo::{next_rev, stamp};

/// Open first, then by title.
pub async fn list(pool: &MySqlPool, user_id: &str) -> Result<Vec<Todo>> {
    let rows: Vec<Todo> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE user_id = ? AND deleted_at IS NULL ORDER BY status DESC, title",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &MySqlPool, user_id: &str, id: u64) -> Result<Option<Todo>> {
    let row: Option<Todo> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// [`get`] under the caller's lock: a merging PATCH writes back fields it never
/// read, so a read outside the lock would restore a concurrent write.
async fn get_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    id: u64,
) -> Result<Option<Todo>> {
    let row: Option<Todo> = sqlx::query_as(
        "SELECT id, title, todo_type, status, priority, notes, not_before, due, shared FROM todos \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row)
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
    .bind(new.todo_type)
    .bind(new.priority)
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
    // Merged onto the stored row, read under the lock (`get_for_update`).
    let mut tx = pool.begin().await?;
    let rev = next_rev(&mut tx).await?;
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

    let res = sqlx::query(
        "UPDATE todos SET title = ?, todo_type = ?, status = ?, priority = ?, notes = ?, \
         not_before = ?, due = ?, shared = ?, rev = ?, updated_at = NOW() \
         WHERE id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(&title)
    .bind(todo_type)
    .bind(status)
    .bind(priority)
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

/// The one undelete path; a fresh `rev` carries it to every device. Links removed
/// with it stay removed.
pub async fn restore(pool: &MySqlPool, user_id: &str, ulid: &str) -> Result<bool> {
    stamp(pool, |rev| {
        sqlx::query(
            "UPDATE todos SET deleted_at = NULL, rev = ?, updated_at = NOW() \
             WHERE ulid = ? AND user_id = ? AND deleted_at IS NOT NULL",
        )
        .bind(rev)
        .bind(ulid)
        .bind(user_id)
    })
    .await
}
