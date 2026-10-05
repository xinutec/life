//! Every listing a shop query has shown us, so later lookups need not ask again.
//! Not the catalogue: a row joins `product_listings` only when attached. Served
//! until the user refreshes.

use anyhow::Result;
use serde::Deserialize;
use sqlx::MySqlPool;
use ts_rs::TS;

use super::asda::AsdaHit;
use super::ids::{Barcode, ExternalId};
use super::off;
use super::source::Source;

/// One listing as the shop described it, from Asda's search or the phone's
/// WebView. Field order matches `find_by_barcode`'s SELECT.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct CachedListing {
    pub source: Source,
    pub external_id: ExternalId,
    /// `None` is "not learned yet": a Waitrose search hit has no barcode.
    pub barcode: Option<Barcode>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub quantity_label: Option<String>,
    pub image_url: Option<String>,
}

impl CachedListing {
    /// Every Asda hit teaches a barcode → CIN mapping.
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

/// A listing a phone's WebView reported; untrusted until [`validate_seen`].
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

/// A search returns 8 to 15; near this is a client bug.
pub const MAX_SEEN: usize = 50;

/// Cache rows from a client's report. An unknown shop, malformed id or bad barcode
/// rejects the batch; an image from a disallowed host is dropped.
pub fn validate_seen(source_id: &str, seen: &[SeenListing]) -> Result<Vec<CachedListing>, String> {
    // Where a client's path segment becomes a `Source`.
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

/// Upserts, never overwriting a learned field with a thinner sighting's `NULL`.
pub async fn remember(pool: &MySqlPool, listings: &[CachedListing]) -> Result<()> {
    if listings.is_empty() {
        return Ok(());
    }
    // One statement: the round trips dominate.
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

/// From memory only. `Ok(None)` means "unknown", never "not sold".
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
