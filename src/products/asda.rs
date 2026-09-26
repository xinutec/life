//! Asda product search via its public Algolia index. The key is search-only and
//! shipped to every browser, so the server can query it with no login or bot
//! wall; that is why Asda works in the web app and Waitrose (a WebView provider
//! in the frontend's `shops/`) does not.
//!
//! `IMAGE_ID` is the primary EAN: both the scene7 image key and a real barcode.
//! It is not searchable, so this is name search only.
//!
//! If searches start failing with 4xx, the key has rotated: copy the
//! `x-algolia-api-key` request header from a search in browser devtools into
//! `SEARCH_KEY`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ids::{Barcode, ExternalId};
use super::nutrition::{Claim, DietaryFlag};
use super::prices::PriceInput;

/// Algolia application id — also the request host (`{app}-dsn.algolia.net`).
const APP_ID: &str = "8I6WSKCCNV";
/// Public *search-only* key (see module docs; safe to commit — it's in Asda's
/// own browser bundle). Not a user secret.
const SEARCH_KEY: &str = "03e4272048dd17f771da37b57ff8a75e";
/// The storefront product index.
const INDEX: &str = "ASDA_PRODUCTS";
/// scene7 image CDN, keyed by the product's `IMAGE_ID` (its EAN). Ungated
/// (200 from anywhere), so the import path can fetch it server-side. `$ProdList$`
/// is Asda's list-thumbnail preset.
const IMAGE_BASE: &str = "https://asdagroceries.scene7.com/is/image/asdagroceries/";

/// A normalized Asda search hit, ready for the product picker. Mirrors the
/// fields the picker shows plus the identity it needs to import + link.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct AsdaHit {
    /// Asda catalogue item number (CIN); the stable per-source id we import by.
    pub external_id: ExternalId,
    pub name: String,
    pub brand: Option<String>,
    /// Primary EAN (from `IMAGE_ID`) when it's barcode-shaped; the shopping row
    /// carries this even though the imported catalogue row stays barcodeless.
    pub barcode: Option<Barcode>,
    /// Pack size, e.g. "400G".
    pub quantity_label: Option<String>,
    /// Formatted England price for display, e.g. "£3.57".
    pub price_label: Option<String>,
    /// Structured England price (minor units + per-unit), recorded as a price
    /// observation when this hit is imported. `None` when the hit has no price.
    pub price: Option<PriceInput>,
    /// scene7 thumbnail URL (host-allowlisted for server-side import).
    pub image_url: Option<String>,
    /// Asda's own lifestyle tags for this product (vegan, gluten-free, …), as
    /// dietary flags. Only ever assertions — see `LIFESTYLE_FLAGS`.
    pub dietary: Vec<DietaryFlag>,
    /// Asda's ENTIRE record for this hit, verbatim — the lossless backstop that
    /// the structured fields above are extracted from. Kept off the wire
    /// (`serde(skip)`) and off the TS bindings (`ts(skip)`): it's for storing on
    /// the listing, not for the picker to render.
    #[serde(skip)]
    #[ts(skip)]
    pub raw: Option<serde_json::Value>,
}

// --- Algolia wire shapes (only the fields we use) ---

#[derive(Deserialize)]
struct AlgoliaResponse {
    results: Vec<AlgoliaResult>,
}

#[derive(Deserialize)]
struct AlgoliaResult {
    // Kept as raw JSON values: each hit is both decoded into `RawHit` (for the
    // fields we model) AND stored verbatim (the lossless record), so a field we
    // don't parse yet is never lost.
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
    /// Asda's lifestyle tag block: every tag it knows, 1 = claimed, 0 = NOT
    /// claimed (see `lifestyle_flags`).
    #[serde(rename = "NUTRITIONAL_INFO", default)]
    nutritional_info: std::collections::BTreeMap<String, i64>,
}

/// Asda's lifestyle tag → our dietary flag slug. We map the diet/lifestyle and
/// free-from tags — a genuine dietary restriction, and (for free-from) safety-
/// relevant — so the two sources' flags line up and merge instead of sitting
/// alongside as near-duplicates. Asda's remaining tags are nutrition *claims*
/// (LowSalt, LowFat, HighFibre, …): marketing about quantity, not a dietary
/// yes/no, so they'd only clutter the dietary chips — left in `raw_json` rather
/// than promoted to a flag. Nothing is lost: the full record is stored verbatim.
const LIFESTYLE_FLAGS: &[(&str, &str)] = &[
    ("Vegan", "vegan"),
    ("Vegetarian", "vegetarian"),
    ("Halal", "halal"),
    ("Kosher", "kosher"),
    ("NoGluten", "gluten_free"),
    ("NoLactose", "lactose_free"),
    ("NoNuts", "nut_free"),
    ("NoMilk", "milk_free"),
    ("NoEgg", "egg_free"),
    ("NoSoya", "soya_free"),
    ("Organic", "organic"),
];

