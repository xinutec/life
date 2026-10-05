//! Product identifiers, each validated once in `FromStr`, so they splice safely
//! into URLs. `shopping_items.barcode` is not a [`Barcode`]: failing validation
//! would strand an offline edit.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

/// An EAN/UPC in one canonical padding, as Open Food Facts stores it: zeros off,
/// then padded to 8 or 13, 14 kept. GS1 compares GTINs as 14 digits, so
/// `065928546009` and `0065928546009` are one product.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(as = "String")]
pub struct Barcode(String);

impl Barcode {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Barcode {
    type Err = String;

    /// All zeros is a shop's "none" (Asda sends `0`).
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

/// A scanned barcode kept as a hint: canonical when it is one, else as typed.
pub fn barcode_hint(s: &str) -> String {
    s.parse::<Barcode>()
        .map_or_else(|_| s.trim().to_string(), |b| b.0)
}

/// Open Food Facts keys its listing by the barcode. Infallible only while
/// [`Barcode`] stays inside the external id's shape.
impl From<&Barcode> for ExternalId {
    fn from(b: &Barcode) -> Self {
        ExternalId(b.0.clone())
    }
}

/// 1 to 64 of `[A-Za-z0-9_-]`, unique within its source: Asda's CIN, Waitrose's
/// lineNumber, OFF's barcode.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(as = "String")]
pub struct ExternalId(String);

impl ExternalId {
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

/// An allergen by its Open Food Facts id ("gluten"). Only parsing makes one, and
/// it maps any source's name, so "Wheat" and "en:gluten" are one value. A name
/// OFF does not list is kept: dropping it would read as "free from".
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

/// Deserialising validates, so a malformed id never reaches a handler.
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

macro_rules! string_id_sql {
    ($t:ty) => {
        $crate::varchar_sql!($t);

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        // `id == "271105"` works; a bare string still cannot stand in for an id.
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
    /// `products.id`.
    ProductId
}

crate::row_id! {
    /// `product_listings.id`.
    ListingId
}
