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

use super::source::Source;

/// One row of a Buy list, as the client asks about it. `key` is the client's own
/// identifier for the row (its ulid) and is echoed back untouched: the client
/// joins on that rather than re-deriving identity, so the two sides can never
/// disagree about which answer belongs to which row.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct CoverageQuery {
    pub key: String,
    #[serde(default)]
    pub barcode: Option<String>,
    #[ts(type = "number | null")]
    #[serde(default)]
    pub product_id: Option<u64>,
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
/// Named rather than a `(u64, Source)` tuple because the two halves of a
/// sighting are also "an id and a shop": as tuples the two kinds are the same
/// type, and passing one where the other belongs would compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedListing {
    pub product_id: u64,
    pub source: Source,
}

/// A shop query of ours once showed this barcode at this shop — the weak half
/// (`shop_listings`). Not a stock check; see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    pub barcode: String,
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
    let mut by_product: HashMap<u64, Vec<Source>> = HashMap::new();
    for l in attached {
        by_product.entry(l.product_id).or_default().push(l.source);
    }
    let mut by_barcode: HashMap<&str, Vec<Source>> = HashMap::new();
    for s in seen {
        by_barcode
            .entry(s.barcode.as_str())
            .or_default()
            .push(s.source);
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
            if let Some(barcode) = q.barcode.as_deref()
                && let Some(found) = by_barcode.get(barcode)
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
pub fn product_ids(queries: &[CoverageQuery]) -> Vec<u64> {
    let set: BTreeSet<u64> = queries.iter().filter_map(|q| q.product_id).collect();
    set.into_iter().collect()
}

/// The barcodes worth querying for, deduped. Blank strings are dropped: a row
/// with an empty barcode knows nothing, and `barcode = ''` would match every
/// other such row in the cache.
pub fn barcodes(queries: &[CoverageQuery]) -> Vec<String> {
    let set: BTreeSet<&str> = queries
        .iter()
        .filter_map(|q| q.barcode.as_deref())
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .collect();
    set.into_iter().map(str::to_string).collect()
}
