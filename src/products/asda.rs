//! Asda groceries product search, via their public Algolia index.
//!
//! Unlike the Waitrose provider (a hidden WebView on the shop site, Android
//! only — see the frontend `shops/`), Asda's storefront search is a plain,
//! CORS-open Algolia REST call keyed by a *search-only* API key. That key is a
//! public client credential: Asda ships it to every browser that loads
//! groceries. So we can query it server-side, from anywhere, with no login and
//! no bot-wall — which is why this lives in the backend and works in the web
//! app as well as the phone.
//!
//! The response's `IMAGE_ID` is the product's primary EAN barcode, which is
//! both the scene7 image key and a genuine barcode we can hand back to the
//! caller. There is no reverse (barcode → product) lookup here: `IMAGE_ID`
//! isn't a searchable Algolia attribute, so this is name search only.
//!
//! REFRESH: if searches start 4xx-ing, Asda has rotated the search key. Grab the
//! fresh one from the `x-algolia-api-key` request header on groceries search
//! (any browser devtools/Network) and update `SEARCH_KEY` below.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::off::is_valid_barcode;
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
    pub external_id: String,
    pub name: String,
    pub brand: Option<String>,
    /// Primary EAN (from `IMAGE_ID`) when it's barcode-shaped; the shopping row
    /// carries this even though the imported catalogue row stays barcodeless.
    pub barcode: Option<String>,
    /// Pack size, e.g. "400G".
    pub quantity_label: Option<String>,
    /// Formatted England price for display, e.g. "£3.57".
    pub price_label: Option<String>,
    /// Structured England price (minor units + per-unit), recorded as a price
    /// observation when this hit is imported. `None` when the hit has no price.
    pub price: Option<PriceInput>,
    /// scene7 thumbnail URL (host-allowlisted for server-side import).
    pub image_url: Option<String>,
}

// --- Algolia wire shapes (only the fields we use) ---

#[derive(Deserialize)]
struct AlgoliaResponse {
    results: Vec<AlgoliaResult>,
}

#[derive(Deserialize)]
struct AlgoliaResult {
    #[serde(default)]
    hits: Vec<RawHit>,
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
fn to_minor(pounds: f64) -> i64 {
    (pounds * 100.0).round() as i64
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
        amount_minor: to_minor(amount),
        currency: "GBP".into(),
        unit_amount_minor: r.price_per_uom.filter(|p| *p > 0.0).map(to_minor),
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
        .filter_map(normalize)
        .collect())
}

/// Turn one raw Algolia hit into an `AsdaHit`, or `None` if it lacks the
/// identity we need (a CIN and a name).
fn normalize(raw: RawHit) -> Option<AsdaHit> {
    let external_id = non_empty(raw.cin.or(raw.object_id))?;
    // Import validates external_id as [A-Za-z0-9_-]{1,64}; a CIN is digits, but
    // guard here too so a weird id never reaches the import route.
    if external_id.len() > 64
        || !external_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let name = non_empty(raw.name)?;
    let image_id = non_empty(raw.image_id);
    let barcode = image_id.clone().filter(|id| is_valid_barcode(id));
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
    })
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
