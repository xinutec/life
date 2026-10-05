//! Product wire types.

use crate::purchases::types::Purchase;
use std::fmt;

use chrono::{DateTime, Utc};

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
    /// A product may have no barcode.
    pub id: ProductId,
    pub barcode: Option<Barcode>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    /// `quantity_label` read as an amount ([`super::packsize`]); derived on read,
    /// as the label can be corrected. `None` for one we would rather not guess.
    pub pack: Option<PackSize>,
    /// `None` only for rows older than provenance.
    pub source: Option<Source>,
    /// Unique per source; how a barcodeless shop product is found again.
    pub external_id: Option<ExternalId>,
    /// Which source's title `name` is.
    pub name_source: Option<Source>,
    /// `None` exactly when there is no picture.
    pub image_source: Option<Source>,
    pub has_image: bool,
}

/// One source's listing, its page link resolved (stored, or derived from its id).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductListing {
    pub source: Source,
    pub external_id: ExternalId,
    pub url: Option<String>,
    /// What this source calls the product.
    pub raw_name: Option<String>,
}

/// A raw payload kept verbatim (0034): metadata only, so the UI knows what is
/// held without shipping the body.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct SourceDocument {
    pub source: Source,
    pub kind: DocKind,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub fetched_at: DateTime<Utc>,
    #[ts(type = "number")]
    pub bytes: i64,
}

crate::str_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum DocKind: "document kind" {
    /// Asda's product page, with its Brandbank blob.
        Page => "page",
    }
}

/// One source's value for a disputed field.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Candidate {
    pub source: Source,
    /// As a display string.
    pub value: String,
}

str_enum! {
    /// A product field sources can disagree on. Closed, so a new field fails to
    /// compile until every dispatch handles it.
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

/// Which machinery settles a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciler {
    /// A scalar on `products`: adopting copies the value.
    Scalar,
    /// Settled by provenance; adopting fetches bytes, which the route owns.
    Picture,
    /// Settled by recording whose account to trust; no value is copied.
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

    /// The row heading in the diff.
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
/// Keep what we have, or adopt one source's account. The variants after `Keep`
/// are exactly [`Source`]'s, as the wire carries one flat set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Choice {
    /// Leave the value; just settle the divergence.
    Keep,
    Asda,
    Off,
    User,
    Waitrose,
}

impl Choice {
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

#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct FieldChoice {
    pub field: ReconcileField,
    pub choice: Choice,
    /// For [`Choice::User`]; optional on the wire, as `serde(default)` makes it.
    #[serde(default)]
    #[ts(optional)]
    pub value: Option<String>,
}

/// A field some source disputes, not yet settled.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct FieldDivergence {
    pub field: ReconcileField,
    pub label: String,
    pub current: Option<String>,
    /// One per differing source.
    pub candidates: Vec<Candidate>,
}

/// What the sources disagree on, computed live minus what was decided.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductReconciliation {
    pub fields: Vec<FieldDivergence>,
}

/// One source's own facts, shown side by side: allergens and diets merge and
/// never reduce to one source, so this is how you see who declared what.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct SourceFacts {
    pub source: Source,
    pub facts: ProductFacts,
}

/// Everything the product page shows (GET /api/products/id/{id}).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductDetail {
    pub product: Product,
    /// Oldest first.
    pub listings: Vec<ProductListing>,
    /// Cheapest first, one per shop.
    pub prices: Vec<ShopPrice>,
    pub facts: ProductFacts,
    /// In source precedence order.
    pub facts_by_source: Vec<SourceFacts>,
    /// The disagreements to settle, including the source-picked facts.
    pub reconciliation: ProductReconciliation,
    pub documents: Vec<SourceDocument>,
    /// What this person paid, newest first: not `prices`, which is what shops
    /// charge. Matched by id or barcode, so older purchases still appear.
    pub purchases: Vec<Purchase>,
}
