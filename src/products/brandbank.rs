//! Asda's product-page facts: the Brandbank JSON it embeds as `c_BRANDBANK_JSON`,
//! which search lacks. Pure; tested against a captured blob.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::nutrition::{
    Allergen, Basis, Claim, Diet, DietaryFlag, Nutrition, Presence, ProductFacts, allergen_list,
    as_f64,
};

/// The fields we use; serde ignores the rest.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Brandbank {
    /// "per 100ml" / "per 100g".
    calculated_nutrition_per100: Option<String>,
    /// "Per 100g" as sold, or "(Boiled) Per 100g" as prepared.
    calculated_nutrition_per100_used: Option<String>,
    #[serde(default)]
    calculated_nutrition: Vec<CalcNutrient>,
    #[serde(default)]
    taggable_ingredients_text: Vec<String>,
    /// `{ nameValue: "Milk", lookupValue: "Free From" }`.
    #[serde(default)]
    allergy_advice: Vec<AllergyAdvice>,
    /// "Suitable for Vegetarians"; some pages have only these.
    #[serde(default)]
    lifestyle: Vec<Lifestyle>,

    // `true` asserts; `false` is not a "no" (see `dietary`).
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
    per100: Option<Value>,
    per100_used: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lifestyle {
    name_value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllergyAdvice {
    name_value: Option<String>,
    lookup_value: Option<String>,
}

pub fn parse(json: &str) -> Result<ProductFacts> {
    let bb: Brandbank = serde_json::from_str(json).context("parse Brandbank JSON")?;
    Ok(ProductFacts {
        nutrition: bb.nutrition(),
        ingredients: bb.ingredients(),
        allergens: bb.allergens(),
        dietary: bb.dietary(),
    })
}

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

/// "of which" rows are matched before their parent nutrient.
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

/// "Vitamin D (µg)" → "vitamin-d"; stored, never shown.
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
            Some(s) if s.to_lowercase().contains("ml") => Basis::Per100ml,
            _ => Basis::Per100g,
        };
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
        // Values as prepared must never be filed as the product's own.
        let used_as_sold = self
            .calculated_nutrition_per100_used
            .as_deref()
            .is_none_or(|l| !l.contains('('));
        for item in &self.calculated_nutrition {
            let value = item
                .per100
                .as_ref()
                .or(item.per100_used.as_ref().filter(|_| used_as_sold));
            let (Some(name), Some(val)) = (item.name_value.as_deref(), value.and_then(as_f64))
            else {
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
        (!n.is_empty()).then_some(n)
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
        // Only Contains and May Contain; Free From comes in as a dietary flag.
        allergen_list(self.allergy_advice.iter().filter_map(|a| {
            let presence = match a.lookup_value.as_deref()?.to_lowercase().as_str() {
                "contains" => Presence::Contains,
                "may contain" => Presence::MayContain,
                _ => return None,
            };
            Some((a.name_value.as_deref()?.parse().ok()?, presence))
        }))
    }

    fn dietary(&self) -> Vec<DietaryFlag> {
        // `true` or a lifestyle claim asserts 'yes'; `false` is "not tagged",
        // never a firm 'no'.
        let says = |claim: &str| {
            self.lifestyle.iter().any(|l| {
                l.name_value
                    .as_deref()
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case(claim))
            })
        };
        let mapped = [
            (self.vegan || says("Suitable for Vegans"), Diet::Vegan),
            (
                self.vegetarian || says("Suitable for Vegetarians"),
                Diet::Vegetarian,
            ),
            (self.halal, Diet::Halal),
            (self.kosher, Diet::Kosher),
            (self.no_gluten, Diet::GlutenFree),
            (self.no_lactose, Diet::LactoseFree),
            (self.no_milk, Diet::MilkFree),
            (self.no_nuts, Diet::NutFree),
            (self.no_egg, Diet::EggFree),
            (self.no_soya, Diet::SoyaFree),
        ];
        let mut flags: Vec<DietaryFlag> = mapped
            .iter()
            .filter(|(claimed, _)| *claimed)
            .map(|(_, flag)| DietaryFlag {
                flag: *flag,
                value: Claim::Yes,
            })
            .collect();
        flags.sort_by_key(|f| f.flag);
        flags
    }
}
