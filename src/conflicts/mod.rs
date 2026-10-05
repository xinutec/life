//! The sync-conflict log: the values a same-field merge discarded, for review.
//! Resolved by stamping, never deleted.

pub mod repo;

use crate::str_enum;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ConflictKind: "conflict kind" {
        Shopping => "shopping",
        Todo => "todo",
        Wellbeing => "wellbeing",
    }
}
/// `mine` and `theirs` are JSON-encoded by the client, so they round-trip exactly.
#[derive(Debug, Clone, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct ConflictEntry {
    #[ts(type = "number")]
    pub id: u64,
    pub kind: ConflictKind,
    pub ulid: String,
    pub field: String,
    pub label: String,
    pub mine: String,
    pub theirs: String,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NewConflict {
    pub kind: ConflictKind,
    pub ulid: String,
    pub field: String,
    pub label: String,
    pub mine: String,
    pub theirs: String,
}
