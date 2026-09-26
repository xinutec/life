//! What an attachment looks like on the wire, and what may be one.

use chrono::{DateTime, Utc};

use crate::media::{self, Media};
use serde::Serialize;
use ts_rs::TS;

/// An attachment's metadata, WITHOUT its bytes.
///
/// Listing and downloading are separate routes for this reason: a list of five
/// receipts should not read five blobs out of the database to tell you their
/// names. `size_bytes` is stored rather than derived for the same reason.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct ItemFile {
    #[ts(type = "number")]
    pub id: u64,
    #[ts(type = "number")]
    pub item_id: u64,
    /// Set when this file is evidence of a particular purchase — a receipt.
    /// `None` for a manual, which belongs to the thing rather than to any one
    /// time you bought it.
    #[ts(type = "number | null")]
    pub purchase_id: Option<u64>,
    pub name: String,
    pub mime: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
}

/// 10 MiB. A phone photo of a receipt is 2–4 MiB and a scanned appliance manual
/// is rarely more; twice the product-image limit because a PDF is not a
/// thumbnail. Not a technical bound — the point past which somebody is storing
/// the wrong thing here.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// What may be attached, by sniffed bytes. An allowlist, because files are served
/// from our origin and anything executable (SVG) would be stored XSS; it takes
/// PDF and HEIC, for receipts and iPhone photos.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    // Exhaustive rather than `.map(Media::mime)`, so a new variant must be
    // decided here.
    let media = media::sniff(bytes)?;
    match media {
        Media::Jpeg
        | Media::Png
        | Media::Gif
        | Media::Webp
        | Media::Avif
        | Media::Heic
        | Media::Pdf => Some(media.mime()),
    }
}
