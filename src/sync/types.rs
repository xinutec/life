//! The RxDB-compatible sync wire types: documents keyed by `ulid`, carrying the
//! server `rev` and RxDB's `_deleted`; the checkpoint is the highest `rev` pulled.

use crate::inventory::types::ItemCategory;
use crate::products::ids::ProductId;
use crate::todo::types::{LinkKind, TargetKind, TodoPriority, TodoStatus, TodoType};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShoppingDoc {
    pub ulid: String,
    /// Carried on pull for `/api/shopping/{id}/buy`; ignored on push.
    #[serde(default)]
    pub id: Option<u64>,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub barcode: Option<String>,
    /// Defaults to `food` for pre-0024 clients.
    #[serde(default = "default_shopping_category")]
    pub category: ItemCategory,
    #[serde(default)]
    pub product_id: Option<ProductId>,
    pub done: bool,
    /// `deleted_at IS NOT NULL`.
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    /// Set by the server; ignored on push.
    #[serde(default)]
    pub rev: u64,
}

fn default_shopping_category() -> ItemCategory {
    ItemCategory::Food
}

/// An unknown enum string is refused at decode, never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoDoc {
    pub ulid: String,
    #[serde(default)]
    pub id: Option<u64>,
    pub title: String,
    #[serde(rename = "type")]
    pub todo_type: TodoType,
    pub status: TodoStatus,
    #[serde(default)]
    pub priority: Option<TodoPriority>,
    pub notes: Option<String>,
    #[serde(rename = "notBefore", default)]
    pub not_before: Option<NaiveDate>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    /// On the case-file site; private by default.
    #[serde(default)]
    pub shared: bool,
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    #[serde(default)]
    pub rev: u64,
}

/// RFC 3339 `recordedAt`. Readings are tenths (35 is 3.5), exact under averaging,
/// and named for their scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellbeingDoc {
    pub ulid: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(rename = "recordedAt")]
    pub recorded_at: DateTime<Utc>,
    #[serde(rename = "scoreTenths")]
    pub score_tenths: u8,
    /// `None` for a mood-only check-in.
    #[serde(default, rename = "energyTenths")]
    pub energy_tenths: Option<u8>,
    #[serde(default)]
    pub emotions: Vec<String>,
    pub note: Option<String>,
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    #[serde(default)]
    pub rev: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoLinkDoc {
    pub ulid: String,
    #[serde(default)]
    pub id: Option<u64>,
    pub from: String,
    pub kind: LinkKind,
    #[serde(rename = "targetKind")]
    pub target_kind: TargetKind,
    #[serde(rename = "targetRef")]
    pub target_ref: String,
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    #[serde(default)]
    pub rev: u64,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PullResponse<D> {
    pub documents: Vec<D>,
    pub checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Checkpoint {
    #[ts(type = "number")]
    pub rev: u64,
}

/// A change and the master state the client assumed (null for an insert). The
/// explicit `DeserializeOwned` bound keeps it usable as an axum `Json` body.
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "D: serde::de::DeserializeOwned"))]
pub struct PushEntry<D> {
    #[serde(rename = "newDocumentState")]
    pub new_document_state: D,
    #[serde(rename = "assumedMasterState", default)]
    pub assumed_master_state: Option<D>,
}
