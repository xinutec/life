//! Open Food Facts lookup by barcode, on a catalogue miss.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::media::{self, Media};

use super::ids::Barcode;
use super::nutrition::{ProductFacts, RawFacts};

const USER_AGENT: &str = "Life/0.1 (https://life.xinutec.org)";

/// So a poisoned URL cannot stream gigabytes into a 256 MiB pod.
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

pub struct OffProduct {
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity: Option<String>,
    pub image_url: Option<String>,
    pub facts: ProductFacts,
    /// The response verbatim, for the `off` listing.
    pub raw: String,
}

#[derive(Deserialize)]
struct Envelope {
    status: i64,
    product: Option<Raw>,
}

#[derive(Deserialize)]
struct Raw {
    product_name: Option<String>,
    brands: Option<String>,
    quantity: Option<String>,
    image_front_url: Option<String>,
    #[serde(flatten)]
    facts: RawFacts,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.trim().is_empty())
}

/// Also the upload cap.
pub const MAX_UPLOAD_BYTES: usize = MAX_IMAGE_BYTES;

/// Not SVG: it can carry script, and images are served from our origin.
const ALLOWED_IMAGE_MIMES: [&str; 5] = [
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/avif",
];

/// The normalised mime if it is on the raster allowlist; for uploads and OFF
/// alike.
pub fn accept_upload_mime(content_type: &str) -> Option<String> {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    ALLOWED_IMAGE_MIMES.contains(&mime.as_str()).then_some(mime)
}

/// The type from the magic bytes, which is what gets stored: the declared one can
/// lie. `None`: refuse.
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    // Narrower than the attachment allowlist on purpose: a product image renders
    // inline everywhere, and PDF and HEIC do not.
    let media = media::sniff(bytes)?;
    match media {
        Media::Jpeg | Media::Png | Media::Gif | Media::Webp | Media::Avif => Some(media.mime()),
        Media::Heic | Media::Pdf => None,
    }
}

/// `Ok(None)`: OFF has no such product. A [`Barcode`] is digits, so it splices
/// safely.
pub async fn fetch(http: &reqwest::Client, barcode: &Barcode) -> Result<Option<OffProduct>> {
    let url = format!(
        "https://world.openfoodfacts.org/api/v2/product/{barcode}.json\
         ?fields=product_name,brands,quantity,image_front_url,\
         nutriments,nutrition_data_per,serving_size,\
         ingredients_text,ingredients_text_en,\
         allergens_tags,traces_tags,ingredients_analysis_tags,labels_tags"
    );
    let res = http
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let body = res.text().await.context("reading OFF response")?;
    let env: Envelope = serde_json::from_str(&body).context("parsing OFF response")?;
    if env.status != 1 {
        return Ok(None);
    }
    let Some(p) = env.product else {
        return Ok(None);
    };
    Ok(Some(OffProduct {
        name: non_empty(p.product_name),
        brand: non_empty(p.brands),
        quantity: non_empty(p.quantity),
        image_url: non_empty(p.image_front_url),
        facts: p.facts.parse(),
        raw: body,
    }))
}

/// The SSRF guard: https, and a host equal to or under an allowed suffix. The
/// leading dot rejects `openfoodfacts.org.evil.com`; `url::Url` defeats
/// `host.tld@evil.com`.
pub fn host_allowed(url: &str, suffixes: &[&str]) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    match parsed.host_str() {
        Some(host) => suffixes
            .iter()
            .any(|s| host == *s || host.ends_with(&format!(".{s}"))),
        None => false,
    }
}

/// Download an image from an allowed host: https, no redirects, bounded time and
/// size, and really an image. `Ok(None)` if disallowed or unfetchable.
pub async fn fetch_image_from(url: &str, allowed: &[&str]) -> Result<Option<(Vec<u8>, String)>> {
    if !host_allowed(url, allowed) {
        tracing::warn!(%url, ?allowed, "refusing product image: host not in the allowlist or not https");
        return Ok(None);
    }
    // No redirects, or an allowed URL could bounce us to an internal host.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let mut res = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;
    if !res.status().is_success() {
        return Ok(None);
    }
    let declared = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg");
    let Some(mime) = accept_upload_mime(declared) else {
        tracing::warn!(%url, %declared, "refusing product image: not an allowed image type");
        return Ok(None);
    };
    if res
        .content_length()
        .is_some_and(|n| n > MAX_IMAGE_BYTES as u64)
    {
        tracing::warn!(%url, "refusing product image: declared size over cap");
        return Ok(None);
    }
    // Capped while streaming: Content-Length may be absent or lie.
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
            tracing::warn!(%url, "refusing product image: streamed size over cap");
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    // The bytes, not the header, decide.
    let Some(sniffed) = sniff_image_mime(&bytes) else {
        tracing::warn!(%url, %mime, "refusing product image: bytes are not a known raster type");
        return Ok(None);
    };
    Ok(Some((bytes, sniffed.to_string())))
}

pub async fn fetch_image(url: &str) -> Result<Option<(Vec<u8>, String)>> {
    fetch_image_from(url, &["openfoodfacts.org"]).await
}
