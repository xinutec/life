//! Product wire types: the canonical product, its per-source listings, and the
//! aggregate the product page fetches.

use serde::Serialize;
use ts_rs::TS;

use super::nutrition::ProductFacts;
use super::prices::ShopPrice;

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
    /// Which source's title `name` currently is (see repo's canonical-name
    /// refresh) — provenance for display, never hand-assigned.
    pub name_source: Option<String>,
    /// True if we have a cached image. Served from /api/products/id/{id}/image
    /// (barcodeless shop products), or /api/products/{barcode}/image when barcoded.
    pub has_image: bool,
}

/// One source's listing of a product, with its public product page resolved
/// (stored URL if the source supplied one, else derived from the listing's
/// identity — see source::listing_url).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductListing {
    pub source: String,
    pub external_id: String,
    /// Deep link to the source's product page, when it has one.
    pub url: Option<String>,
    /// What this source titles the product (the canonical `name` picks among
    /// these).
    pub raw_name: Option<String>,
}

/// One source's value for a field that disagrees with the canonical product —
/// a choice you can adopt.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Candidate {
    /// The source offering this value ('off', 'asda', 'waitrose', …).
    pub source: String,
    /// The source's value for the field, as a display string.
    pub value: String,
}

/// A field where at least one source disagrees with the canonical product and no
/// decision has settled it yet.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FieldDivergence {
    /// The canonical field: 'name' | 'brand' | 'quantity_label'.
    pub field: String,
    /// Human label for the field ('Name', 'Brand', 'Pack size').
    pub label: String,
    /// The current canonical value, or None when the product has none.
    pub current: Option<String>,
    /// Each source whose value differs from the current one — the choices to
    /// adopt, one per source (two sources may agree on the same value).
    pub candidates: Vec<Candidate>,
}

/// What a product's sources disagree about, for you to approve — empty when
/// there is nothing to review. Computed live from the listings vs the canonical
/// row, minus anything already decided (see repo's field decisions), so it never
/// goes stale.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductReconciliation {
    pub fields: Vec<FieldDivergence>,
}

/// Everything the product page shows, in one fetch —
/// GET /api/products/id/{id}.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductDetail {
    pub product: Product,
    /// Every source that lists the product, oldest first.
    pub listings: Vec<ProductListing>,
    /// Latest price per shop, cheapest first.
    pub prices: Vec<ShopPrice>,
    pub facts: ProductFacts,
    /// Where the sources disagree with the canonical row and you haven't decided
    /// yet — the diff to approve. Empty when everything agrees or is settled.
    pub reconciliation: ProductReconciliation,
}
