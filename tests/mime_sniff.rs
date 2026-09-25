//! The two magic-byte readers must not disagree.
//!
//! `files::types::sniff_mime` and `products::off::sniff_image_mime` answer the
//! same question — what are these bytes — for two callers with different
//! allowlists. These tests pin the part that must be identical, and state the
//! part that must differ.

use life::files::types::sniff_mime;
use life::products::off::sniff_image_mime;

/// Real AVIF encoders set the MAJOR brand to `mif1`/`msf1` and list `avif` only
/// among the compatible brands. Matching the major brand alone reads such a file
/// as HEIC, because `mif1` is also HEIC's.
#[test]
fn an_avif_whose_major_brand_is_mif1_is_avif_on_both_paths() {
    let avif = b"\x00\x00\x00\x1cftypmif1\x00\x00\x00\x00mif1avif";
    assert_eq!(sniff_image_mime(avif), Some("image/avif"));
    assert_eq!(sniff_mime(avif), Some("image/avif"));
}

/// A real HEIC lists `heic`/`heix` and never `avif`, so widening the AVIF test
/// must not start reading iPhone photos as AVIF.
#[test]
fn a_heic_is_still_heic_on_the_files_path_and_still_refused_for_products() {
    for heic in [
        &b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00heic"[..],
        &b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00mif1heic"[..],
    ] {
        assert_eq!(sniff_mime(heic), Some("image/heic"), "files path");
        assert_eq!(sniff_image_mime(heic), None, "product images refuse HEIC");
    }
}

/// The shared part: for every signature both callers accept, they must name the
/// same type. The allowlists differ deliberately — files takes PDF and HEIC,
/// product images take neither — so only agreement is asserted here, never
/// acceptance.
#[test]
fn the_two_readers_never_name_the_same_bytes_differently() {
    let cases: &[&[u8]] = &[
        &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0],
        b"GIF87a....",
        b"GIF89a....",
        b"RIFF\x10\x00\x00\x00WEBPVP8 ",
        b"\x00\x00\x00\x20ftypavif....",
        b"\x00\x00\x00\x20ftypavis....",
        b"\x00\x00\x00\x1cftypmif1\x00\x00\x00\x00mif1avif",
        // Neither may claim these.
        b"<svg xmlns='http://www.w3.org/2000/svg'>",
        b"<!doctype html><script>alert(1)</script>",
        b"",
        b"RIFF\x10\x00\x00\x00WAVE",
        b"\x00\x00\x00\x20ftypmp42....",
    ];
    for bytes in cases {
        if let (Some(files), Some(products)) = (sniff_mime(bytes), sniff_image_mime(bytes)) {
            assert_eq!(files, products, "the two readers disagree about {bytes:?}");
        }
    }
}
