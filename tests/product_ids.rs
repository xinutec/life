//! What a product identifier is allowed to be.
//!
//! These rules used to be re-implemented at each boundary that read one — the
//! import route, the shop-sighting report, Asda's hit normaliser, the Open Food
//! Facts client — so they lived here as tests of the *route's* guard. They now
//! live in the types themselves ([`Barcode`], [`ExternalId`]), and so do their
//! tests: every caller inherits whatever this file pins.
//!
//! The traversal and query-parameter cases below are the ones that matter, since
//! both types are spliced directly into outbound URLs.

use life::products::ids::{Barcode, ExternalId};

#[test]
fn a_barcode_is_one_to_fourteen_digits() {
    assert_eq!(
        "5000112548167".parse::<Barcode>().unwrap().as_str(),
        "5000112548167"
    );
    assert_eq!("7".parse::<Barcode>().unwrap().as_str(), "7");
    assert_eq!(
        "12345678901234".parse::<Barcode>().unwrap().as_str(),
        "12345678901234"
    );
}

#[test]
fn nothing_that_could_reshape_a_url_is_a_barcode() {
    // The Open Food Facts lookup formats a barcode straight into its URL, so a
    // value that could add a path segment or a query parameter must not exist.
    for bad in [
        "",
        "abc",
        "123456789012345",
        "12/34",
        "../x",
        "123?fields=all",
        "../../secret",
    ] {
        assert!(
            bad.parse::<Barcode>().is_err(),
            "expected {bad:?} to be refused"
        );
    }
}

#[test]
fn an_external_id_is_the_shops_own_id_shape() {
    assert_eq!("9346702".parse::<ExternalId>().unwrap().as_str(), "9346702");
    assert_eq!("271105".parse::<ExternalId>().unwrap().as_str(), "271105");
    // Hyphens and underscores are in: our own fixtures and some shops use them.
    assert_eq!(
        "listtest-bl_aaa".parse::<ExternalId>().unwrap().as_str(),
        "listtest-bl_aaa"
    );
    assert_eq!(
        "x".repeat(64).parse::<ExternalId>().unwrap().as_str().len(),
        64
    );
}

#[test]
fn nothing_that_could_reshape_a_url_is_an_external_id() {
    // Same splice, same rule: `Source::listing_url` formats one into a shop's
    // product-page URL with no guard of its own.
    for bad in ["", "   ", "../../etc/passwd", "a/b", "a?b=c", "a b", "a.b"] {
        assert!(
            bad.parse::<ExternalId>().is_err(),
            "expected {bad:?} to be refused"
        );
    }
    assert!(
        "x".repeat(65).parse::<ExternalId>().is_err(),
        "65 is too long"
    );
}

#[test]
fn surrounding_space_is_transport_not_identity() {
    // Every boundary that built one used to trim first; the type does it now, so
    // a space picked up in transit doesn't become a different product.
    assert_eq!(
        "  5000112548167 ".parse::<Barcode>().unwrap().as_str(),
        "5000112548167"
    );
    assert_eq!(" 271105 ".parse::<ExternalId>().unwrap().as_str(), "271105");
}

#[test]
fn every_barcode_is_also_a_well_formed_external_id() {
    // Open Food Facts keys its listing by the barcode itself, which is only
    // sound while digits stay a subset of the external-id character set and 14
    // stays under 64. This is that assumption, written down.
    let bc: Barcode = "5000112548167".parse().unwrap();
    assert_eq!(ExternalId::from(&bc).as_str(), bc.as_str());
}