/// Asda's lifestyle tags as dietary flags.
///
/// **A 0 is not a "no".** Asda ships all 24 tags on every product and sets the
/// ones it claims: Quaker Oat So Simple has `Vegetarian: 0` though oats plainly
/// are, while an oat drink has `Vegetarian: 1`. So 0 means "not claimed" and
/// must assert NOTHING — reading it as a negative would have the app telling you
/// a vegetarian product isn't one. Every flag here is therefore 'yes'.
fn lifestyle_flags(info: &std::collections::BTreeMap<String, i64>) -> Vec<DietaryFlag> {
    LIFESTYLE_FLAGS
        .iter()
        .filter(|(tag, _)| info.get(*tag).is_some_and(|v| *v == 1))
        .map(|(_, flag)| DietaryFlag {
            flag: (*flag).to_string(),
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

/// Pounds (Asda gives prices as floats) → integer minor units (pence). Rounded,
/// so float error can't leak into stored money.
///
/// `None` for anything that isn't a real price. `as i64` on an f64 saturates
/// (and turns NaN into 0) without saying so, which on money means a malformed
/// payload lands in the price history as a plausible-looking number. A price we
/// can't read is not a price.
fn to_minor(pounds: f64) -> Option<i64> {
    /// £1,000,000 in pence. Not a technical limit — an i64 holds far more —
    /// but the point past which a "price" is evidence the payload is wrong.
    const MAX_PENCE: f64 = 100_000_000.0;
    let pence = (pounds * 100.0).round();
    if !pence.is_finite() || !(0.0..=MAX_PENCE).contains(&pence) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "guarded above: finite and within 0..=MAX_PENCE, which i64 holds exactly"
    )]
    Some(pence as i64)
}

/// The unit of measure out of Asda's per-unit label: "£8.93/KG" → "KG". `None`
/// when there's no "/…" measure to take.
fn unit_measure(formatted: &str) -> Option<String> {
    formatted
        .rsplit_once('/')
        .map(|(_, m)| m.trim().to_string())
        .filter(|m| !m.is_empty())
}

/// Build a price observation from Asda's England price region, or `None` if it
/// has no positive shelf price.
fn price_input(r: &PriceRegion) -> Option<PriceInput> {
    let amount = r.price.filter(|p| *p > 0.0)?;
    Some(PriceInput {
        amount_minor: to_minor(amount)?,
        currency: "GBP".into(),
        unit_amount_minor: r.price_per_uom.filter(|p| *p > 0.0).and_then(to_minor),
        unit_measure: r.price_per_uom_formatted.as_deref().and_then(unit_measure),
        region: Some("EN".into()),
    })
}

/// Parse a raw Algolia multi-query response body into normalized hits. The pure
/// half of `search` (no I/O), so it's exercised directly from tests against a
/// captured response. Hits missing the identity we need (CIN + name) are dropped.
pub fn parse_hits(body: &str) -> Result<Vec<AsdaHit>> {
    let parsed: AlgoliaResponse =
        serde_json::from_str(body).context("Asda Algolia decode failed")?;
    Ok(parsed
        .results
        .into_iter()
        .flat_map(|r| r.hits)
        .filter_map(|value| {
            // A hit that can't even be decoded into the fields we model can't be
            // identified or imported, so it's dropped — same fate as one missing
            // its CIN/name below.
            let raw: RawHit = serde_json::from_value(value.clone()).ok()?;
            normalize(raw, value)
        })
        .collect())
}

/// Turn one raw Algolia hit into an `AsdaHit`, or `None` if it lacks the
/// identity we need (a CIN and a name). `raw_value` is the same hit untouched,
/// carried onto the `AsdaHit` as the lossless record.
fn normalize(raw: RawHit, raw_value: serde_json::Value) -> Option<AsdaHit> {
    // A hit whose CIN isn't a well-formed id is dropped rather than repaired:
    // an id we can't address the listing by is not an identity.
    let external_id: ExternalId = non_empty(raw.cin.or(raw.object_id))?.parse().ok()?;
    let name = non_empty(raw.name)?;
    let image_id = non_empty(raw.image_id);
    // IMAGE_ID is usually the EAN but not always; a non-barcode one just means
    // this hit teaches us no barcode.
    let barcode = image_id.as_deref().and_then(|id| id.parse().ok());
    let image_url = image_id.map(|id| format!("{IMAGE_BASE}{id}?$ProdList$"));
    // The England region feeds both the display label and the structured price.
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

/// A second, shorter search for when the full name finds nothing: the first
/// listed brand and the name's first word, skipping brand words and words with
/// digits or `%`. Asda ranks long names badly ("Fusilli 100% durum wheat"
/// matches only "100%"); measured on the catalogue, this recovers what a third
/// or fourth query shape would. `None` without a brand or a word left.
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

/// The hit whose barcode is this product's, or `None`.
///
/// Asda can only be searched by name, and its ranking is no identity (a search
/// for "Asda ES Balsamic Modena" ranks a raspberry glaze first), so relevance is
/// ignored and only a barcode match counts. `None` means every hit was checked.
pub fn match_barcode(hits: Vec<AsdaHit>, barcode: &Barcode) -> Option<AsdaHit> {
    hits.into_iter()
        .find(|h| h.barcode.as_ref() == Some(barcode))
}

/// One product by its CIN — the exact-identity fetch behind "refresh this
/// listing". Asda has no by-id endpoint, but the CIN IS a searchable attribute,
/// so we query it and then VERIFY the hit's own CIN rather than trusting the
/// first result: a search is a relevance guess, and this must be an identity.
pub async fn fetch_by_id(http: &reqwest::Client, cin: &ExternalId) -> Result<Option<AsdaHit>> {
    Ok(search(http, cin.as_str(), 5)
        .await?
        .into_iter()
        .find(|h| &h.external_id == cin))
}

/// Search the Asda storefront by product name. Returns up to `limit` normalized
/// hits (best-match order preserved from Algolia). A blank query yields `[]`
/// without a network call.
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
