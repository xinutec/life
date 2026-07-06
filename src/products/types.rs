//! Product reference data (cached from Open Food Facts).

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Product {
    /// Catalog id (surrogate key). A product may have no barcode (hand-defined).
    #[ts(type = "number")]
    pub id: u64,
    pub barcode: Option<String>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    /// Where the row came from: 'off', 'user', or a shop ('waitrose', …).
    pub source: Option<String>,
    /// Source-scoped external id (e.g. a Waitrose lineNumber). Unique per source;
    /// how a shop product with no barcode is addressed and de-duped.
    pub external_id: Option<String>,
    /// True if we have a cached image. Served from /api/products/id/{id}/image
    /// (barcodeless shop products), or /api/products/{barcode}/image when barcoded.
    pub has_image: bool,
}
