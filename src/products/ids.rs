//! The identifiers the product domain is keyed on, as types rather than
//! `String`s and `u64`s.
//!
//! Two bug classes motivate this, and both had already happened here in the
//! small:
//!
//! * **A rule written down more than once drifts.** `external_id` is
//!   `[A-Za-z0-9_-]{1,64}`, and that sentence used to be re-implemented at
//!   three separate boundaries (the import route, the shop-sighting report, and
//!   Asda's hit normaliser), each free to loosen independently. It is now one
//!   `FromStr`, and the boundaries call it.
//! * **Same shape, different meaning.** A product id and a listing id are both
//!   `u64`, so passing one where the other belongs type-checks and then writes a
//!   price observation against the wrong row. `ProductId` and `ListingId` are
//!   distinct types precisely so that swap can't compile.
//!
//! Validation lives at construction, so a value of these types is *already*
//! well-formed everywhere downstream: [`Source::listing_url`](super::source::Source::listing_url)
//! and the Open Food Facts client splice them straight into outbound URLs, and
//! the reason that is safe is now the parameter type rather than a comment
//! asking you to trust the caller.
//!
//! `shopping_items.barcode` is deliberately **not** one of these. It is whatever
//! the phone scanned, carried on a synced row for the client's own use — a hint,
//! not a catalog key — and a sync push that fails validation would strand an
//! offline edit. Catalog identity goes through `shopping_items.product_id`.
//!
//! On the frontend each of these becomes a named alias (`type Barcode = string`)
//! rather than a bare `string`/`number`. TypeScript aliases are structural, so
//! that is documentation and not a guarantee — the guarantee is here, on the side
//! that constructs them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

/// A product's EAN/UPC: 1 to 14 ASCII digits, and nothing else.
///
/// The cap and the digits-only rule are what make it safe to splice into the
/// outbound Open Food Facts URL — no path segment or query parameter can hide in
/// a value of this type. (Database lookups are parameterised regardless; this is
/// about the URL.)
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

    /// Trims first: every boundary that used to build one did so from a trimmed
    /// string, and a leading space is a transport artefact rather than a
    /// different barcode.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.len() > 14 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err("barcode must be 1-14 digits".to_string());
        }
        Ok(Barcode(s.to_string()))
    }
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

/// A source-scoped listing id: 1 to 64 characters of `[A-Za-z0-9_-]`.
///
/// Asda's CIN, Waitrose's lineNumber, Open Food Facts' barcode-as-id. Unique
/// only within its own [`Source`](super::source::Source) — the identity of a
/// listing is the *pair*, which is why nothing here tries to be globally unique.
///
/// Same splice guarantee as [`Barcode`]: the character set is the reason
/// `listing_url` can format one into a product-page URL directly.
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

/// Database mapping for the string ids, delegating to `str` — the columns are
/// `VARCHAR`, and `#[derive(sqlx::Type)]` would declare something else (see
/// [`Source`](super::source::Source), where that cost a debugging session).
///
/// Decoding **parses**, so a stored value outside the shape fails the query
/// loudly rather than arriving as a value the rest of the code would have to
/// second-guess.
macro_rules! string_id_sql {
    ($t:ty) => {
        impl sqlx::Type<sqlx::MySql> for $t {
            fn type_info() -> <sqlx::MySql as sqlx::Database>::TypeInfo {
                <str as sqlx::Type<sqlx::MySql>>::type_info()
            }
            fn compatible(ty: &<sqlx::MySql as sqlx::Database>::TypeInfo) -> bool {
                <str as sqlx::Type<sqlx::MySql>>::compatible(ty)
            }
        }

        impl<'q> sqlx::Encode<'q, sqlx::MySql> for $t {
            fn encode_by_ref(
                &self,
                buf: &mut <sqlx::MySql as sqlx::Database>::ArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as sqlx::Encode<'q, sqlx::MySql>>::encode_by_ref(&self.as_str(), buf)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::MySql> for $t {
            fn decode(
                value: <sqlx::MySql as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                <&str as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?
                    .parse()
                    .map_err(Into::into)
            }
        }

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

/// A surrogate key: a row number, distinct from every other kind of row number.
///
/// There is no validation to do — any `u64` the database hands back is a valid
/// id — so the whole point is the *name*. `record_price(pool, listing_id, …)`
/// sits beside a dozen `product_id`-taking functions, and until these were
/// separate types the compiler was happy either way.
macro_rules! row_id {
    ($(#[$m:meta])* $t:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
        #[ts(type = "number")]
        pub struct $t(pub u64);

        impl sqlx::Type<sqlx::MySql> for $t {
            fn type_info() -> <sqlx::MySql as sqlx::Database>::TypeInfo {
                <u64 as sqlx::Type<sqlx::MySql>>::type_info()
            }
            fn compatible(ty: &<sqlx::MySql as sqlx::Database>::TypeInfo) -> bool {
                <u64 as sqlx::Type<sqlx::MySql>>::compatible(ty)
            }
        }

        impl<'q> sqlx::Encode<'q, sqlx::MySql> for $t {
            fn encode_by_ref(
                &self,
                buf: &mut <sqlx::MySql as sqlx::Database>::ArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <u64 as sqlx::Encode<'q, sqlx::MySql>>::encode_by_ref(&self.0, buf)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::MySql> for $t {
            fn decode(
                value: <sqlx::MySql as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                <u64 as sqlx::Decode<'r, sqlx::MySql>>::decode(value).map($t)
            }
        }

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

row_id! {
    /// `products.id` — the canonical product every listing, price, fact and
    /// picture hangs off.
    ProductId
}

/// `last_insert_id()` hands back a bare `u64` — the one place a `ProductId` is
/// minted rather than read back through a typed column.
impl From<u64> for ProductId {
    fn from(id: u64) -> Self {
        ProductId(id)
    }
}

row_id! {
    /// `product_listings.id` — one source's line for a product, and the FK a
    /// price observation is recorded against.
    ListingId
}
