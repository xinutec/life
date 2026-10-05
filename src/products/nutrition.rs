//! Product facts. `RawFacts::parse` reads Open Food Facts (`brandbank` reads
//! Asda's); the UK panel is stored a field each, OFF's long tail in `extra`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use super::ids::AllergenId;
use super::source::Source;
use crate::str_enum;

str_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[ts(export)]
    pub enum Basis: "nutrition basis" {
        /// Solids.
        #[serde(rename = "100g")]
        Per100g => "100g",
        /// Liquids.
        #[serde(rename = "100ml")]
        Per100ml => "100ml",
    }
}

/// Every figure optional; all `None` with an empty `extra` is no panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Nutrition {
    pub basis: Basis,
    /// Verbatim, e.g. "40g".
    pub serving_size: Option<String>,
    pub energy_kj: Option<f64>,
    pub energy_kcal: Option<f64>,
    pub fat_g: Option<f64>,
    pub saturates_g: Option<f64>,
    pub carbohydrate_g: Option<f64>,
    pub sugars_g: Option<f64>,
    pub fibre_g: Option<f64>,
    pub protein_g: Option<f64>,
    pub salt_g: Option<f64>,
    /// Other nutriments by OFF's name, `_100g` stripped; never the promoted ones.
    #[ts(type = "Record<string, number>")]
    pub extra: BTreeMap<String, f64>,
}

