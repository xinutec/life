//! Price observations: what a shop charged for a listing, when. Money is always
//! integer minor units (pence) on the wire and in the DB — never a float.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::ExternalId;
use super::source::Source;

crate::str_enum! {
    /// What a per-unit price is quoted per: the scale shops print ("£8.00/KG").
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[ts(export)]
    pub enum UnitMeasure: "unit measure" {
        #[serde(rename = "KG")]
        Kg => "KG",
        #[serde(rename = "L")]
        Litre => "L",
        #[serde(rename = "each")]
        Each => "each",
    }
}

impl UnitMeasure {
    /// A shop's own spelling ("KG", "LT", "EA"), or `None` for one we don't
    /// know rather than a guess.
    pub fn from_shop(label: &str) -> Option<Self> {
        match label.trim().to_ascii_uppercase().as_str() {
            "KG" => Some(Self::Kg),
            "L" | "LT" | "LTR" | "LITRE" => Some(Self::Litre),
            "EA" | "EACH" => Some(Self::Each),
            _ => None,
        }
    }
}

/// A price per unit of measure, for comparing packs: 892 per KG. The two halves
/// travel together because either alone says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnitPrice {
    /// Minor units per `measure`.
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub measure: UnitMeasure,
}

/// An ISO 4217 code, checked once where it arrives: three letters, upper case.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, TS)]
#[ts(export)]
pub struct Currency(String);
crate::varchar_sql!(Currency);

impl Currency {
    pub fn gbp() -> Self {
        Self("GBP".into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Currency {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let code = s.trim().to_ascii_uppercase();
        if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
            Ok(Self(code))
        } else {
            Err(format!(
                "currency must be a 3-letter ISO 4217 code, got {s:?}"
            ))
        }
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// A price a source reported for a listing. The client sends this on import
/// (derived from an Asda hit or a Waitrose product); the backend appends it to
/// the listing's price history.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PriceInput {
    /// Shelf price in minor units (pence for GBP).
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
    /// For fair cross-pack comparison, when the shop quotes one.
    pub unit_price: Option<UnitPrice>,
}

/// What one shop currently charges for a product — the `prices` part of the
/// product detail (GET /api/products/id/{id}), cheapest shop first.
///
/// Exactly one row per source: a shop can list the same physical product twice
/// (two Asda CINs sharing an EAN), and "where do I buy this, for how much" wants
/// one answer per shop — the cheapest. `external_id` names the listing that
/// quoted this price, so the shop link goes to the item actually being quoted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ShopPrice {
    /// The listing's source. Unique within a response.
    pub source: Source,
    /// Source-scoped id of the listing this price came from.
    pub external_id: ExternalId,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
    pub unit_price: Option<UnitPrice>,
    /// When observed; Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub observed_at: DateTime<Utc>,
}
