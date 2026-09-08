//! What a file's leading bytes say it is.
//!
//! Two callers ask this question with different allowlists: item attachments
//! take everything here, product images take neither PDF nor HEIC. They used to
//! ask it with two separate `match` blocks, and the ISO-BMFF arms drifted — a
//! real AVIF read as HEIC on one path and AVIF on the other (#1448). One table,
//! and the allowlist stays at the caller.

/// A type identifiable from magic bytes alone.
///
/// Callers match on this exhaustively, so a new variant cannot join the sniffer
/// without every allowlist deciding what to do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Media {
    Jpeg,
    Png,
    Gif,
    Webp,
    Avif,
    Heic,
    Pdf,
}

impl Media {
    /// The media type stored in the database and sent on the wire.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
            Self::Heic => "image/heic",
            Self::Pdf => "application/pdf",
        }
    }
}

/// Name the bytes, or refuse them.
///
/// Signatures must match at the START: a file that merely mentions `%PDF-`
/// somewhere is not a PDF. Anything with an executable interpretation — SVG and
/// HTML above all — is absent by construction rather than filtered afterwards,
/// because these bytes are served back on our own origin.
pub fn sniff(bytes: &[u8]) -> Option<Media> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => return Some(Media::Jpeg),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => return Some(Media::Png),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => return Some(Media::Gif),
        // "%PDF-", the only signature a PDF is required to start with.
        [b'%', b'P', b'D', b'F', b'-', ..] => return Some(Media::Pdf),
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
        ] => {
            return Some(Media::Webp);
        }
        _ => {}
    }

    // ISO-BMFF: "<4-byte size>ftyp<major brand><compatible brands…>".
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        // ⚠ AVIF is tested BEFORE HEIC, and by scanning the whole box rather
        // than reading the major brand. Real encoders set the major brand to
        // `mif1`/`msf1` and list `avif` only among the compatible brands — and
        // `mif1` is HEIC's brand too, so a major-brand match calls such a file
        // HEIC. That was #1448. A genuine HEIC lists `heic`/`heix` and never
        // `avif`, so the wider test cannot capture one.
        let end = bytes.len().min(64);
        if bytes[8..end]
            .windows(4)
            .any(|w| w == b"avif" || w == b"avis")
        {
            return Some(Media::Avif);
        }
        if matches!(&bytes[8..12], b"heic" | b"mif1") {
            return Some(Media::Heic);
        }
    }
    None
}
