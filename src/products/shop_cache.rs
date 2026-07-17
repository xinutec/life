//! Our memory of the shops' catalogues — every listing a shop query ever showed
//! us, kept so the next lookup can be answered without asking the shop again.
//!
//! WHY: a shop query returns far more than the product that prompted it. One
//! Asda search hands back ~15 hits, each with its own EAN; we used to read the
//! one that matched and drop the other 14, then pay for a fresh search the next
//! time. Those 14 were durable facts about the world (this barcode is this CIN),
//! bought and thrown away. Here they're kept, so lookups converge on zero
//! outbound traffic as the cache fills.
//!
//! This is NOT the catalogue. `products`/`product_listings` are the things in
//! your life; these are things a shop happens to sell that we've laid eyes on.
//! A row graduates into a real `product_listings` row only when it's matched to
//! a product and attached (see routes::products::sync_listing).
//!
//! Nothing here refreshes itself. A cached row is served until you press
//! refresh — shop data going quietly stale beats a price you didn't ask for
//! being silently wrong.

use anyhow::Result;
use sqlx::MySqlPool;

use super::asda::AsdaHit;

/// One shop listing as the shop described it. Shop-agnostic on purpose: Asda
/// fills it from an Algolia hit server-side, Waitrose from the Android bridge's
/// WebView fetch. Field order matches `find_by_barcode`'s SELECT (FromRow reads
/// by position).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct CachedListing {
    pub source: String,
    pub external_id: String,
    /// `None` means "we haven't learned it yet", not "it has none" — a Waitrose
    /// search hit carries no barcode until its product page is fetched.
    pub barcode: Option<String>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    pub image_url: Option<String>,
}

impl CachedListing {
    /// An Asda search hit is already a complete cache row: its `IMAGE_ID` is the
    /// EAN, so every hit teaches us a barcode → CIN mapping for free.
    pub fn from_asda(hit: &AsdaHit) -> Self {
        Self {
            source: "asda".to_string(),
            external_id: hit.external_id.clone(),
            barcode: hit.barcode.clone(),
            name: Some(hit.name.clone()),
            brand: hit.brand.clone(),
            quantity_label: hit.quantity_label.clone(),
            image_url: hit.image_url.clone(),
        }
    }
}

/// Store everything a shop query showed us, keyed by the shop's own identity.
///
/// Upserts: re-seeing a listing refreshes its description and bumps
/// `last_seen_at`. A field we've since learned is never overwritten with the
/// `NULL` of a thinner sighting — a Waitrose search hit (no barcode) must not
/// erase the barcode an earlier product fetch taught us, which is exactly the
/// silent-erasure shape that bit `product_dietary_flags` in increment 6.
pub async fn remember(pool: &MySqlPool, listings: &[CachedListing]) -> Result<()> {
    for l in listings {
        sqlx::query(
            "INSERT INTO shop_listings
                 (source, external_id, barcode, name, brand, quantity_label, image_url)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE
                 barcode        = COALESCE(VALUES(barcode), barcode),
                 name           = COALESCE(VALUES(name), name),
                 brand          = COALESCE(VALUES(brand), brand),
                 quantity_label = COALESCE(VALUES(quantity_label), quantity_label),
                 image_url      = COALESCE(VALUES(image_url), image_url),
                 last_seen_at   = CURRENT_TIMESTAMP",
        )
        .bind(&l.source)
        .bind(&l.external_id)
        .bind(&l.barcode)
        .bind(&l.name)
        .bind(&l.brand)
        .bind(&l.quantity_label)
        .bind(&l.image_url)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// "Does <source> carry <barcode>?", answered from memory alone.
///
/// `Ok(None)` means only "we don't know" — never "the shop doesn't sell it".
/// The caller decides whether to go ask; this function never does.
pub async fn find_by_barcode(
    pool: &MySqlPool,
    source: &str,
    barcode: &str,
) -> Result<Option<CachedListing>> {
    Ok(sqlx::query_as::<_, CachedListing>(
        "SELECT source, external_id, barcode, name, brand, quantity_label, image_url
               FROM shop_listings
              WHERE source = ? AND barcode = ?
              LIMIT 1",
    )
    .bind(source)
    .bind(barcode)
    .fetch_optional(pool)
    .await?)
}
