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
use serde::Deserialize;
use sqlx::MySqlPool;
use ts_rs::TS;

use super::asda::AsdaHit;
use super::ids::{Barcode, ExternalId};
use super::off;
use super::source::Source;

/// One shop listing as the shop described it. Shop-agnostic on purpose: Asda
/// fills it from an Algolia hit server-side, Waitrose from the Android bridge's
/// WebView fetch. Field order matches `find_by_barcode`'s SELECT (FromRow reads
/// by position).
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct CachedListing {
    pub source: Source,
    pub external_id: ExternalId,
    /// `None` means "we haven't learned it yet", not "it has none" — a Waitrose
    /// search hit carries no barcode until its product page is fetched.
    pub barcode: Option<Barcode>,
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
            source: Source::Asda,
            external_id: hit.external_id.clone(),
            barcode: hit.barcode.clone(),
            name: Some(hit.name.clone()),
            brand: hit.brand.clone(),
            quantity_label: hit.quantity_label.clone(),
            image_url: hit.image_url.clone(),
        }
    }
}

/// One listing a client's WebView saw, as it reports it.
///
/// Exists because a bot-walled shop (Waitrose) can only be queried by the phone:
/// the server can't see what the phone saw, so the phone hands it back and the
/// memory fills exactly as it does for the shops the server can query itself.
/// Everything but the shop's own id is optional — a search hit knows a name and
/// a lineNumber, and only a product fetch learns the barcode.
///
/// Its fields are plain strings, deliberately: this is what the phone *said*,
/// not yet something we believe. `validate_seen` is where it becomes typed, and
/// the pair of structs is that boundary made visible.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct SeenListing {
    pub external_id: String,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    #[serde(default)]
    pub quantity_label: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
}

/// The most listings one report may carry. A Waitrose search returns 8 and an
/// Asda search 15; anything near this is a client bug, not a busy shopper.
pub const MAX_SEEN: usize = 50;

/// Turn a client's report into cache rows, or say why it can't be trusted.
///
/// Rejects (whole batch) what would poison the identity index: an unknown shop,
/// a malformed shop id, or a barcode that isn't a barcode — the `(source,
/// barcode)` lookup is the one thing this table exists to answer, so a wrong
/// row there is worse than no row.
///
/// Drops (row keeps its identity) an `image_url` from a host the source isn't
/// allowed to serve pictures from: the picture is a nicety, the identity is the
/// point, and losing a hunt over a CDN rename would be the wrong trade. Dropping
/// is logged by the caller rather than done silently.
pub fn validate_seen(source_id: &str, seen: &[SeenListing]) -> Result<Vec<CachedListing>, String> {
    // The untrusted boundary: `source_id` is a path segment a client chose, so
    // this is where a string becomes a `Source` and stops being one.
    let source = match source_id.parse::<Source>() {
        Ok(s) if s.is_shop() => s,
        _ => return Err(format!("unknown shop: {source_id}")),
    };
    if seen.len() > MAX_SEEN {
        return Err(format!("at most {MAX_SEEN} listings per report"));
    }
    seen.iter()
        .map(|s| {
            let external_id: ExternalId = s.external_id.parse()?;
            // The shape rule belongs to the type; naming the offending value is
            // this caller's job — it's the one that knows whose report it is.
            let barcode = trimmed(&s.barcode)
                .map(|bc| {
                    bc.parse::<Barcode>()
                        .map_err(|_| format!("not a barcode: {bc}"))
                })
                .transpose()?;
            Ok(CachedListing {
                source,
                external_id,
                barcode,
                name: trimmed(&s.name),
                brand: trimmed(&s.brand),
                quantity_label: trimmed(&s.quantity_label),
                image_url: trimmed(&s.image_url)
                    .filter(|u| off::host_allowed(u, source.image_hosts())),
            })
        })
        .collect()
}

fn trimmed(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Store everything a shop query showed us, keyed by the shop's own identity.
///
/// Upserts: re-seeing a listing refreshes its description and bumps
/// `last_seen_at`. A field we've since learned is never overwritten with the
/// `NULL` of a thinner sighting — a Waitrose search hit (no barcode) must not
/// erase the barcode an earlier product fetch taught us, which is exactly the
/// silent-erasure shape that bit `product_dietary_flags` in increment 6.
pub async fn remember(pool: &MySqlPool, listings: &[CachedListing]) -> Result<()> {
    if listings.is_empty() {
        return Ok(());
    }
    // One statement, not one per listing: an Asda search hands back ~15 and the
    // round trips dominate. Same shape as `routes::telemetry::store`.
    let mut q = sqlx::QueryBuilder::new(
        "INSERT INTO shop_listings \
         (source, external_id, barcode, name, brand, quantity_label, image_url) ",
    );
    q.push_values(listings, |mut row, l| {
        row.push_bind(l.source)
            .push_bind(&l.external_id)
            .push_bind(&l.barcode)
            .push_bind(&l.name)
            .push_bind(&l.brand)
            .push_bind(&l.quantity_label)
            .push_bind(&l.image_url);
    });
    q.push(
        " ON DUPLICATE KEY UPDATE \
         barcode        = COALESCE(VALUES(barcode), barcode), \
         name           = COALESCE(VALUES(name), name), \
         brand          = COALESCE(VALUES(brand), brand), \
         quantity_label = COALESCE(VALUES(quantity_label), quantity_label), \
         image_url      = COALESCE(VALUES(image_url), image_url), \
         last_seen_at   = CURRENT_TIMESTAMP",
    );
    q.build().execute(pool).await?;
    Ok(())
}

/// "Does <source> carry <barcode>?", answered from memory alone.
///
/// `Ok(None)` means only "we don't know" — never "the shop doesn't sell it".
/// The caller decides whether to go ask; this function never does.
pub async fn find_by_barcode(
    pool: &MySqlPool,
    source: Source,
    barcode: &Barcode,
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
