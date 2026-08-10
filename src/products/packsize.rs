//! Reading a shop's pack label as an amount you can do arithmetic with.
//!
//! A pack size arrives as whatever the shop typed: `950g`, `250ML`, `33 cl`,
//! `35 grammes`, `22x27G`, `EACH`. That string is the right thing to *show* — it
//! is what is printed on the tub — and the wrong thing to compute with. You
//! cannot ask how much of a 950g tub is left when all you hold is `"950g"`, so
//! stock linked to a product has had no quantity to start from and has gone
//! untracked.
//!
//! **One canonical unit per dimension, decided here.** Everything mass becomes
//! grams and everything volume becomes millilitres at this boundary, so nothing
//! downstream needs a conversion table: two parsed packs of the same dimension
//! subtract from each other directly. That is deliberately the opposite of
//! [[crate::inventory::consume]], which compares units and never converts them —
//! that rule is about two amounts a *person* typed, where `kg` against `g` is a
//! disagreement worth surfacing rather than papering over. This is about one
//! string a shop typed, where there is nothing to disagree with.
//!
//! **It refuses rather than guesses.** An unrecognised label parses to `None`,
//! and the raw string goes on being displayed exactly as it is today — nothing
//! is lost by refusing. A wrong unit in a cupboard is worse than an absent one:
//! absent shows as "not tracked", wrong shows as a number you would believe. So
//! `oz` is not in the table, because a grocery `oz` is usually mass and
//! sometimes fluid and the label does not say which; nor are `gr` and `ltr`,
//! which are guesses at what an abbreviation meant. Symbols and spelled-out
//! words only.

use serde::Serialize;
use ts_rs::TS;

/// The unit a parsed pack size is expressed in — one per dimension, so two
/// packs in the same dimension are always directly comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum PackUnit {
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "ml")]
    Millilitre,
    /// A number of things rather than an amount of anything — Asda's `EACH`.
    #[serde(rename = "count")]
    Count,
}

/// What a pack holds, read off its label.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct PackSize {
    /// How much, measured in `unit`. Always finite and greater than zero: a
    /// pack of none of something is not a reading, it is a parse that went
    /// wrong, and this reports that as `None` instead.
    pub value: f64,
    pub unit: PackUnit,
}

/// Read a shop's pack label, or `None` when there is no amount in it we are
/// sure of. See the module docs for why refusing is the safe direction.
pub fn parse(label: &str) -> Option<PackSize> {
    let text = label.trim().to_lowercase();
    // A pack sold by the item says so instead of measuring itself. It is still
    // an amount — one of them — which is what makes it worth reading.
    if text == "each" || text == "ea" {
        return Some(PackSize {
            value: 1.0,
            unit: PackUnit::Count,
        });
    }
    // A multipack states its size twice over — "22x27g" is 22 sachets of 27g —
    // and what is in the cupboard is the product of the two.
    let (multiplier, rest) = split_multipack(&text);
    let (amount, word) = split_amount(rest)?;
    let (per, unit) = unit_of(word)?;
    let value = multiplier * amount * per;
    (value.is_finite() && value > 0.0).then_some(PackSize { value, unit })
}

/// `"22x27g"` → `(22.0, "27g")`. Anything that is not a leading count and a
/// separator → `(1.0, the whole thing)`, so a stray `x` inside a word (`"box of
/// 6"`) falls through to the ordinary path rather than splitting it.
fn split_multipack(text: &str) -> (f64, &str) {
    for separator in ['x', '×'] {
        let Some((count, rest)) = text.split_once(separator) else {
            continue;
        };
        let Ok(n) = count.trim().parse::<f64>() else {
            continue;
        };
        if n.is_finite() && n > 0.0 && !rest.trim().is_empty() {
            return (n, rest);
        }
    }
    (1.0, text)
}

/// Split `"27g"` / `"33 cl"` / `"750 grams"` into its number and its unit word.
/// The number must come first and the unit must be all that follows it — a
/// label with anything else in it is one we do not understand.
fn split_amount(text: &str) -> Option<(f64, &str)> {
    let text = text.trim();
    let end = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, word) = text.split_at(end);
    Some((number.parse().ok()?, word.trim()))
}

/// How much of the canonical unit one of `word` is, and which unit that is.
///
/// Non-exhaustive by design: a word that is not here is one this refuses, and
/// adding to the table is how a newly-seen spelling is supported — never by
/// falling back to a default, which would put an invented dimension on a pack.
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
