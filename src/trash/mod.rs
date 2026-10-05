//! The trash: everything the user deleted, restorable. Deletes only ever
//! tombstone (`deleted_at`); this lists tombstones across all kinds and clears
//! them on restore. Nothing is purged. A synced row's restore bumps `rev`, so it
//! reaches every device through the pull; this is the one deliberate undelete,
//! since a sync push can never clear a tombstone.

pub mod repo;

use crate::str_enum;
use chrono::{DateTime, Utc};
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
        Purchase => "purchase",
        File => "file",
    }
}
/// One deleted thing, as shown on the trash screen. `ref_` identifies the row
/// within its kind: the numeric id for REST entities (item/location/recipe),
/// the ULID for synced ones — ids can be absent client-side
/// for never-synced rows, ULIDs never are.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashEntry {
    pub kind: TrashKind,
    #[serde(rename = "ref")]
    #[ts(rename = "ref")]
    pub ref_: String,
    pub name: String,
    /// When it was deleted; Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub deleted_at: DateTime<Utc>,
}
