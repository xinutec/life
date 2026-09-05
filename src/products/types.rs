//! Product wire types: the canonical product, its per-source listings, and the
//! aggregate the product page fetches.

use crate::purchases::types::Purchase;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::{Barcode, ExternalId, ProductId};
use super::nutrition::ProductFacts;
use super::packsize::PackSize;
use super::prices::ShopPrice;
use super::source::Source;
use crate::str_enum;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Product {
    /// Catalog id (surrogate key). A product may have no barcode (hand-defined).
    pub id: ProductId,
    pub barcode: Option<Barcode>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    /// `quantity_label` read as an amount — see [`super::packsize`]. What lets
    /// stock linked to this product start out knowing how much it holds.
    ///
    /// Derived on read, never stored: the label is reconcilable between sources
    /// and hand-editable, so a stored copy would be one more thing that can
    /// disagree with it. `None` when there is no label, or none we would rather
    /// guess at than refuse.
    pub pack: Option<PackSize>,
    /// Where the row came from. `None` only for rows predating provenance.
    pub source: Option<Source>,
    /// Source-scoped external id (e.g. a Waitrose lineNumber). Unique per source;
    /// how a shop product with no barcode is addressed and de-duped.
    pub external_id: Option<ExternalId>,
    /// Which source's title `name` currently is (see repo's canonical-name
    /// refresh) — provenance for display, never hand-assigned.
    pub name_source: Option<Source>,
    /// Which source the cached picture came from — provenance for picture
    /// reconciliation. `None` when unknown.
    pub image_source: Option<Source>,
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
    pub source: Source,
    pub external_id: ExternalId,
    /// Deep link to the source's product page, when it has one.
    pub url: Option<String>,
    /// What this source titles the product (the canonical `name` picks among
    /// these).
    pub raw_name: Option<String>,
}

/// A raw payload we fetched from a source and kept verbatim (product_documents,
/// 0034) — metadata only, so the UI can show what's already held (and when) and
/// avoid re-fetching. The body itself is read on demand, not shipped here.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SourceDocument {
    pub source: Source,
    /// Which fetch it was ('page' = Asda's Brandbank product-page blob).
    pub kind: String,
    /// When we fetched it (epoch millis).
    #[ts(type = "number")]
    pub fetched_at: i64,
    /// Size of the stored payload, bytes — a hint that we hold it, not the body.
    #[ts(type = "number")]
    pub bytes: i64,
}

/// One source's value for a field that disagrees with the canonical product —
/// a choice you can adopt.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Candidate {
    /// The source offering this value.
    pub source: Source,
    /// The source's value for the field, as a display string.
    pub value: String,
}

str_enum! {
    /// A field of a product that sources can disagree about.
    ///
    /// Closed, because every one of them has to be handled by name somewhere: the
    /// route splits the picture out (its bytes come through the SSRF gate), the repo
    /// splits facts out (they record a trusted source rather than copying a value),
    /// and each scalar needs a reader for its current and offered values. As a
    /// `String` those four dispatch sites agreed only by convention and a new field
    /// could silently fall through all of them; as an enum, [`Self::reconciler`] is
    /// exhaustive and the compiler names every site that must learn about it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum ReconcileField: "reconcile field" {
        Name => "name",
        Brand => "brand",
        QuantityLabel => "quantity_label",
        Picture => "picture",
        Nutrition => "nutrition",
        Ingredients => "ingredients",
    }
}

/// Which machinery settles a field — see [`ReconcileField`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciler {
    /// A canonical scalar on `products`: adopting copies the value across.
    Scalar,
    /// The picture. Provenance-based rather than value-based, and adopting means
    /// re-fetching bytes through the SSRF gate — I/O the route layer owns.
    Picture,
    /// A fact trusted from one source (`product_fact_sources`): adopting records
    /// whose account to believe, and never invents or copies a value.
    Fact,
}

impl ReconcileField {
    pub fn reconciler(self) -> Reconciler {
        match self {
            ReconcileField::Name | ReconcileField::Brand | ReconcileField::QuantityLabel => {
                Reconciler::Scalar
            }
            ReconcileField::Picture => Reconciler::Picture,
            ReconcileField::Nutrition | ReconcileField::Ingredients => Reconciler::Fact,
        }
    }

