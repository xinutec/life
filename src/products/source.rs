//! Where product data came from: every `source` and provenance column holds one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::ExternalId;
use crate::str_enum;

str_enum! {
    /// A shop, Open Food Facts, or our own hand entry. Alphabetical on purpose: the
    /// derived `Ord` orders shop lists without ranking any shop by position; a real
    /// preference is [`Source::name_rank`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export)]
    pub enum Source: "source" {
        Asda => "asda",
    /// The crowd-sourced catalogue, not a shop.
        Off => "off",
    /// Typed by hand: authoritative over every shop.
        User => "user",
        Waitrose => "waitrose",
    }
}

impl Source {
    /// Somewhere you can buy the thing; also what may be imported.
    pub fn is_shop(self) -> bool {
        match self {
            Source::Asda | Source::Waitrose => true,
            Source::Off | Source::User => false,
        }
    }

    pub fn shops() -> impl Iterator<Item = Source> {
        Source::ALL.iter().copied().filter(|s| s.is_shop())
    }

    /// Image-host suffixes for the SSRF guard; empty means no adoptable picture.
    pub fn image_hosts(self) -> &'static [&'static str] {
        match self {
            Source::Asda => &["scene7.com"],
            Source::Off => &["openfoodfacts.org"],
            Source::Waitrose => &["wtrecom.com"],
            Source::User => &[],
        }
    }

    /// The product page derived from the listing's id alone (Asda's is slugless,
    /// Waitrose redirects any slug); an [`ExternalId`] splices safely. `None`
    /// without a page.
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

    /// Lower wins the canonical name; `None` never names. Retailers curate titles,
    /// OFF's are crowd-sourced. `user` takes the name outright instead.
    pub fn name_rank(self) -> Option<usize> {
        match self {
            Source::Waitrose => Some(0),
            Source::Asda => Some(1),
            Source::Off => Some(2),
            Source::User => None,
        }
    }
}
