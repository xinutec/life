//! To-do types. A to-do is a *typed* task with an open/done status and optional
//! notes. The type is a curated enum that starts at `purchase`/`call` and grows
//! as new kinds are actually needed — not up front. Typed, directional
//! connections to other to-dos and app entities live in the `todo_link` table.

use crate::str_enum;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    /// The kind of to-do. Add a variant here (plus its `Display`/`FromStr` arm) when
    /// a new kind earns its place — the set is deliberately small to start.
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
    /// Lifecycle status. Open or done for now; richer states (e.g. blocked) can be
    /// added when the connection semantics call for them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TodoStatus: "todo status" {
        Open => "open",
        Done => "done",
    }
}

str_enum! {
    /// Triage priority. Optional on a to-do (`None` = unprioritised, sorts last).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TodoPriority: "todo priority" {
        High => "high",
        Medium => "medium",
        Low => "low",
    }
}
/// A to-do as returned by the API.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
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
    /// Start-gate: don't surface / can't act before this day (drives "waiting";
    /// doubles as snooze). `None` = no gate.
    #[serde(rename = "notBefore")]
    pub not_before: Option<NaiveDate>,
    /// Deadline (drives urgency ordering). `None` = no deadline.
    pub due: Option<NaiveDate>,
    /// Belongs on the case-file site (mirrors a case-file checkbox), vs private
    /// and app-only. Default private; publishing is an explicit act.
    pub shared: bool,
}

/// Request body for creating a to-do. New to-dos start `open`.
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
    /// Private unless the caller opts in — the safe default for a case file.
    #[serde(default)]
    pub shared: bool,
}

/// Partial update, as `PATCH` implies: **an absent field is left alone.** For the
/// nullable columns, `null` is *not* the same as absent — it clears the value,
/// while absent preserves it. That distinction is what lets a caller send
/// `{"notes": "..."}` without having to restate the whole to-do (and without
/// silently resetting `shared` to private, which a `#[serde(default)]` bool would
/// do).
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

/// Deserialize a nullable field into `Option<Option<T>>` so the two cases stay
/// distinct: field absent → `None` (leave it), field present as `null` →
/// `Some(None)` (clear it). Plain `#[serde(default)]` collapses both to `None`.
fn absent_or_null<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::deserialize(de).map(Some)
}

str_enum! {
    /// How a to-do connects to its target. Directional: the edge runs *from* the
    /// to-do *to* the target.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum LinkKind: "link kind" {
        /// The to-do depends on the target (target should come first / blocks it).
        DependsOn => "depends_on",
        /// The target is a sub-task of the to-do (parent → child).
        Subtask => "subtask",
        /// A plain association, no ordering implied.
        Related => "related",
    }
}

str_enum! {
    /// What a connection points at. A target is referenced *softly* — by `ulid`
    /// (another to-do), DB id (an app entity), or room name (a house room) — never a
    /// hard FK, so links sync independently of their endpoints.
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
/// A typed, directional connection as returned by the API.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TodoLink {
    #[ts(type = "number")]
    pub id: u64,
    /// `ulid` of the source to-do.
    pub from: String,
    pub kind: LinkKind,
    #[serde(rename = "targetKind")]
    pub target_kind: TargetKind,
    /// The target's ulid / id-string / room name (per `target_kind`).
    #[serde(rename = "targetRef")]
    pub target_ref: String,
}

/// Request body for creating a connection.
#[derive(Debug, Deserialize)]
pub struct NewTodoLink {
    pub from: String,
    pub kind: LinkKind,
    #[serde(rename = "targetKind")]
    pub target_kind: TargetKind,
    #[serde(rename = "targetRef")]
    pub target_ref: String,
}
