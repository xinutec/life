//! Attachments.

use chrono::{DateTime, Utc};

use crate::media::{self, Media};
use serde::Serialize;
use ts_rs::TS;

crate::row_id! {
    /// `item_files.id`.
    FileId
}

/// Metadata only: listing five receipts must not read five blobs.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[ts(export)]
pub struct ItemFile {
    #[ts(type = "number")]
    pub id: FileId,
    #[ts(type = "number")]
    pub item_id: crate::inventory::types::ItemId,
    /// A receipt's purchase; `None` for a manual, which belongs to the thing.
    #[ts(type = "number | null")]
    pub purchase_id: Option<crate::purchases::types::PurchaseId>,
    pub name: String,
    pub mime: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Unix milliseconds on the wire.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    #[ts(type = "number")]
    pub created_at: DateTime<Utc>,
}

/// Past this, somebody is storing the wrong thing here.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// By sniffed bytes: nothing executable, as files are served from our origin.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    // Exhaustive, so a new variant must be decided here.
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
