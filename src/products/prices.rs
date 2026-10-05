//! What a shop charged for a listing, and when; always integer minor units.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::ExternalId;
use super::source::Source;

crate::str_enum! {
    /// What a per-unit price is per ("£8.00/KG").
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
    /// `None` for a spelling we do not know.
    pub fn from_shop(label: &str) -> Option<Self> {
        match label.trim().to_ascii_uppercase().as_str() {
            "KG" => Some(Self::Kg),
            "L" | "LT" | "LTR" | "LITRE" => Some(Self::Litre),
            "EA" | "EACH" => Some(Self::Each),
            _ => None,
        }
    }
}

/// 892 per KG; either half alone says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnitPrice {
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub measure: UnitMeasure,
}

/// Three upper-case letters, checked where it arrives.
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

/// A price a shop quoted, appended to the listing's history.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct PriceInput {
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
    pub unit_price: Option<UnitPrice>,
}

/// What one shop charges now: its cheapest listing, as a shop can list one
/// product twice. `external_id` names that listing, so the link goes to it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ShopPrice {
    pub source: Source,
    pub external_id: ExternalId,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub currency: Currency,
    pub unit_price: Option<UnitPrice>,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub observed_at: DateTime<Utc>,
}
