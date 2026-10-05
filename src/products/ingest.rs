//! One source's account of a product: the decisions, pure; `repo::ingest` applies
//! them in one transaction. Fill-if-empty throughout: a disagreement is a
//! divergence to approve. Only a barcodeless product's own source may refresh its
//! name and brand, and never a value we made our own.

use super::ids::{Barcode, ExternalId};
use super::nutrition::{DietaryFlag, ProductFacts};
use super::prices::PriceInput;
use super::repo::Listing;
use super::source::Source;
use super::types::Product;

#[derive(Debug, Clone, PartialEq)]
pub enum FactsUpdate {
    None,
    /// Only dietary claims (Asda's lifestyle tags).
    Dietary(Vec<DietaryFlag>),
    /// The whole account, replacing what this source said before.
    Full(Box<ProductFacts>),
}

/// Fetched before anything is written, so no network call runs inside the
/// transaction.
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
    pub raw_json: Option<String>,
    pub price: Option<PriceInput>,
    pub facts: FactsUpdate,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Held {
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    /// Typed by us: no source replaces it.
    pub name_ours: bool,
    pub brand_ours: bool,
    /// Barcodeless and reached through this source's listing.
    pub single_owner: bool,
}

/// `None` leaves the column; `Some(v)` sets it.
pub type Write = Option<Option<String>>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanonicalWrites {
    pub name: Write,
    pub brand: Write,
    pub quantity_label: Write,
}

fn blank(v: Option<&str>) -> bool {
    v.is_none_or(|s| s.trim().is_empty())
}

fn fill(held: Option<&str>, incoming: Option<&str>) -> Write {
    match incoming.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if blank(held) => Some(Some(v.to_string())),
        _ => None,
    }
}

/// What this account writes. A shared product's name comes from the best-ranked
/// listing ([`best_name`]), not from here.
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
        quantity_label: fill(
            held.quantity_label.as_deref(),
            account.quantity_label.as_deref(),
        ),
    }
}

/// Only for a product without one (a held picture changes through reconcile), and
/// only from a source with picture hosts.
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

/// The best-ranked source's title ([`Source::name_rank`]), the oldest on a tie.
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
