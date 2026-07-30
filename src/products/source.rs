//! Where a piece of product data came from — the closed set the whole product
//! domain turns on.
//!
//! `products.source`, `product_listings.source`, `shop_listings.source`,
//! `price_observations.source`, the facts tables' `source`, and the
//! `{name,image}_source` provenance columns all name a value from this set and
//! nothing else. It is an enum rather than a `String` for the reason
//! [[super::nutrition::Presence]] is: a closed set in the type system can't have
//! a fifth spelling invented at a call site, `match` tells us every place that
//! must change when a shop is added, and ts-rs hands the frontend the union
//! `"asda" | "off" | "user" | "waitrose"` instead of a bare `string` it would
//! have to re-assert.
//!
//! Adding a shop is: one variant, the arms the compiler then demands, and the
//! frontend's display label. Nothing else needs finding.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::ExternalId;

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
pub enum Source {
    Asda,
    /// Open Food Facts — the crowd-sourced catalogue, not a shop.
    Off,
    /// Typed by hand. Our own layer: authoritative over every shop, because it
    /// is how a product still reads correctly when every source is wrong.
    User,
    Waitrose,
}

impl Source {
    /// Every source. Iterate this rather than writing a set out again — a new
    /// variant then reaches every list that derives from it.
    pub const ALL: [Source; 4] = [Source::Asda, Source::Off, Source::User, Source::Waitrose];

    /// Somewhere you can walk into and buy the thing.
    ///
    /// This is the predicate behind both "which shops carry it" and "what may be
    /// imported through `POST /api/products/import`": Open Food Facts and
    /// hand-entry each have their own path in and are not places. The two
    /// questions coincide today and share this one answer rather than two lists
    /// that could drift apart.
    pub fn is_shop(self) -> bool {
        match self {
            Source::Asda | Source::Waitrose => true,
            Source::Off | Source::User => false,
        }
    }

    /// Every shop, in display order.
    pub fn shops() -> impl Iterator<Item = Source> {
        Source::ALL.into_iter().filter(|s| s.is_shop())
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
    /// alone — no slug needed (probed 2026-07-16: Asda's PDP is slugless and the
    /// old groceries.asda.com host just 301s to it; Waitrose redirects any slug
    /// to the canonical one, keyed by the trailing lineNumber).
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

    /// The value stored in the database and sent on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Asda => "asda",
            Source::Off => "off",
            Source::User => "user",
            Source::Waitrose => "waitrose",
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

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// --- Database mapping ---
//
// Written out rather than `#[derive(sqlx::Type)]`: the derive declares the type
// as a SQL `ENUM`, and every `source` column here is a `VARCHAR`, so decoding a
// real row fails at runtime with a type mismatch. These delegate to `str`, which
// is what the columns actually hold.
//
// Decoding parses, so a value in the database that isn't a `Source` fails the
// query loudly instead of arriving as something the rest of the code would have
// to second-guess.

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

impl FromStr for Source {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "asda" => Ok(Source::Asda),
            "off" => Ok(Source::Off),
            "user" => Ok(Source::User),
            "waitrose" => Ok(Source::Waitrose),
            other => Err(format!("unknown source {other:?}")),
        }
    }
}
