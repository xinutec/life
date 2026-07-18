//! Product facts: the nutrition panel, ingredients text, allergens, and dietary
//! flags. These describe the physical product, so they attach to the canonical
//! `products` row (see repo), reconciled by barcode like every other enrichment.
//!
//! Open Food Facts is the source. The pure `RawFacts::parse` turns an OFF product
//! JSON into our domain shapes — no I/O, so it's exercised directly from tests
//! against captured OFF responses (`off` does the fetch, `repo` the persistence).
//!
//! Nutrition is stored WIDE: the UK mandatory panel (the "big 8") is a fixed
//! small set, one field each; OFF's long tail keeps its structure in `extra`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// The nutrition panel, per `basis`. Every figure is optional — a source declares
/// whatever it has. `None` throughout + empty `extra` means "no panel".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Nutrition {
    /// What the figures are per: "100g" (solids) or "100ml" (liquids).
    pub basis: String,
    /// The manufacturer's serving description, verbatim (e.g. "40g").
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
    /// Other per-`basis` nutriments (sodium, vitamins, …), keyed by OFF's name
    /// with the `_100g` suffix stripped. The promoted big-8 keys are excluded.
    #[ts(type = "Record<string, number>")]
    pub extra: BTreeMap<String, f64>,
}

/// One allergen and how it's present in a product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Allergen {
    /// OFF's canonical allergen id, language prefix stripped ("en:milk" → "milk").
    pub allergen: String,
    /// "contains" (declared ingredient) or "may_contain" (trace).
    pub presence: String,
}

/// One dietary flag and its assertion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DietaryFlag {
    /// Stable slug: "vegan", "vegetarian", "gluten_free", "organic", ….
    pub flag: String,
    /// "yes" | "no" | "maybe" — tri-state so we never over-claim.
    pub value: String,
}

/// Everything we know about a product beyond its identity — the `facts` part
/// of the product detail (GET /api/products/id/{id}).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ProductFacts {
    pub nutrition: Option<Nutrition>,
    pub ingredients: Option<String>,
    pub allergens: Vec<Allergen>,
    pub dietary: Vec<DietaryFlag>,
}

// --- OFF wire shapes (only the fact fields; flattened into off::Raw) ---

/// The fact-bearing fields of an OFF product. Flattened into `off::Raw`, so the
/// single OFF fetch yields both the basic metadata and these.
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

/// The OFF nutriment keys we promote to columns (with the `_100g` suffix), so the
/// `extra` tail excludes them — no duplication. `energy` (the unit-ambiguous
/// combined key) is dropped too; we keep only the explicit kJ/kcal splits.
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

/// The label tags we recognise as dietary claims, mapped to our flag slug. A
/// label is a manufacturer assertion → always "yes".
const LABEL_FLAGS: &[(&str, &str)] = &[
    ("gluten-free", "gluten_free"),
    ("lactose-free", "lactose_free"),
    ("organic", "organic"),
    ("kosher", "kosher"),
    ("halal", "halal"),
    ("vegan", "vegan"),
    ("vegetarian", "vegetarian"),
    ("fair-trade", "fair_trade"),
    ("palm-oil-free", "palm_oil_free"),
];

/// Reconcile every source's claims about each dietary flag into one answer.
///
/// Sources overlap: Open Food Facts derives flags from a crowd-entered
/// ingredient list, while a retailer tags the product it actually sells (see
/// products::asda). Both are stored, per source (migration 0028), and this
/// decides what the product page shows — in the tri-state the flags already
/// speak:
/// - sources agree → that value;
/// - a firm claim beats a soft guess ('yes' over 'maybe') — a retailer tagging
///   its own product Vegan settles OFF's "maybe-vegan" analysis;
/// - **'yes' against 'no' → 'maybe'.** They genuinely disagree, so say so rather
///   than pick a winner. Over-claiming is the harmful direction: telling someone
///   avoiding animal products that a thing is vegan when a source says otherwise
///   is a real-world error, where "we're not sure" merely sends them to the
///   label. This is why `value` is tri-state at all.
///
/// Input may hold repeated flags (one per source, in any order); the result has
/// one entry per flag, sorted. Pure — the unit under test.
pub fn merge_dietary(claims: Vec<DietaryFlag>) -> Vec<DietaryFlag> {
    let mut by_flag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in claims {
        by_flag.entry(c.flag).or_default().push(c.value);
    }
    by_flag
        .into_iter()
        .map(|(flag, values)| {
            let yes = values.iter().any(|v| v == "yes");
            let no = values.iter().any(|v| v == "no");
            let value = match (yes, no) {
                (true, true) => "maybe", // a real conflict — don't take a side
                (true, false) => "yes",
                (false, true) => "no",
                (false, false) => "maybe",
            };
            DietaryFlag {
                flag,
                value: value.to_string(),
            }
        })
        .collect()
}

/// Merge several sources' nutrition panels into the one to show.
///
/// Panels aren't blendable — averaging two sources' "fat per 100g" would invent a
/// number neither reported (false precision). So we pick ONE panel whole, by
/// source precedence (see products::source — retailers' manufacturer-grade data
/// over the crowd's), and show it verbatim. Where sources genuinely differ, the
/// reconciliation UI surfaces it; this is only the default display pick.
///
/// Input is `(source, panel)` pairs in any order; a source not in the precedence
/// list sorts last. Pure — the unit under test.
pub fn merge_nutrition(panels: Vec<(String, Nutrition)>) -> Option<Nutrition> {
    panels
        .into_iter()
        .min_by_key(|(source, _)| fact_rank(source))
        .map(|(_, n)| n)
}

