//! A pack label (`950g`, `33 cl`, `22x27G`, `EACH`) as an amount in grams,
//! millilitres or a count. Unrecognised labels are `None`, not a guess.

use serde::Serialize;
use ts_rs::TS;

/// One per dimension, so packs compare directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum PackUnit {
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "ml")]
    Millilitre,
    /// Asda's `EACH`.
    #[serde(rename = "count")]
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PackSize {
    /// Finite and positive; anything else parses as `None`.
    pub value: f64,
    pub unit: PackUnit,
}

pub fn parse(label: &str) -> Option<PackSize> {
    let text = label.trim().to_lowercase();
    if text == "each" || text == "ea" {
        return Some(PackSize {
            value: 1.0,
            unit: PackUnit::Count,
        });
    }
    // "22x27g" is 22 sachets of 27g.
    let (multiplier, rest) = split_multipack(&text);
    let (amount, word) = split_amount(rest)?;
    let (per, unit) = unit_of(word)?;
    let value = multiplier * amount * per;
    (value.is_finite() && value > 0.0).then_some(PackSize { value, unit })
}

/// `"22x27g"` → `(22.0, "27g")`; anything else → `(1.0, whole)`, so `"box of 6"`
/// is not split.
fn split_multipack(text: &str) -> (f64, &str) {
    for separator in ['x', '×'] {
        if let Some((count, rest)) = text.split_once(separator)
            && let Ok(n) = count.trim().parse::<f64>()
        {
            return (n, rest);
        }
    }
    (1.0, text)
}

/// The number first, and nothing after it but the unit.
fn split_amount(text: &str) -> Option<(f64, &str)> {
    let text = text.trim();
    let end = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, word) = text.split_at(end);
    Some((number.parse().ok()?, word.trim()))
}

/// Non-exhaustive by design: a new spelling is added here, never defaulted.
fn unit_of(word: &str) -> Option<(f64, PackUnit)> {
    use PackUnit::{Gram, Millilitre};
    Some(match word {
        "g" | "gm" | "gram" | "grams" | "gramme" | "grammes" => (1.0, Gram),
        "mg" | "milligram" | "milligrams" => (0.001, Gram),
        "kg" | "kilo" | "kilos" | "kilogram" | "kilograms" | "kilogramme" | "kilogrammes" => {
            (1000.0, Gram)
        }
        "ml" | "millilitre" | "millilitres" | "milliliter" | "milliliters" => (1.0, Millilitre),
        "cl" | "centilitre" | "centilitres" => (10.0, Millilitre),
        "dl" | "decilitre" | "decilitres" => (100.0, Millilitre),
        "l" | "litre" | "litres" | "liter" | "liters" => (1000.0, Millilitre),
        _ => return None,
    })
}
