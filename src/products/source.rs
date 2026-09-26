//! Where product data came from. Every `source` column (products, listings,
//! shop listings, prices, facts) and the `{name,image}_source` provenance
//! columns hold a value from this enum; adding a shop is a variant, the arms
//! the compiler demands, and the frontend's label.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::ExternalId;
use crate::str_enum;

str_enum! {
    /// A source of product data: a shop, Open Food Facts, or our own hand-entry.
    ///
    /// **Variants are alphabetical, and that is load-bearing**: the derived `Ord` is
    /// what `BTreeSet<Source>` sorts by, which is how shop lists reach the screen in
    /// a stable order (see [[super::coverage]]). Alphabetical means no shop is
    /// implicitly ranked above another by where it happens to sit in this list —
    /// where a genuine preference is meant, it is written down explicitly
    /// ([`Source::name_rank`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export)]
    pub enum Source: "source" {
        Asda => "asda",
        /// Open Food Facts — the crowd-sourced catalogue, not a shop.
        Off => "off",
        /// Typed by hand. Our own layer: authoritative over every shop, because it
        /// is how a product still reads correctly when every source is wrong.
        User => "user",
        Waitrose => "waitrose",
    }
}

impl Source {
    /// Somewhere you can walk into and buy the thing.
    ///
    /// This is the predicate behind both "which shops carry it" and "what may be
    /// imported through `POST /api/products/import`": Open Food Facts and
    /// hand-entry each have their own path in and are not places.
    pub fn is_shop(self) -> bool {
        match self {
            Source::Asda | Source::Waitrose => true,
            Source::Off | Source::User => false,
        }
    }

    /// Every shop, in display order.
    pub fn shops() -> impl Iterator<Item = Source> {
        Source::ALL.iter().copied().filter(|s| s.is_shop())
    }

    /// Allowed image-host suffixes for this source's picture, for the SSRF guard
    /// (https only; host must equal a suffix or be a subdomain of one).
    ///
    /// Empty means this source carries no adoptable picture — which is a real
    /// answer, not an absence, so it is an empty slice rather than a `None`.
    pub fn image_hosts(self) -> &'static [&'static str] {
        match self {
            // Products keyed by their CIN (see super::asda); images on the
            // (ungated) scene7 CDN, keyed by the product's EAN.
            Source::Asda => &["scene7.com"],
            Source::Off => &["openfoodfacts.org"],
            // Products keyed by their `lineNumber`; images on the (ungated) CDN.
            Source::Waitrose => &["wtrecom.com"],
            // Our own upload; there is no remote host to fetch from.
            Source::User => &[],
        }
    }

    /// The public product-page URL for a listing, derived from its identity
    /// alone: Asda's page is slugless, and Waitrose redirects any slug to the
    /// canonical one, keyed by the trailing lineNumber.
    ///
    /// Splicing is safe by construction: an [`ExternalId`] is
    /// `[A-Za-z0-9_-]{1,64}` and can carry no path segment or query parameter.
    /// `None` for a source with no page of its own.
    pub fn listing_url(self, external_id: &ExternalId) -> Option<String> {
        match self {
            Source::Off => Some(format!(
                "https://world.openfoodfacts.org/product/{external_id}"
            )),
            Source::Asda => Some(format!(
                "https://www.asda.com/groceries/product/{external_id}"
            )),
            Source::Waitrose => Some(format!(
                "https://www.waitrose.com/ecom/products/x/{external_id}"
            )),
            Source::User => None,
        }
    }

    /// Rank in the canonical-name preference order (lower wins), or `None` if
    /// this source never supplies the canonical name.
    ///
    /// Retailers curate their titles; Open Food Facts names are crowd-sourced and
    /// often messy. `user` is absent because a hand-typed name doesn't compete
    /// for the slot — it takes it outright (see repo's reconcile).
    pub fn name_rank(self) -> Option<usize> {
        match self {
            Source::Waitrose => Some(0),
            Source::Asda => Some(1),
            Source::Off => Some(2),
            Source::User => None,
        }
    }
}
// --- Database mapping ---
//
// Hand-written: `#[derive(sqlx::Type)]` declares a SQL `ENUM`, and the `source`
// columns are `VARCHAR`, which fails at runtime on real rows. Decoding parses,
// so a stored value that isn't a `Source` fails the query loudly.

impl sqlx::Type<sqlx::MySql> for Source {
    fn type_info() -> <sqlx::MySql as sqlx::Database>::TypeInfo {
        <str as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &<sqlx::MySql as sqlx::Database>::TypeInfo) -> bool {
        <str as sqlx::Type<sqlx::MySql>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::MySql> for Source {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::MySql as sqlx::Database>::ArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<'q, sqlx::MySql>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for Source {
    fn decode(
        value: <sqlx::MySql as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        <&str as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?
            .parse()
            .map_err(Into::into)
    }
}