str_enum! {
    /// Ordered by severity, so "the more severe claim wins" is a `max`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum Presence: "allergen presence" {
    /// Possible cross-contamination.
        MayContain => "may_contain",
        Contains => "contains",
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Allergen {
    pub allergen: AllergenId,
    pub presence: Presence,
}

str_enum! {
    /// Tri-state: `Maybe` reports a disagreement, as over-claiming is the
    /// harmful direction (see `merge_dietary`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum Claim: "dietary claim" {
        Yes => "yes",
        No => "no",
        Maybe => "maybe",
    }
}
str_enum! {
    /// A dietary or free-from claim; every source's vocabulary maps onto it. In
    /// slug order, which the derived `Ord` uses to sort.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
    #[serde(rename_all = "snake_case")]
    #[ts(export)]
    pub enum Diet: "dietary flag" {
        EggFree => "egg_free",
        FairTrade => "fair_trade",
        GlutenFree => "gluten_free",
        Halal => "halal",
        Kosher => "kosher",
        LactoseFree => "lactose_free",
        MilkFree => "milk_free",
        NutFree => "nut_free",
        Organic => "organic",
        PalmOilFree => "palm_oil_free",
        SoyaFree => "soya_free",
        Vegan => "vegan",
        Vegetarian => "vegetarian",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DietaryFlag {
    pub flag: Diet,
    pub value: Claim,
}

/// Everything known about a product beyond its identity.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductFacts {
    pub nutrition: Option<Nutrition>,
    pub ingredients: Option<String>,
    pub allergens: Vec<Allergen>,
    pub dietary: Vec<DietaryFlag>,
}

/// OFF's fact fields, flattened into `off::Raw` so one fetch yields both.
#[derive(Debug, Default, Deserialize)]
pub struct RawFacts {
    #[serde(default)]
    nutriments: Map<String, Value>,
    nutrition_data_per: Option<String>,
    serving_size: Option<String>,
    ingredients_text: Option<String>,
    ingredients_text_en: Option<String>,
    #[serde(default)]
    allergens_tags: Vec<String>,
    #[serde(default)]
    traces_tags: Vec<String>,
    #[serde(default)]
    ingredients_analysis_tags: Vec<String>,
    #[serde(default)]
    labels_tags: Vec<String>,
}

/// Promoted to columns, so `extra` leaves them out. The unit-ambiguous `energy`
/// is dropped; the kJ and kcal keys are kept.
const PROMOTED: &[&str] = &[
    "energy",
    "energy-kj",
    "energy-kcal",
    "fat",
    "saturated-fat",
    "carbohydrates",
    "sugars",
    "fiber",
    "proteins",
    "salt",
];

/// Manufacturer labels: always "yes".
const LABEL_FLAGS: &[(&str, Diet)] = &[
    ("gluten-free", Diet::GlutenFree),
    ("lactose-free", Diet::LactoseFree),
    ("organic", Diet::Organic),
    ("kosher", Diet::Kosher),
    ("halal", Diet::Halal),
    ("vegan", Diet::Vegan),
    ("vegetarian", Diet::Vegetarian),
    ("fair-trade", Diet::FairTrade),
    ("palm-oil-free", Diet::PalmOilFree),
];

impl Nutrition {
    /// No numbers and no tail is no panel.
    pub fn is_empty(&self) -> bool {
        self.extra.is_empty()
            && [
                self.energy_kj,
                self.energy_kcal,
                self.fat_g,
                self.saturates_g,
                self.carbohydrate_g,
                self.sugars_g,
                self.fibre_g,
                self.protein_g,
                self.salt_g,
            ]
            .iter()
            .all(Option::is_none)
    }
}

/// One tri-state per flag: agreement wins, a firm claim beats a soft one, and
/// 'yes' against 'no' is 'maybe', as calling a thing vegan against a source is
/// the harmful error.
pub fn merge_dietary(claims: Vec<DietaryFlag>) -> Vec<DietaryFlag> {
    let mut by_flag: BTreeMap<Diet, Vec<Claim>> = BTreeMap::new();
    for c in claims {
        by_flag.entry(c.flag).or_default().push(c.value);
    }
    by_flag
        .into_iter()
        .map(|(flag, values)| {
            let yes = values.contains(&Claim::Yes);
            let no = values.contains(&Claim::No);
            let value = match (yes, no) {
                (true, true) => Claim::Maybe, // a real conflict — don't take a side
                (true, false) => Claim::Yes,
                (false, true) => Claim::No,
                (false, false) => Claim::Maybe,
            };
            DietaryFlag { flag, value }
        })
        .collect()
}

/// One source's panel whole, by precedence: blending would invent numbers.
pub fn merge_nutrition(panels: Vec<(Source, Nutrition)>) -> Option<Nutrition> {
    panels
        .into_iter()
        .min_by_key(|(source, _)| fact_rank(*source))
        .map(|(_, n)| n)
}

/// One source's text whole, by precedence.
pub fn merge_ingredients(texts: Vec<(Source, String)>) -> Option<String> {
    texts
        .into_iter()
        .filter(|(_, t)| !t.trim().is_empty())
        .min_by_key(|(source, _)| fact_rank(*source))
        .map(|(_, t)| t)
}

/// A union: silence is not "free from".
pub fn merge_allergens(claims: Vec<(Source, Allergen)>) -> Vec<Allergen> {
    allergen_list(claims.into_iter().map(|(_, a)| (a.allergen, a.presence)))
}

/// One entry per allergen, `contains` beating `may_contain`: "Wheat" and
/// "Barley" are both gluten.
pub fn allergen_list(claims: impl IntoIterator<Item = (AllergenId, Presence)>) -> Vec<Allergen> {
    let mut by_id: BTreeMap<AllergenId, Presence> = BTreeMap::new();
    for (id, presence) in claims {
        by_id
            .entry(id)
            .and_modify(|p| *p = (*p).max(presence))
            .or_insert(presence);
    }
    by_id
        .into_iter()
        .map(|(allergen, presence)| Allergen { allergen, presence })
        .collect()
}

/// Retailer over crowd, as for names; an unlisted source only fills a gap.
pub fn fact_rank(source: Source) -> usize {
    source.name_rank().unwrap_or(usize::MAX)
}

/// A panel in one line, for a reconcile candidate.
pub fn summarize_nutrition(n: &Nutrition) -> String {
    let mut parts = Vec::new();
    if let Some(kcal) = n.energy_kcal {
        parts.push(format!("{} kcal", trim_num(kcal)));
    } else if let Some(kj) = n.energy_kj {
        parts.push(format!("{} kJ", trim_num(kj)));
    }
    for (label, value) in [
        ("fat", n.fat_g),
        ("sugars", n.sugars_g),
        ("protein", n.protein_g),
        ("salt", n.salt_g),
    ] {
        if let Some(v) = value {
            parts.push(format!("{label} {}g", trim_num(v)));
        }
    }
    let head = if parts.is_empty() {
        "panel".to_string()
    } else {
        parts.join(" · ")
    };
    format!("{head} (per {})", n.basis)
}

/// 3.0 → "3", 3.4 → "3.4".
fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        // Formatted, not cast: `as i64` would truncate.
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// A number or a numeric string, as OFF and Asda mix both.
pub(crate) fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// "en:milk" → "milk".
fn strip_lang(tag: &str) -> &str {
    tag.split_once(':').map(|(_, rest)| rest).unwrap_or(tag)
}

impl RawFacts {
    pub fn parse(&self) -> ProductFacts {
        ProductFacts {
            nutrition: self.nutrition(),
            ingredients: self.ingredients(),
            allergens: self.allergens(),
            dietary: self.dietary(),
        }
    }

    fn num(&self, key: &str) -> Option<f64> {
        self.nutriments.get(key).and_then(as_f64)
    }

    fn nutrition(&self) -> Option<Nutrition> {
        // OFF says `_100g` even for liquids; `basis` records which it is.
        let basis = match self.nutrition_data_per.as_deref() {
            Some("100ml") => Basis::Per100ml,
            _ => Basis::Per100g,
        };
        let mut extra = BTreeMap::new();
        for (key, value) in &self.nutriments {
            let Some(name) = key.strip_suffix("_100g") else {
                continue;
            };
            if PROMOTED.contains(&name) {
                continue;
            }
            if let Some(n) = as_f64(value) {
                extra.insert(name.to_string(), n);
            }
        }
        let n = Nutrition {
            basis,
            serving_size: non_empty(self.serving_size.as_deref()),
            energy_kj: self.num("energy-kj_100g"),
            energy_kcal: self.num("energy-kcal_100g"),
            fat_g: self.num("fat_100g"),
            saturates_g: self.num("saturated-fat_100g"),
            carbohydrate_g: self.num("carbohydrates_100g"),
            sugars_g: self.num("sugars_100g"),
            fibre_g: self.num("fiber_100g"),
            protein_g: self.num("proteins_100g"),
            salt_g: self.num("salt_100g"),
            extra,
        };
        (!n.is_empty()).then_some(n)
    }

    fn ingredients(&self) -> Option<String> {
        non_empty(self.ingredients_text_en.as_deref())
            .or_else(|| non_empty(self.ingredients_text.as_deref()))
    }

    fn allergens(&self) -> Vec<Allergen> {
        let tagged = |tags: &[String], presence| {
            tags.iter()
                .filter_map(move |t| Some((t.parse::<AllergenId>().ok()?, presence)))
                .collect::<Vec<_>>()
        };
        allergen_list(
            tagged(&self.traces_tags, Presence::MayContain)
                .into_iter()
                .chain(tagged(&self.allergens_tags, Presence::Contains)),
        )
    }

    fn dietary(&self) -> Vec<DietaryFlag> {
        let mut flags: BTreeMap<Diet, Claim> = BTreeMap::new();
        // OFF's analysis is tri-state.
        for tag in &self.ingredients_analysis_tags {
            if let Some((flag, value)) = analysis_flag(strip_lang(tag)) {
                flags.insert(flag, value);
            }
        }
        // A label is a firm claim and overrides the analysis.
        for tag in &self.labels_tags {
            let stripped = strip_lang(tag);
            if let Some((_, flag)) = LABEL_FLAGS.iter().find(|(label, _)| *label == stripped) {
                flags.insert(*flag, Claim::Yes);
            }
        }
        flags
            .into_iter()
            .map(|(flag, value)| DietaryFlag { flag, value })
            .collect()
    }
}

/// `None` for a tag that asserts nothing.
fn analysis_flag(tag: &str) -> Option<(Diet, Claim)> {
    Some(match tag {
        "vegan" => (Diet::Vegan, Claim::Yes),
        "non-vegan" => (Diet::Vegan, Claim::No),
        "maybe-vegan" => (Diet::Vegan, Claim::Maybe),
        "vegetarian" => (Diet::Vegetarian, Claim::Yes),
        "non-vegetarian" => (Diet::Vegetarian, Claim::No),
        "maybe-vegetarian" => (Diet::Vegetarian, Claim::Maybe),
        "palm-oil-free" => (Diet::PalmOilFree, Claim::Yes),
        "palm-oil" => (Diet::PalmOilFree, Claim::No),
        "may-contain-palm-oil" => (Diet::PalmOilFree, Claim::Maybe),
        _ => return None,
    })
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}
