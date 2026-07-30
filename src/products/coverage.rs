//! "Which shops can I get this at?", answered from memory alone.
//!
//! Two kinds of knowledge, deliberately merged rather than ranked:
//!
//! - an **attached listing** (`product_listings`) — this shop's own line for the
//!   catalogue product, the strongest thing we can hold;
//! - a **sighting** (`shop_listings`, see [[super::shop_cache]]) — a shop query
//!   we once ran showed this barcode at that shop.
//!
//! Neither is a stock check and this module must never be described as one. A
//! shop carrying a product last month says nothing about the shelf tonight; what
//! it does say is where the thing is *sold*, which is the question a Buy list
//! actually asks — "can I do this trip in one shop?".
//!
//! Nothing here goes out to a shop. It reads what earlier lookups already paid
//! for, so a whole list costs two queries and no outbound traffic.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use ts_rs::TS;

use super::ids::{Barcode, ProductId};
use super::source::Source;

/// One row of a Buy list, as the client asks about it. `key` is the client's own
/// identifier for the row (its ulid) and is echoed back untouched: the client
/// joins on that rather than re-deriving identity, so the two sides can never
/// disagree about which answer belongs to which row.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct CoverageQuery {
    pub key: String,
    /// Whatever the client's row carries — plain text, since a Buy row's barcode
    /// is what a phone scanned rather than a catalogue key (see [[super::ids]]).
    /// One that isn't barcode-shaped simply teaches us nothing here.
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub product_id: Option<ProductId>,
}

/// Where one row is known to be sold. `sources` is empty when we know nothing —
/// which is NOT the same as "nowhere sells it", and the UI has to say so.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RowCoverage {
    pub key: String,
    /// The shops, sorted (see [`Source`]'s alphabetical ordering) so the display
    /// order is stable across reloads rather than following row order in the DB.
    pub sources: Vec<Source>,
}

/// The most rows one request may ask about. A Buy list is a shopping trip, not a
/// catalogue export; a client sending more than this is a bug.
pub const MAX_ROWS: usize = 200;

/// A shop's own listing for a catalogue product — the strong half of what we
/// know (`product_listings`).
///
/// Named rather than a tuple because a sighting is also "an identifier and a
/// shop": as tuples the two kinds would be interchangeable, and passing one
/// where the other belongs would compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedListing {
    pub product_id: ProductId,
    pub source: Source,
}

/// A shop query of ours once showed this barcode at this shop — the weak half
/// (`shop_listings`). Not a stock check; see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub barcode: Barcode,
    pub source: Source,
}

/// Fold what the two tables know onto the rows that asked.
///
/// Split out of the route so the rule is testable without a database. Both
/// inputs may hold several rows per key, and a shop that appears in both is one
/// answer, not two.
pub fn combine(
    queries: &[CoverageQuery],
    attached: &[AttachedListing],
    seen: &[Sighting],
) -> Vec<RowCoverage> {
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
            // BTreeSet: dedupe (a shop can be both attached and sighted) and
            // sort in one step.
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
            RowCoverage {
                key: q.key.clone(),
                sources: sources.into_iter().collect(),
            }
        })
        .collect()
}

/// The product ids worth querying for — deduped, and empty when nothing on the
/// list is linked (in which case the caller skips the query entirely).
pub fn product_ids(queries: &[CoverageQuery]) -> Vec<ProductId> {
    let set: BTreeSet<ProductId> = queries.iter().filter_map(|q| q.product_id).collect();
    set.into_iter().collect()
}

/// A row's barcode, when it is one. Blank and malformed values are dropped
/// rather than queried: a row carrying something that isn't a barcode knows
/// nothing about shops, and `barcode = ''` in particular would match every other
/// such row in the cache.
fn as_barcode(raw: &str) -> Option<Barcode> {
    raw.parse().ok()
}

/// The barcodes worth querying for, deduped. See [`as_barcode`] for what's dropped.
pub fn barcodes(queries: &[CoverageQuery]) -> Vec<Barcode> {
    let set: BTreeSet<Barcode> = queries
        .iter()
        .filter_map(|q| q.barcode.as_deref())
        .filter_map(as_barcode)
        .collect();
    set.into_iter().collect()
}
