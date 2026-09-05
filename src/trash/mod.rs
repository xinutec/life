//! The trash: everything the user deleted, restorable. Deletes anywhere in the
//! app only ever tombstone (`deleted_at`); this module lists those tombstones
//! across all entity kinds and clears them again on restore. Nothing is purged.
//!
//! For the synced entities (shopping/to-do) a restore bumps the global `rev`,
//! so the resurrected row propagates to every device through the normal pull.
//! The sync push path itself can never clear a tombstone (set-only, see
//! `sync::repo`) — this explicit restore is the one deliberate undelete.

pub mod repo;

use crate::str_enum;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
    /// Which table a trash entry lives in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum TrashKind: "trash kind" {
        Item => "item",
        Location => "location",
        Recipe => "recipe",
        Shopping => "shopping",
        Todo => "todo",
        Wellbeing => "wellbeing",
    }
}
/// One deleted thing, as shown on the trash screen. `ref_` identifies the row
/// within its kind: the numeric id for REST entities (item/location/recipe),
/// the ULID for synced ones (shopping/todo) — ids can be absent client-side
/// for never-synced rows, ULIDs never are.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashEntry {
    pub kind: TrashKind,
    #[serde(rename = "ref")]
    #[ts(rename = "ref")]
    pub ref_: String,
    pub name: String,
    /// When it was deleted, Unix milliseconds (UTC).
    #[ts(type = "number")]
    pub deleted_at: i64,
}