    /// Human label, shown as the row heading in the diff.
    pub fn label(self) -> &'static str {
        match self {
            ReconcileField::Name => "Name",
            ReconcileField::Brand => "Brand",
            ReconcileField::QuantityLabel => "Pack size",
            ReconcileField::Picture => "Picture",
            ReconcileField::Nutrition => "Nutrition",
            ReconcileField::Ingredients => "Ingredients",
        }
    }
}
/// What to do about one field's divergence: keep what we have, or adopt one
/// source's account of it.
///
/// The variants after `Keep` are exactly [`Source`]'s — kept as one flat set
/// because that is what travels on the wire (`"keep" | "asda" | …`). The
/// conversions to and from `Source` are exhaustive matches, so adding a shop
/// fails to compile here until this list grows too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Choice {
    /// Leave the canonical value alone; just settle the divergence.
    Keep,
    Asda,
    Off,
    User,
    Waitrose,
}

impl Choice {
    /// The source being adopted, or `None` for [`Choice::Keep`].
    pub fn source(self) -> Option<Source> {
        match self {
            Choice::Keep => None,
            Choice::Asda => Some(Source::Asda),
            Choice::Off => Some(Source::Off),
            Choice::User => Some(Source::User),
            Choice::Waitrose => Some(Source::Waitrose),
        }
    }
}

impl From<Source> for Choice {
    fn from(s: Source) -> Self {
        match s {
            Source::Asda => Choice::Asda,
            Source::Off => Choice::Off,
            Source::User => Choice::User,
            Source::Waitrose => Choice::Waitrose,
        }
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source() {
            Some(s) => write!(f, "{s}"),
            None => f.write_str("keep"),
        }
    }
}

/// One decision in a reconcile request: what to do about one field.
///
/// Shared by the route and the repo rather than each holding its own copy —
/// there used to be two structurally identical structs and a hand-written copy
/// between them, which is one more place for the two ideas of a decision to
/// drift apart.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct FieldChoice {
    pub field: ReconcileField,
    pub choice: Choice,
    /// The typed value, when `choice` is [`Choice::User`]. `#[ts(optional)]` so
    /// the generated type says `value?: string` — matching `serde(default)`
    /// exactly, rather than forcing every keep/adopt decision to spell out a
    /// null it doesn't have.
    #[serde(default)]
    #[ts(optional)]
    pub value: Option<String>,
}

/// A field where at least one source disagrees with the canonical product and no
/// decision has settled it yet.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FieldDivergence {
    pub field: ReconcileField,
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

/// One source's own account of the facts — its nutrition panel, ingredients,
/// allergens, and dietary claims, exactly as that source gave them. The product
/// page shows these side by side as provenance: for the safety-critical facts
/// (allergens, dietary) it's how you see *who* declared what, since those merge
/// by union / tri-state and are never reduced to a single-source pick.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SourceFacts {
    /// The source these facts came from.
    pub source: Source,
    pub facts: ProductFacts,
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
    /// Each source's own facts, for provenance (who declared which allergen, whose
    /// nutrition panel is which). Oldest-ranked source order.
    pub facts_by_source: Vec<SourceFacts>,
    /// Where the sources disagree with the canonical row and you haven't decided
    /// yet — the diff to approve. Empty when everything agrees or is settled.
    /// Includes the facts that reconcile by source-pick (nutrition, ingredients).
    pub reconciliation: ProductReconciliation,
    /// Raw source payloads we've fetched and kept (see SourceDocument) — so the
    /// UI knows what's already stored and needn't re-fetch it.
    pub documents: Vec<SourceDocument>,
    /// What THIS person has paid for it, newest first — a different claim from
    /// `prices`, which is what shops charge. Shown together they answer "is this
    /// the going rate"; shown interchangeably they would be a lie, so they are
    /// two fields and not one list.
    ///
    /// Matched by product id OR barcode, so a purchase made before the catalogue
    /// link existed, or one whose link was corrected, still appears.
    pub purchases: Vec<Purchase>,
}
