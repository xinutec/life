//! Everything deleted, restorable; nothing is purged. A synced row's restore bumps
//! its `rev`: the one undelete, as no push can clear a tombstone.

pub mod repo;

use crate::str_enum;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

str_enum! {
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
/// `ref_` is an id for REST kinds, a ULID for synced ones (a never-synced row has
/// no id).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TrashEntry {
    pub kind: TrashKind,
    #[serde(rename = "ref")]
    #[ts(rename = "ref")]
    pub ref_: String,
    pub name: String,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub deleted_at: DateTime<Utc>,
}
