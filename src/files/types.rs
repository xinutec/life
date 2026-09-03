//! What an attachment looks like on the wire, and what may be one.

use chrono::{DateTime, Utc};
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

/// What may be attached, by SNIFFED bytes.
///
/// ⚠ Deliberately no `image/svg+xml`, for the reason the product-image
/// allowlist gives: SVG can carry script, these are served back on our own
/// origin, and an SVG upload would be stored XSS for anyone opening the file
/// URL. The same argument applies to anything else with an executable
/// interpretation, which is why this is an allowlist and not a denylist.
///
/// PDF is here and is the reason this list exists separately from the image
/// one — a manual is a PDF, and that is the whole point of per-item files.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some("image/gif"),
        // "%PDF-", the only signature a PDF is required to start with.
        [b'%', b'P', b'D', b'F', b'-', ..] => Some("application/pdf"),
        // RIFF container: "RIFF" <4-byte size> "WEBP".
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        // ISO-BMFF: <4-byte size> "ftyp" then a brand. HEIC is what an iPhone
        // produces by default, so refusing it would refuse the commonest way a
        // receipt actually gets photographed.
        [
            _,
            _,
            _,
            _,
            b'f',
            b't',
            b'y',
            b'p',
            b'h',
            b'e',
            b'i',
            b'c',
            ..,
        ]
        | [
            _,
            _,
            _,
            _,
            b'f',
            b't',
            b'y',
            b'p',
            b'm',
            b'i',
            b'f',
            b'1',
            ..,
        ] => Some("image/heic"),
        [
            _,
            _,
            _,
            _,
            b'f',
            b't',
            b'y',
            b'p',
            b'a',
            b'v',
            b'i',
            b'f',
            ..,
        ] => Some("image/avif"),
        _ => None,
    }
}
