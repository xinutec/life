//! To-dos: typed tasks with a status; their connections are in `todo_link`.

use crate::str_enum;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    /// Add a variant when a new kind earns its place.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TodoType: "todo type" {
        Purchase => "purchase",
        Call => "call",
        Appointment => "appointment",
        Admin => "admin",
        Task => "task",
    }
}

str_enum! {
    /// "Blocked" and "waiting" are derived, not stored.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TodoStatus: "todo status" {
        Open => "open",
        Done => "done",
    }
}

str_enum! {
    /// Optional; unprioritised sorts last.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TodoPriority: "todo priority" {
        High => "high",
        Medium => "medium",
        Low => "low",
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct Todo {
    #[ts(type = "number")]
    pub id: u64,
    pub title: String,
    #[serde(rename = "type")]
    pub todo_type: TodoType,
    pub status: TodoStatus,
    pub priority: Option<TodoPriority>,
    pub notes: Option<String>,
    /// Not before this day ("waiting"; doubles as snooze).
    #[serde(rename = "notBefore")]
    pub not_before: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
    /// On the case-file site; private unless chosen.
    pub shared: bool,
}

/// New to-dos start `open`.
#[derive(Debug, Deserialize)]
pub struct NewTodo {
    pub title: String,
    #[serde(rename = "type")]
    pub todo_type: TodoType,
    #[serde(default)]
    pub priority: Option<TodoPriority>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(rename = "notBefore", default)]
    pub not_before: Option<NaiveDate>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    #[serde(default)]
    pub shared: bool,
}

/// A PATCH: an absent field is left alone, `null` clears a nullable one.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateTodo {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(rename = "type", default)]
    pub todo_type: Option<TodoType>,
    #[serde(default)]
    pub status: Option<TodoStatus>,
    #[serde(default, deserialize_with = "absent_or_null")]
    pub priority: Option<Option<TodoPriority>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    pub notes: Option<Option<String>>,
    #[serde(rename = "notBefore", default, deserialize_with = "absent_or_null")]
    pub not_before: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "absent_or_null")]
    pub due: Option<Option<NaiveDate>>,
    #[serde(default)]
    pub shared: Option<bool>,
}

/// Absent → `None` (leave it), `null` → `Some(None)` (clear it);
/// `#[serde(default)]` alone would collapse the two.
fn absent_or_null<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::deserialize(de).map(Some)
}

str_enum! {
    /// Directional: from the to-do to the target.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum LinkKind: "link kind" {
    /// The target comes first.
        DependsOn => "depends_on",
    /// The target is a sub-task.
        Subtask => "subtask",
        Related => "related",
    }
}

str_enum! {
    /// Referenced softly (a ulid, an id, a room name), never by FK, so links sync
    /// apart from their endpoints.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TargetKind: "target kind" {
        Todo => "todo",
        Item => "item",
        Recipe => "recipe",
        Room => "room",
        Shopping => "shopping",
        Place => "place",
    }
}
