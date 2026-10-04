//! Taking in one source's account of a product: the decisions, pure. Open Food
//! Facts by barcode, a shop import and an Asda refresh all arrive as a
//! [`SourceAccount`]; `repo::ingest` reads what the product holds and applies
//! what these functions decide, in one transaction.
//!
//! The rule throughout is fill-if-empty: a source disagreeing with what we hold
//! is a divergence to approve (`repo::divergences`), never applied behind your
//! back. The one exception is a barcodeless product, which only its own source
//! lists, so that source may refresh its name and brand — except a value we made
//! our own.

use super::ids::{Barcode, ExternalId};
use super::nutrition::{DietaryFlag, ProductFacts};
use super::prices::PriceInput;
use super::repo::Listing;
use super::source::Source;
use super::types::Product;

/// What an account says about the facts.
#[derive(Debug, Clone, PartialEq)]
pub enum FactsUpdate {
    /// Nothing: an import names the product but states no facts.
    None,
    /// Only dietary claims (an Asda search hit's lifestyle tags). Allergens and
    /// the rest are left as this source last stated them.
    Dietary(Vec<DietaryFlag>),
    /// The source's whole account of the facts (Open Food Facts), replacing
    /// what it said before.
    Full(Box<ProductFacts>),
}

/// One source's whole account of one product, fetched before anything is
/// written so no network call happens inside the transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceAccount {
    pub source: Source,
    pub external_id: ExternalId,
    pub barcode: Option<Barcode>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    pub url: Option<String>,
    pub image_url: Option<String>,
    /// The source's record verbatim, kept on its listing.
    pub raw_json: Option<String>,
    pub price: Option<PriceInput>,
    pub facts: FactsUpdate,
}

/// What a canonical product holds, as far as an ingest decides anything.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    /// The name, or the brand, was typed by us: no source may replace it.
    pub name_ours: bool,
    pub brand_ours: bool,
    /// Barcodeless and reached through this source's own listing, so nothing
    /// else lists it to disagree.
    pub single_owner: bool,
}

/// A field write: `None` leaves the column alone, `Some(v)` sets it to `v`.
pub type Write = Option<Option<String>>;

/// The canonical columns an account writes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalWrites {
    pub name: Write,
    pub brand: Write,
    pub quantity_label: Write,
}

fn blank(v: Option<&str>) -> bool {
    v.is_none_or(|s| s.trim().is_empty())
}

/// `incoming` if the column is empty and the account has a value, else leave it.
fn fill(held: Option<&str>, incoming: Option<&str>) -> Write {
    match incoming.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if blank(held) => Some(Some(v.to_string())),
        _ => None,
    }
}

/// What this account writes to the canonical row it lands on.
///
/// A name on a shared (barcoded) product is not written here: the best-ranked
/// listing names it ([`best_name`]), whichever source arrived first.
pub fn canonical_writes(held: &Held, account: &SourceAccount) -> CanonicalWrites {
    let (name, brand) = if held.single_owner {
        let refresh = |ours: bool, v: &Option<String>| (!ours).then(|| v.clone());
        (
            refresh(held.name_ours, &account.name),
            refresh(held.brand_ours, &account.brand),
        )
    } else {
        (None, fill(held.brand.as_deref(), account.brand.as_deref()))
    };
    CanonicalWrites {
        name,
        brand,
        // The product's own pack size (OFF's quantity) and a shop's ("22x27G")
        // both only fill a gap.
        quantity_label: fill(
            held.quantity_label.as_deref(),
            account.quantity_label.as_deref(),
        ),
    }
}

/// The picture worth fetching for this account, if any: only for a product that
/// has none (a held picture is replaced through the picture reconcile, which the
/// listing's `image_url` feeds), and only from a source with picture hosts.
pub fn picture_to_fetch<'a>(
    current: Option<&Product>,
    account: &'a SourceAccount,
) -> Option<&'a str> {
    let url = account
        .image_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())?;
    let has_one = current.is_some_and(|p| p.has_image);
    (!has_one && !account.source.image_hosts().is_empty()).then_some(url)
}

/// The name a product with none should take: the best-ranked source's
/// ([`Source::name_rank`]) non-blank title, the oldest listing on a tie.
pub fn best_name(listings: &[Listing]) -> Option<(&str, Source)> {
    listings
        .iter()
        .filter_map(|l| {
            let name = l
                .raw_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())?;
            Some((l.source.name_rank()?, name, l.source))
        })
        .min_by_key(|(rank, ..)| *rank)
        .map(|(_, name, source)| (name, source))
}
