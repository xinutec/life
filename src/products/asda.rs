//! Asda search through its public Algolia index: the key is search-only and in
//! every browser, so no login or bot wall. Name search only (`IMAGE_ID`, the EAN,
//! is not searchable). On 4xx the key has rotated: copy `x-algolia-api-key` from a
//! search in browser devtools into `SEARCH_KEY`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::{Barcode, ExternalId};
use super::nutrition::{Claim, Diet, DietaryFlag};
use super::prices::{Currency, PriceInput, UnitMeasure, UnitPrice};

/// Also the request host (`{app}-dsn.algolia.net`).
const APP_ID: &str = "8I6WSKCCNV";
/// Search-only and in Asda's own bundle; not a secret.
const SEARCH_KEY: &str = "03e4272048dd17f771da37b57ff8a75e";
const INDEX: &str = "ASDA_PRODUCTS";
/// The scene7 CDN, keyed by `IMAGE_ID`; open to the server. `$ProdList$` is the
/// list-thumbnail preset.
const IMAGE_BASE: &str = "https://asdagroceries.scene7.com/is/image/asdagroceries/";

/// An Asda search hit, as the picker shows and imports it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AsdaHit {
    /// The CIN, which the import keys on.
    pub external_id: ExternalId,
    pub name: String,
    pub brand: Option<String>,
    /// The EAN from `IMAGE_ID`, when barcode-shaped.
    pub barcode: Option<Barcode>,
    pub quantity_label: Option<String>,
    /// England price, formatted.
    pub price_label: Option<String>,
    /// England price, recorded as an observation on import.
    pub price: Option<PriceInput>,
    pub image_url: Option<String>,
    /// Lifestyle tags as dietary flags, assertions only.
    pub dietary: Vec<DietaryFlag>,
    /// The whole record, verbatim, for the listing; not sent to the client.
    #[serde(skip)]
    #[ts(skip)]
    pub raw: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AlgoliaResponse {
    results: Vec<AlgoliaResult>,
}

#[derive(Deserialize)]
struct AlgoliaResult {
    // Each hit is decoded AND kept verbatim, so an unparsed field is never lost.
    #[serde(default)]
    hits: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawHit {
    #[serde(rename = "CIN")]
    cin: Option<String>,
    #[serde(rename = "objectID")]
    object_id: Option<String>,
    #[serde(rename = "NAME")]
    name: Option<String>,
    #[serde(rename = "BRAND")]
    brand: Option<String>,
    #[serde(rename = "IMAGE_ID")]
    image_id: Option<String>,
    #[serde(rename = "PACK_SIZE")]
    pack_size: Option<String>,
    #[serde(rename = "PRICES")]
    prices: Option<Prices>,
    /// Every tag Asda knows, 1 = claimed, 0 = not claimed.
    #[serde(rename = "NUTRITIONAL_INFO", default)]
    nutritional_info: std::collections::BTreeMap<String, i64>,
}

/// Lifestyle tag → our flag, so Asda's merge with OFF's. Only diet and free-from
/// tags; the rest (LowSalt, …) are not yes/no.
const LIFESTYLE_FLAGS: &[(&str, Diet)] = &[
    ("Vegan", Diet::Vegan),
    ("Vegetarian", Diet::Vegetarian),
    ("Halal", Diet::Halal),
    ("Kosher", Diet::Kosher),
    ("NoGluten", Diet::GlutenFree),
    ("NoLactose", Diet::LactoseFree),
    ("NoNuts", Diet::NutFree),
    ("NoMilk", Diet::MilkFree),
    ("NoEgg", Diet::EggFree),
    ("NoSoya", Diet::SoyaFree),
    ("Organic", Diet::Organic),
];

/// Every flag 'yes': a 0 is not a "no". Asda sets only what it claims, and Quaker
/// Oat So Simple has `Vegetarian: 0`.
fn lifestyle_flags(info: &std::collections::BTreeMap<String, i64>) -> Vec<DietaryFlag> {
    LIFESTYLE_FLAGS
        .iter()
        .filter(|(tag, _)| info.get(*tag).is_some_and(|v| *v == 1))
        .map(|(_, flag)| DietaryFlag {
            flag: *flag,
            value: Claim::Yes,
        })
        .collect()
}

#[derive(Deserialize)]
struct Prices {
    #[serde(rename = "EN")]
    en: Option<PriceRegion>,
}

#[derive(Deserialize)]
struct PriceRegion {
    #[serde(rename = "PRICE")]
    price: Option<f64>,
    #[serde(rename = "PRICEPERUOM")]
    price_per_uom: Option<f64>,
    #[serde(rename = "PRICEPERUOMFORMATTED")]
    price_per_uom_formatted: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Pounds to pence, or `None` for anything not a real price: `as i64` would turn
/// NaN into 0 silently.
fn to_minor(pounds: f64) -> Option<i64> {
    /// Past this, a "price" means the payload is wrong.
    const MAX_PENCE: f64 = 100_000_000.0;
    let pence = (pounds * 100.0).round();
    if !pence.is_finite() || !(0.0..=MAX_PENCE).contains(&pence) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "guarded above: finite and within 0..=MAX_PENCE, which i64 holds exactly"
    )]
    Some(pence as i64)
}

