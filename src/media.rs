//! What a file's leading bytes say it is; each caller keeps its own allowlist.

/// Matched exhaustively, so every allowlist decides about a new variant.
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

/// Signatures match at the start only. Nothing executable (SVG, HTML) exists
/// here: these bytes are served from our origin.
pub fn sniff(bytes: &[u8]) -> Option<Media> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => return Some(Media::Jpeg),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, ..] => return Some(Media::Png),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => return Some(Media::Gif),
        [b'%', b'P', b'D', b'F', b'-', ..] => return Some(Media::Pdf),
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

    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        // AVIF before HEIC, over the whole box: AVIF shares HEIC's major brands
        // and lists `avif` only among the compatible ones.
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
