//! "Which shops sell this?" from memory, in two queries: a shop's own listing
//! (`product_listings`) or a sighting (`shop_listings`). Never a stock check.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use ts_rs::TS;

use super::ids::{Barcode, ProductId};
use super::prices::Currency;
use super::source::Source;

/// One Buy row. `key` is the client's ulid, echoed back for it to join on.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct CoverageQuery {
    pub key: String,
    /// What the phone scanned; one not barcode-shaped teaches nothing.
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub product_id: Option<ProductId>,
}

/// Empty `sources` means unknown, not "nowhere".
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RowCoverage {
    pub key: String,
    /// Sorted, so the display is stable.
    pub sources: Vec<Source>,
    /// Only a linked product has prices; a sighting has none.
    pub prices: Vec<RowPrice>,
}

/// What the shop charges, never what anybody paid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RowPrice {
    pub source: Source,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
}

/// The cheapest listing's newest observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingPrice {
    pub product_id: ProductId,
    pub price: RowPrice,
}

/// More than a shopping trip is a client bug.
pub const MAX_ROWS: usize = 200;

/// A shop's own listing. Named, so it cannot be passed where a sighting belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedListing {
    pub product_id: ProductId,
    pub source: Source,
}

/// A past query showed this barcode at this shop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub barcode: Barcode,
    pub source: Source,
}

/// Fold both tables onto the rows that asked; a shop in both is one answer.
pub fn combine(
    queries: &[CoverageQuery],
    attached: &[AttachedListing],
    seen: &[Sighting],
    prices: &[ListingPrice],
) -> Vec<RowCoverage> {
    let mut priced: HashMap<ProductId, Vec<RowPrice>> = HashMap::new();
    for p in prices {
        priced
            .entry(p.product_id)
            .or_default()
            .push(p.price.clone());
    }
    let mut by_product: HashMap<ProductId, Vec<Source>> = HashMap::new();
    for l in attached {
        by_product.entry(l.product_id).or_default().push(l.source);
    }
    let mut by_barcode: HashMap<&Barcode, Vec<Source>> = HashMap::new();
    for s in seen {
        by_barcode.entry(&s.barcode).or_default().push(s.source);
    }
    queries
        .iter()
        .map(|q| {
            // Dedupes and sorts.
            let mut sources: BTreeSet<Source> = BTreeSet::new();
            if let Some(id) = q.product_id
                && let Some(found) = by_product.get(&id)
            {
                sources.extend(found);
            }
            if let Some(barcode) = q.barcode.as_deref().and_then(as_barcode)
                && let Some(found) = by_barcode.get(&barcode)
            {
                sources.extend(found);
            }
            let mut prices = q
                .product_id
                .and_then(|id| priced.get(&id).cloned())
                .unwrap_or_default();
            prices.sort_by_key(|p| p.source);
            RowCoverage {
                key: q.key.clone(),
                sources: sources.into_iter().collect(),
                prices,
            }
        })
        .collect()
}

/// Empty when nothing is linked, and the query is skipped.
pub fn product_ids(queries: &[CoverageQuery]) -> Vec<ProductId> {
    let set: BTreeSet<ProductId> = queries.iter().filter_map(|q| q.product_id).collect();
    set.into_iter().collect()
}

/// Blank and malformed values are dropped: `barcode = ''` would match every
/// other such row.
fn as_barcode(raw: &str) -> Option<Barcode> {
    raw.parse().ok()
}

pub fn barcodes(queries: &[CoverageQuery]) -> Vec<Barcode> {
    let set: BTreeSet<Barcode> = queries
        .iter()
        .filter_map(|q| q.barcode.as_deref())
        .filter_map(as_barcode)
        .collect();
    set.into_iter().collect()
}