/// "£8.93/KG" → Kg.
fn unit_measure(formatted: &str) -> Option<UnitMeasure> {
    UnitMeasure::from_shop(formatted.rsplit_once('/')?.1)
}

/// From the England region; `None` without a positive shelf price.
fn price_input(r: &PriceRegion) -> Option<PriceInput> {
    let amount = r.price.filter(|p| *p > 0.0)?;
    Some(PriceInput {
        amount_minor: to_minor(amount)?,
        currency: Currency::gbp(),
        unit_price: r
            .price_per_uom
            .filter(|p| *p > 0.0)
            .and_then(to_minor)
            .zip(r.price_per_uom_formatted.as_deref().and_then(unit_measure))
            .map(|(amount_minor, measure)| UnitPrice {
                amount_minor,
                measure,
            }),
    })
}

/// The pure half of `search`. Hits without a CIN and a name are dropped.
pub fn parse_hits(body: &str) -> Result<Vec<AsdaHit>> {
    let parsed: AlgoliaResponse =
        serde_json::from_str(body).context("Asda Algolia decode failed")?;
    Ok(parsed
        .results
        .into_iter()
        .flat_map(|r| r.hits)
        .filter_map(|value| {
            let raw: RawHit = serde_json::from_value(value.clone()).ok()?;
            normalize(raw, value)
        })
        .collect())
}

fn normalize(raw: RawHit, raw_value: serde_json::Value) -> Option<AsdaHit> {
    // An id we cannot address the listing by is not an identity.
    let external_id: ExternalId = non_empty(raw.cin.or(raw.object_id))?.parse().ok()?;
    let name = non_empty(raw.name)?;
    let image_id = non_empty(raw.image_id);
    // IMAGE_ID is usually but not always the EAN.
    let barcode = image_id.as_deref().and_then(|id| id.parse().ok());
    let image_url = image_id.map(|id| format!("{IMAGE_BASE}{id}?$ProdList$"));
    let en = raw.prices.and_then(|p| p.en);
    let price = en.as_ref().and_then(price_input);
    let price_label = en
        .as_ref()
        .and_then(|r| r.price)
        .filter(|p| *p > 0.0)
        .map(|p| format!("£{p:.2}"));
    Some(AsdaHit {
        external_id,
        name,
        brand: non_empty(raw.brand),
        barcode,
        quantity_label: non_empty(raw.pack_size),
        price_label,
        price,
        image_url,
        dietary: lifestyle_flags(&raw.nutritional_info),
        raw: Some(raw_value),
    })
}

/// A shorter second search when the full name finds nothing: the brand and the
/// name's first word, skipping brand words and words with digits or `%`. Asda
/// ranks long names badly ("Fusilli 100% durum wheat" matches only "100%").
pub fn fallback_query(name: &str, brand: Option<&str>) -> Option<String> {
    let brands = brand?;
    let brand = brands.split(',').next()?.trim();
    if brand.is_empty() {
        return None;
    }
    let words = |s: &str| {
        s.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '&'))
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    let brand_words = words(brands);
    let head = name
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '&' || c == '%'))
        .find(|w| {
            !w.is_empty()
                && !w.chars().any(|c| c.is_ascii_digit() || c == '%')
                && !brand_words.contains(&w.to_lowercase())
        })?;
    Some(format!("{brand} {head}"))
}

/// The hit with this product's barcode. Ranking is no identity: "Asda ES
/// Balsamic Modena" ranks a raspberry glaze first.
pub fn match_barcode(hits: Vec<AsdaHit>, barcode: &Barcode) -> Option<AsdaHit> {
    hits.into_iter()
        .find(|h| h.barcode.as_ref() == Some(barcode))
}

/// One product by its CIN. Asda has no by-id endpoint, so the CIN is searched and
/// the hit's own CIN verified.
pub async fn fetch_by_id(http: &reqwest::Client, cin: &ExternalId) -> Result<Option<AsdaHit>> {
    Ok(search(http, cin.as_str(), 5)
        .await?
        .into_iter()
        .find(|h| &h.external_id == cin))
}

/// Up to `limit` hits in Algolia's order; a blank query makes no call.
pub async fn search(http: &reqwest::Client, query: &str, limit: u32) -> Result<Vec<AsdaHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }
    let url = format!("https://{APP_ID}-dsn.algolia.net/1/indexes/*/queries");
    let body = serde_json::json!({
        "requests": [{
            "indexName": INDEX,
            "query": query,
            "params": format!("hitsPerPage={}", limit.clamp(1, 40)),
        }]
    });
    let resp = http
        .post(&url)
        .header("x-algolia-application-id", APP_ID)
        .header("x-algolia-api-key", SEARCH_KEY)
        .header("content-type", "application/x-www-form-urlencoded")
        .json(&body)
        .send()
        .await
        .context("Asda Algolia request failed")?;
    if !resp.status().is_success() {
        anyhow::bail!("Asda Algolia returned HTTP {}", resp.status());
    }
    let text = resp.text().await.context("Asda Algolia read failed")?;
    parse_hits(&text)
}
