//! Product-source registry.
//!
//! `products.source` records where a catalog row came from ('off' = Open Food
//! Facts, 'user' = hand-entered, or a shop). Open Food Facts and hand-entry have
//! their own code paths; this registry lists the sources importable through the
//! generic `POST /api/products/import` and carries the only per-source policy the
//! import path needs: the image-host allowlist for the SSRF guard. Adding a shop
//! is one entry here — the route, storage, and wire types stay source-agnostic.

/// A source that can be imported via the generic import endpoint.
pub struct Source {
    /// Value stored in `products.source`.
    pub id: &'static str,
    /// Allowed image-host suffixes for the SSRF guard (https only; host must
    /// equal a suffix or be a subdomain of one). Empty = this source may not
    /// carry an `image_url`.
    pub image_hosts: &'static [&'static str],
}

/// Waitrose: products keyed by their `lineNumber`; images on the (ungated) CDN.
static WAITROSE: Source = Source {
    id: "waitrose",
    image_hosts: &["wtrecom.com"],
};

/// Asda: products keyed by their CIN (see products::asda); images on the
/// (ungated) scene7 CDN, keyed by the product's EAN.
static ASDA: Source = Source {
    id: "asda",
    image_hosts: &["scene7.com"],
};

/// Every source importable through `POST /api/products/import`.
static IMPORTABLE: &[&Source] = &[&WAITROSE, &ASDA];

/// The source policy for `id`, or `None` if that source can't be imported.
pub fn importable(id: &str) -> Option<&'static Source> {
    IMPORTABLE.iter().copied().find(|s| s.id == id)
}

/// The public product-page URL for a listing, derived from its identity alone —
/// no slug needed (probed 2026-07-16: Asda's PDP is slugless and the old
/// groceries.asda.com host just 301s to it; Waitrose redirects any slug to the
/// canonical one, keyed by the trailing lineNumber). `external_id` is safe to
/// splice: import validates it as [A-Za-z0-9_-]{1,64}, and OFF's is a validated
/// numeric barcode. `None` for sources without a page ('user').
pub fn listing_url(source: &str, external_id: &str) -> Option<String> {
    match source {
        "off" => Some(format!(
            "https://world.openfoodfacts.org/product/{external_id}"
        )),
        "asda" => Some(format!(
            "https://www.asda.com/groceries/product/{external_id}"
        )),
        "waitrose" => Some(format!(
            "https://www.waitrose.com/ecom/products/x/{external_id}"
        )),
        _ => None,
    }
}

/// Canonical-name preference, best first: retailers curate their titles, Open
/// Food Facts names are crowd-sourced and often messy. A source not listed here
/// (e.g. 'user') never supplies the canonical name.
const NAME_PREFERENCE: &[&str] = &["waitrose", "asda", "off"];

/// Rank of `source` in the canonical-name preference order (lower wins), or
/// `None` if the source doesn't participate.
pub fn name_rank(source: &str) -> Option<usize> {
    NAME_PREFERENCE.iter().position(|s| *s == source)
}