/// Merge several sources' ingredient texts into the one to show: the
/// highest-precedence source's, whole (same reasoning as `merge_nutrition` — you
/// don't splice two ingredient lists together). Pure.
pub fn merge_ingredients(texts: Vec<(String, String)>) -> Option<String> {
    texts
        .into_iter()
        .filter(|(_, t)| !t.trim().is_empty())
        .min_by_key(|(source, _)| fact_rank(source))
        .map(|(_, t)| t)
}

/// Merge several sources' allergen claims into one set.
///
/// Allergens are safety-critical, so this is a UNION, never a pick: an allergen
/// one source declares is kept even if another source is silent about it
/// (absence is not a "free from" — the same reason the dietary merge never reads
/// a missing flag as "no"). Where two sources name the same allergen with
/// different presence, the more severe wins: `contains` beats `may_contain`.
///
/// Input is `(source, allergen)` pairs; the result has one entry per allergen,
/// sorted. Pure — the unit under test.
pub fn merge_allergens(claims: Vec<(String, Allergen)>) -> Vec<Allergen> {
    let mut by_name: BTreeMap<String, &'static str> = BTreeMap::new();
    for (_, a) in &claims {
        let severe = a.presence == "contains";
        by_name
            .entry(a.allergen.clone())
            .and_modify(|p| {
                if severe {
                    *p = "contains";
                }
            })
            .or_insert(if severe { "contains" } else { "may_contain" });
    }
    by_name
        .into_iter()
        .filter(|(name, _)| !name.is_empty())
        .map(|(allergen, presence)| Allergen {
            allergen,
            presence: presence.to_string(),
        })
        .collect()
}

/// Rank of a fact source (lower wins), reusing the canonical-name precedence so
/// facts follow the same "retailer over crowd" order. An unlisted source sorts
/// last, so it only ever fills a gap.
pub fn fact_rank(source: &str) -> usize {
    crate::products::source::name_rank(source).unwrap_or(usize::MAX)
}

/// A one-line summary of a nutrition panel — the headline figures, per basis —
/// used as the display value when the panel is a reconcile candidate (you don't
/// diff a whole table in a radio row). Whatever figures a source declares; energy
/// leads, then the macros it has.
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

/// Format a nutrition figure without a trailing ".0" (so 3.0 → "3", 3.4 → "3.4").
fn trim_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A numeric OFF value, whether it arrived as a JSON number or a numeric string
/// (OFF is inconsistent). `None` for anything non-numeric.
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The part of an OFF tag after its language prefix: "en:milk" → "milk". Tags
/// without a prefix pass through unchanged.
fn strip_lang(tag: &str) -> &str {
    tag.split_once(':').map(|(_, rest)| rest).unwrap_or(tag)
}

impl RawFacts {
    /// Turn the raw OFF fields into our domain facts. Pure — the unit under test.
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
        // OFF suffixes every per-quantity nutriment `_100g` even for liquids (a
        // historical misnomer — the value is per 100 ml there); `basis` records
        // which unit it really is for display.
        let basis = match self.nutrition_data_per.as_deref() {
            Some("100ml") => "100ml",
            _ => "100g",
        }
        .to_string();
        // The tail: every other per-100 nutriment, suffix stripped, promoted keys
        // excluded. Deterministic order via BTreeMap.
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
        // Prefer the English text when OFF has a localised copy.
        non_empty(self.ingredients_text_en.as_deref())
            .or_else(|| non_empty(self.ingredients_text.as_deref()))
    }

    fn allergens(&self) -> Vec<Allergen> {
        // "contains" wins over a "may_contain" trace of the same allergen.
        let mut by_name: BTreeMap<String, &str> = BTreeMap::new();
        for tag in &self.traces_tags {
            by_name.insert(strip_lang(tag).to_string(), "may_contain");
        }
        for tag in &self.allergens_tags {
            by_name.insert(strip_lang(tag).to_string(), "contains");
        }
        by_name
            .into_iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|(allergen, presence)| Allergen {
                allergen,
                presence: presence.to_string(),
            })
            .collect()
    }

    fn dietary(&self) -> Vec<DietaryFlag> {
        let mut flags: BTreeMap<String, String> = BTreeMap::new();
        // OFF's ingredient analysis is tri-state (vegan / non-vegan / maybe-vegan;
        // same for vegetarian; palm oil has its own vocabulary).
        for tag in &self.ingredients_analysis_tags {
            if let Some((flag, value)) = analysis_flag(strip_lang(tag)) {
                flags.insert(flag.to_string(), value.to_string());
            }
        }
        // A manufacturer label is a firm claim: it asserts "yes" and overrides a
        // softer analysis guess for the same flag.
        for tag in &self.labels_tags {
            let stripped = strip_lang(tag);
            if let Some((_, flag)) = LABEL_FLAGS.iter().find(|(label, _)| *label == stripped) {
                flags.insert(flag.to_string(), "yes".to_string());
            }
        }
        flags
            .into_iter()
            .map(|(flag, value)| DietaryFlag { flag, value })
            .collect()
    }
}

/// Map one OFF ingredient-analysis tag (prefix already stripped) to a
/// (flag, value) pair, or `None` when it asserts nothing (e.g. content unknown).
fn analysis_flag(tag: &str) -> Option<(&'static str, &'static str)> {
    Some(match tag {
        "vegan" => ("vegan", "yes"),
        "non-vegan" => ("vegan", "no"),
        "maybe-vegan" => ("vegan", "maybe"),
        "vegetarian" => ("vegetarian", "yes"),
        "non-vegetarian" => ("vegetarian", "no"),
        "maybe-vegetarian" => ("vegetarian", "maybe"),
        "palm-oil-free" => ("palm_oil_free", "yes"),
        "palm-oil" => ("palm_oil_free", "no"),
        "may-contain-palm-oil" => ("palm_oil_free", "maybe"),
        _ => return None,
    })
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}
