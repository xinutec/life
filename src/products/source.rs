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

/// Every source importable through `POST /api/products/import`.
static IMPORTABLE: &[&Source] = &[&WAITROSE];

/// The source policy for `id`, or `None` if that source can't be imported.
pub fn importable(id: &str) -> Option<&'static Source> {
    IMPORTABLE.iter().copied().find(|s| s.id == id)
}
