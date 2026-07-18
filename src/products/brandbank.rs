//! Parse Asda's Brandbank product-content JSON into our domain facts.
//!
//! Asda's product page embeds a `c_BRANDBANK_JSON` blob (Brandbank is the UK
//! grocery industry's standard product-content feed): the full nutrition panel,
//! ingredients, allergen advice and a raft of dietary/free-from booleans — none
//! of which the storefront SEARCH API carries (see products::asda). The hidden
//! WebView returns that blob verbatim (the page is behind Cloudflare, so a plain
//! server fetch can't reach it); this module turns it into `ProductFacts`, stored
//! per source and merged with Open Food Facts on read (see repo::facts_for).
//!
//! Pure — no I/O — so it's exercised directly against a captured real blob.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::nutrition::{Allergen, DietaryFlag, Nutrition, ProductFacts};

/// The Brandbank fields we consume. Unknown fields (the bulk of the blob —
/// company address, marketing copy, packaging, …) are ignored by serde.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Brandbank {
    /// e.g. "per 100ml" / "per 100g" — the basis of `calculated_nutrition`.
    calculated_nutrition_per100: Option<String>,
    /// The per-100 panel: each `{ nameValue: "Energy (kcal)", per100: 61 }`.
    #[serde(default)]
    calculated_nutrition: Vec<CalcNutrient>,
    /// The ingredients, already split into a list of components.
    #[serde(default)]
    taggable_ingredients_text: Vec<String>,
    /// Per-allergen advice: `{ nameValue: "Milk", lookupValue: "Free From" }`.
    #[serde(default)]
    allergy_advice: Vec<AllergyAdvice>,

    // Dietary / free-from booleans. `true` = the manufacturer asserts it; a
    // `false` is NOT read as a firm "no" (see `dietary`).
    #[serde(default)]
    vegan: bool,
    #[serde(default)]
    vegetarian: bool,
    #[serde(default)]
    halal: bool,
    #[serde(default)]
    kosher: bool,
    #[serde(default)]
    no_gluten: bool,
    #[serde(default)]
    no_lactose: bool,
    #[serde(default)]
    no_milk: bool,
    #[serde(default)]
    no_nuts: bool,
    #[serde(default)]
    no_egg: bool,
    #[serde(default)]
    no_soya: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalcNutrient {
    name_value: Option<String>,
    /// A JSON number, or occasionally a numeric string.
    per100: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllergyAdvice {
    name_value: Option<String>,
    lookup_value: Option<String>,
}

/// Parse a raw `c_BRANDBANK_JSON` blob into product facts.
pub fn parse(json: &str) -> Result<ProductFacts> {
    let bb: Brandbank = serde_json::from_str(json).context("parse Brandbank JSON")?;
    Ok(ProductFacts {
        nutrition: bb.nutrition(),
        ingredients: bb.ingredients(),
        allergens: bb.allergens(),
        dietary: bb.dietary(),
    })
}

/// Which big-8 column a Brandbank nutrient name maps to, or `Extra` for the tail.
enum Slot {
    EnergyKj,
    EnergyKcal,
    Fat,
    Saturates,
    Carbohydrate,
    Sugars,
    Fibre,
    Protein,
    Salt,
    Extra,
}

/// Map a Brandbank nutrient label ("Energy (kcal)", "of which sugars (g)", …) to
/// its panel slot. The "of which" rows are checked before the parent nutrient so
/// they don't fall into fat/carbohydrate.
fn classify(name: &str) -> Slot {
    let l = name.to_lowercase();
    if l.contains("energy") && l.contains("kcal") {
        Slot::EnergyKcal
    } else if l.contains("energy") && l.contains("kj") {
        Slot::EnergyKj
    } else if l.contains("of which saturates") {
        Slot::Saturates
    } else if l.contains("of which sugars") {
        Slot::Sugars
    } else if l.starts_with("fat") {
        Slot::Fat
    } else if l.contains("carbohydrate") {
        Slot::Carbohydrate
    } else if l.contains("fibre") || l.contains("fiber") {
        Slot::Fibre
    } else if l.contains("protein") {
        Slot::Protein
    } else if l.contains("salt") {
        Slot::Salt
    } else {
        Slot::Extra
    }
}

/// A per100 value as a number, whether it arrived as a JSON number or a numeric
/// string (Brandbank is mostly numbers, but be tolerant). `None` otherwise.
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A tail-nutrient key: the label without its "(unit)" suffix, lowercased with
/// spaces hyphenated ("Vitamin D (µg)" → "vitamin-d"). Only stored, not shown.
fn extra_key(name: &str) -> String {
    name.split('(')
        .next()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
        .replace(' ', "-")
}

impl Brandbank {
    fn nutrition(&self) -> Option<Nutrition> {
        let basis = match self.calculated_nutrition_per100.as_deref() {
            Some(s) if s.to_lowercase().contains("ml") => "100ml",
            _ => "100g",
        }
        .to_string();
        let mut n = Nutrition {
            basis,
            serving_size: None,
            energy_kj: None,
            energy_kcal: None,
            fat_g: None,
            saturates_g: None,
            carbohydrate_g: None,
            sugars_g: None,
            fibre_g: None,
            protein_g: None,
            salt_g: None,
            extra: BTreeMap::new(),
        };
        for item in &self.calculated_nutrition {
            let (Some(name), Some(val)) = (
                item.name_value.as_deref(),
                item.per100.as_ref().and_then(as_f64),
            ) else {
                continue;
            };
            match classify(name) {
                Slot::EnergyKj => n.energy_kj = Some(val),
                Slot::EnergyKcal => n.energy_kcal = Some(val),
                Slot::Fat => n.fat_g = Some(val),
                Slot::Saturates => n.saturates_g = Some(val),
                Slot::Carbohydrate => n.carbohydrate_g = Some(val),
                Slot::Sugars => n.sugars_g = Some(val),
                Slot::Fibre => n.fibre_g = Some(val),
                Slot::Protein => n.protein_g = Some(val),
                Slot::Salt => n.salt_g = Some(val),
                Slot::Extra => {
                    n.extra.insert(extra_key(name), val);
                }
            }
        }
        // A panel with no numbers and no tail is no panel.
        let empty = n.extra.is_empty()
            && [
                n.energy_kj,
                n.energy_kcal,
                n.fat_g,
                n.saturates_g,
                n.carbohydrate_g,
                n.sugars_g,
                n.fibre_g,
                n.protein_g,
                n.salt_g,
            ]
            .iter()
            .all(Option::is_none);
        (!empty).then_some(n)
    }

    fn ingredients(&self) -> Option<String> {
        let joined = self
            .taggable_ingredients_text
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        (!joined.is_empty()).then_some(joined)
    }

    fn allergens(&self) -> Vec<Allergen> {
        // Only positive presences become allergens: "Contains" / "May Contain".
        // A "Free From" entry is a negative — it's not an allergen (the matching
        // free-from *dietary* flag comes from the booleans instead).
        let mut out: Vec<Allergen> = self
            .allergy_advice
            .iter()
            .filter_map(|a| {
                let name = a.name_value.as_deref()?.trim();
                let presence = match a.lookup_value.as_deref()?.to_lowercase().as_str() {
                    "contains" => "contains",
                    "may contain" => "may_contain",
                    _ => return None,
                };
                (!name.is_empty()).then(|| Allergen {
                    allergen: name.to_lowercase(),
                    presence: presence.to_string(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.allergen.cmp(&b.allergen));
        out
    }

    fn dietary(&self) -> Vec<DietaryFlag> {
        // Brandbank booleans map to our dietary slugs (aligned with OFF's and the
        // Asda search tags so all sources merge). `true` = an asserted claim →
        // 'yes'. A `false` asserts NOTHING: it is "not claimed", not "not the
        // case" — the same caution the search tags carry — so we never emit a
        // firm 'no' from it (that would risk telling someone a product isn't
        // vegan/gluten-free when Brandbank simply hadn't tagged it).
        let mapped = [
            (self.vegan, "vegan"),
            (self.vegetarian, "vegetarian"),
            (self.halal, "halal"),
            (self.kosher, "kosher"),
            (self.no_gluten, "gluten_free"),
            (self.no_lactose, "lactose_free"),
            (self.no_milk, "milk_free"),
            (self.no_nuts, "nut_free"),
            (self.no_egg, "egg_free"),
            (self.no_soya, "soya_free"),
        ];
        let mut flags: Vec<DietaryFlag> = mapped
            .iter()
            .filter(|(claimed, _)| *claimed)
            .map(|(_, slug)| DietaryFlag {
                flag: slug.to_string(),
                value: "yes".to_string(),
            })
            .collect();
        flags.sort_by(|a, b| a.flag.cmp(&b.flag));
        flags
    }
}
