//! Wire types for the offline-first sync protocol (RxDB-compatible).
//!
//! A *document* is the canonical synced shape of a row, keyed by its `ulid` and
//! carrying the server `rev` (version) plus RxDB's `_deleted` tombstone flag. The
//! checkpoint is simply the highest `rev` the client has pulled. The page/entry
//! envelopes are generic over the document type so each collection reuses them.
//! See `docs/design/sync.md`.

use crate::inventory::types::ItemCategory;
use crate::products::ids::ProductId;
use crate::todo::types::{LinkKind, TargetKind, TodoPriority, TodoStatus, TodoType};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One shopping row as it travels over sync. `rev` is the server revision; a pull
/// returns rows ordered by it and the client checkpoints on the maximum seen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShoppingDoc {
    pub ulid: String,
    /// Server autoincrement id — carried on *pull* so the client can still call the
    /// `/api/shopping/{id}/buy` (convert→inventory) for already-synced rows.
    /// Ignored on push (offline-created rows have none until they sync).
    #[serde(default)]
    pub id: Option<u64>,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub barcode: Option<String>,
    /// Inventory category the buy→inventory conversion will use. An unknown one
    /// is refused when the push is decoded. Defaults to `food` so docs from
    /// pre-0024 clients stay pushable.
    #[serde(default = "default_shopping_category")]
    pub category: ItemCategory,
    /// Optional link to the products catalog (mirrors `items.product_id`).
    #[serde(default)]
    pub product_id: Option<ProductId>,
    pub done: bool,
    /// RxDB tombstone flag (maps to `deleted_at IS NOT NULL`).
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    /// Server revision (version). Ignored as push *input*; set by the server.
    #[serde(default)]
    pub rev: u64,
}

fn default_shopping_category() -> ItemCategory {
    ItemCategory::Food
}

/// One to-do row as it travels over sync. The enums travel as their snake_case
/// strings, the same text the DB stores; an unknown one is refused when the push
/// is decoded, so it can never be stored and then fail every read.
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
    /// On the case-file site vs private/app-only. Default private.
    #[serde(default)]
    pub shared: bool,
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    #[serde(default)]
    pub rev: u64,
}

/// One wellbeing check-in as it travels over sync. `recorded_at` is the moment
/// the feeling was (UTC, RFC3339 on the wire).
///
/// The readings ride in TENTHS of a point: 10..50, where 35 is a 3.5 — a mood
/// between two faces ("4, but a bit lower at the gym"). Fixed-point integers, so
/// they average and compare exactly; the name carries the scale so no layer can
/// mistake a 4 for a 0.4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellbeingDoc {
    pub ulid: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(rename = "recordedAt")]
    pub recorded_at: DateTime<Utc>,
    #[serde(rename = "scoreTenths")]
    pub score_tenths: u8,
    /// Optional energy reading (10..50 tenths, drained..energetic; higher = better,
    /// like the score); `None` = mood-only check-in.
    #[serde(default, rename = "energyTenths")]
    pub energy_tenths: Option<u8>,
    /// Fine-grained feelings-wheel tokens; independent of mood and energy.
    #[serde(default)]
    pub emotions: Vec<String>,
    pub note: Option<String>,
    #[serde(rename = "_deleted", default)]
    pub deleted: bool,
    #[serde(default)]
    pub rev: u64,
}

/// One to-do connection as it travels over sync. The kinds are typed as on a
/// to-do; the endpoints are soft refs.
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

/// A page of pulled documents plus the advanced checkpoint.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct PullResponse<D> {
    pub documents: Vec<D>,
    pub checkpoint: Checkpoint,
}

/// The opaque (to the client) pull cursor: the highest `rev` delivered so far.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Checkpoint {
    #[ts(type = "number")]
    pub rev: u64,
}

/// One change pushed by the client: the desired new state, plus the master state
/// the client assumed (null for a fresh insert) — used for optimistic-concurrency
/// conflict detection. The explicit `DeserializeOwned` bound (rather than serde's
/// inferred `Deserialize<'de>`) keeps the doc type usable as a `Json` body — the
/// inferred higher-ranked bound otherwise fails to satisfy axum's extractor.
#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "D: serde::de::DeserializeOwned"))]
pub struct PushEntry<D> {
    #[serde(rename = "newDocumentState")]
    pub new_document_state: D,
    #[serde(rename = "assumedMasterState", default)]
    pub assumed_master_state: Option<D>,
}
