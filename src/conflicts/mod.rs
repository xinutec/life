//! The sync-conflict log. When two devices edit the SAME field of the same row
//! while one is offline, the client's field-level merge keeps the pushing
//! device's value and reports the losing one here, so nothing is silently
//! discarded — the Conflicts screen offers keep-mine / use-other. Entries are
//! resolved (stamped), never deleted.

pub mod repo;

use crate::str_enum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    /// Which synced collection the conflicted row belongs to.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ConflictKind: "conflict kind" {
        Shopping => "shopping",
        Todo => "todo",
        Wellbeing => "wellbeing",
    }
}
/// One unresolved same-field conflict, as listed on the Conflicts screen.
/// `mine`/`theirs` are JSON-encoded field values (the client encodes them, so
/// numbers/nulls round-trip exactly).
#[derive(Debug, Clone, Serialize, TS)]
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
    /// When the conflict happened, Unix milliseconds (UTC).
    #[ts(type = "number")]
    pub created_at: i64,
}

/// Client report of one same-field conflict (POST /api/conflicts body).
#[derive(Debug, Deserialize)]
pub struct NewConflict {
    pub kind: ConflictKind,
    pub ulid: String,
    pub field: String,
    pub label: String,
    pub mine: String,
    pub theirs: String,
}
