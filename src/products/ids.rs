//! The product domain's identifiers as types, each validated once in `FromStr`,
//! so [`Source::listing_url`](super::source::Source::listing_url) and the Open
//! Food Facts client may splice them into URLs. The frontend's aliases are
//! documentation only.
//!
//! `shopping_items.barcode` is deliberately not a [`Barcode`]: it is a hint on a
//! synced row, and failing validation would strand an offline edit.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

/// A product's EAN/UPC: 1 to 14 digits, in one canonical padding.
///
/// GS1 compares GTINs as 14 digits, so `065928546009` and `0065928546009` are one
/// product. Stored as Open Food Facts normalises: zeros off, then padded to 8 or
/// 13, 14 kept — the printed form, with equal codes equal strings. Digits only is
/// what makes it safe to splice into the OFF URL.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(as = "String")]
pub struct Barcode(String);

impl Barcode {
    /// The value to store, send, or splice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Barcode {
    type Err = String;

    /// Trims first: a leading space is a transport artefact, not a different
    /// barcode. All zeros is a shop's "none" (Asda sends `0`), not a product.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.len() > 14 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err("barcode must be 1-14 digits".to_string());
        }
        let digits = s.trim_start_matches('0');
        let width = match digits.len() {
            0 => return Err("barcode must not be all zeros".to_string()),
            1..=8 => 8,
            9..=13 => 13,
            _ => 14,
        };
        Ok(Barcode(format!("{digits:0>width$}")))
    }
}

/// A scanned or typed barcode kept as a hint (an item's, a Buy row's): the
/// canonical [`Barcode`] when it is one, else as typed, so an odd code is kept
/// rather than refused.
pub fn barcode_hint(s: &str) -> String {
    s.parse::<Barcode>()
        .map_or_else(|_| s.trim().to_string(), |b| b.0)
}

/// A barcode is also a well-formed external id — digits are inside
/// `[A-Za-z0-9_-]` and 14 is inside 64 — which is what lets Open Food Facts key
/// its listing by the barcode itself. Infallible, and it stays infallible only
/// as long as both shapes above agree; widening [`Barcode`] means revisiting it.
impl From<&Barcode> for ExternalId {
    fn from(b: &Barcode) -> Self {
        ExternalId(b.0.clone())
    }
}

/// A source-scoped listing id: 1 to 64 characters of `[A-Za-z0-9_-]`, which is
/// what lets `listing_url` format it into a URL directly.
///
/// Asda's CIN, Waitrose's lineNumber, OFF's barcode. Unique only within its
/// [`Source`](super::source::Source): a listing's identity is the pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(as = "String")]
pub struct ExternalId(String);

impl ExternalId {
    /// The value to store, send, or splice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ExternalId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty()
            || s.len() > 64
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("external_id must be 1-64 chars of [A-Za-z0-9_-]".to_string());
        }
        Ok(ExternalId(s.to_string()))
    }
}

/// An allergen as Open Food Facts names it: "gluten", "sesame-seeds". Parsing
/// is the only way to make one, and maps any source's name to OFF's id, so
/// "Wheat" and "en:gluten" are one value. A name OFF doesn't list is kept,
/// lowercased: dropping it would read as "free from".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(as = "String")]
pub struct AllergenId(String);

impl AllergenId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AllergenId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // OFF's tags carry a language: "en:milk".
        let name = s.split_once(':').map_or(s, |(_, rest)| rest);
        let name = name.trim().to_lowercase();
        if name.is_empty() {
            return Err("an allergen needs a name".to_string());
        }
        Ok(AllergenId(
            super::allergens::off_id(&name).map_or(name, str::to_string),
        ))
    }
}

/// Deserialising validates, so a malformed id is refused by the request body's
/// own decoding — before a handler runs, and without the handler restating the
/// rule.
macro_rules! validating_deserialize {
    ($t:ty) => {
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

validating_deserialize!(Barcode);
validating_deserialize!(ExternalId);
validating_deserialize!(AllergenId);

/// Database mapping ([`varchar_sql!`](crate::varchar_sql)), `Display`, and
/// comparison with literals for the string ids.
macro_rules! string_id_sql {
    ($t:ty) => {
        $crate::varchar_sql!($t);

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        // Comparing against a literal reads naturally without letting a bare
        // string stand in for one: `id == "271105"` works, `f(some_string)`
        // still doesn't.
        impl PartialEq<str> for $t {
            fn eq(&self, other: &str) -> bool {
                self.as_str() == other
            }
        }

        impl PartialEq<&str> for $t {
            fn eq(&self, other: &&str) -> bool {
                self.as_str() == *other
            }
        }
    };
}

string_id_sql!(Barcode);
string_id_sql!(ExternalId);
string_id_sql!(AllergenId);

crate::row_id! {
    /// `products.id` — the canonical product every listing, price, fact and
    /// picture hangs off.
    ProductId
}

crate::row_id! {
    /// `product_listings.id` — one source's line for a product, and the FK a
    /// price observation is recorded against.
    ListingId
}
