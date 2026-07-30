//! What a client's WebView may teach the shop cache (increment 7b).
//!
//! Pure — no DB. The rule these tests pin is which reports are trustworthy
//! enough to enter the `(source, barcode)` identity index, and which parts of a
//! report can be dropped without losing the row.

use life::products::shop_cache::{MAX_SEEN, SeenListing, validate_seen};
use life::products::source::Source;

fn seen(external_id: &str, barcode: Option<&str>) -> SeenListing {
    SeenListing {
        external_id: external_id.to_string(),
        barcode: barcode.map(str::to_string),
        name: Some("Waitrose Balsamic Vinegar of Modena".to_string()),
        brand: Some("Waitrose".to_string()),
        quantity_label: Some("250ml".to_string()),
        image_url: Some(
            "https://ecom-su-static-prod.wtrecom.com/images/products/3/LN_271105_BP_3.jpg"
                .to_string(),
        ),
    }
}

#[test]
fn a_product_the_phone_fetched_becomes_a_cache_row() {
    let rows = validate_seen("waitrose", &[seen("271105", Some("5000169146767"))]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, Source::Waitrose);
    assert_eq!(rows[0].external_id, "271105");
    assert_eq!(rows[0].barcode.as_deref(), Some("5000169146767"));
    assert_eq!(rows[0].quantity_label.as_deref(), Some("250ml"));
    assert!(rows[0].image_url.is_some());
}

#[test]
fn a_search_hit_with_no_barcode_yet_is_still_worth_remembering() {
    // Waitrose search hits carry no EAN — the lineNumber and name alone still
    // save the next hunt a page load, and a later fetch fills the barcode in.
    let rows = validate_seen("waitrose", &[seen("271105", None)]).unwrap();
    assert_eq!(rows[0].barcode, None);
    assert_eq!(rows[0].external_id, "271105");
}

#[test]
fn a_report_from_a_shop_we_do_not_know_is_refused() {
    let err = validate_seen("tesco", &[seen("1", Some("5000169146767"))]).unwrap_err();
    assert!(err.contains("tesco"), "{err}");
}

#[test]
fn a_barcode_that_is_not_a_barcode_kills_the_whole_report() {
    // The `(source, barcode)` lookup is the one thing this table exists to
    // answer, so a wrong row there is worse than no row at all.
    let err = validate_seen("waitrose", &[seen("271105", Some("not-an-ean"))]).unwrap_err();
    assert!(err.contains("not a barcode"), "{err}");
}

#[test]
fn a_shop_id_that_could_not_be_a_shop_id_is_refused() {
    let err = validate_seen("waitrose", &[seen("../../etc/passwd", None)]).unwrap_err();
    assert!(err.contains("external_id"), "{err}");
}

#[test]
fn an_empty_shop_id_is_refused() {
    let err = validate_seen("waitrose", &[seen("   ", None)]).unwrap_err();
    assert!(err.contains("external_id"), "{err}");
}

#[test]
fn an_image_from_somewhere_the_shop_does_not_serve_pictures_is_dropped_not_fatal() {
    // Identity is the point and the picture is a nicety: a CDN we don't
    // allowlist costs the row its image, not its place in the index.
    let mut s = seen("271105", Some("5000169146767"));
    s.image_url = Some("https://evil.example/pic.jpg".to_string());
    let rows = validate_seen("waitrose", &[s]).unwrap();
    assert_eq!(rows[0].image_url, None);
    assert_eq!(rows[0].external_id, "271105");
}

#[test]
fn an_http_image_url_on_an_allowlisted_host_is_still_dropped() {
    // Same guard as the import path: https only.
    let mut s = seen("271105", None);
    s.image_url = Some("http://ecom-su-static-prod.wtrecom.com/x.jpg".to_string());
    assert_eq!(validate_seen("waitrose", &[s]).unwrap()[0].image_url, None);
}

#[test]
fn blank_text_is_absence_not_an_empty_string() {
    let mut s = seen("271105", None);
    s.brand = Some("  ".to_string());
    s.name = Some(String::new());
    let rows = validate_seen("waitrose", &[s]).unwrap();
    assert_eq!(rows[0].brand, None);
    assert_eq!(rows[0].name, None);
}

#[test]
fn an_implausibly_large_report_is_refused() {
    // A Waitrose search returns 8 and an Asda search 15; a client sending
    // hundreds is a bug, and this table is not a place to bulk-load a catalogue.
    let many: Vec<SeenListing> = (0..=MAX_SEEN)
        .map(|i| seen(&format!("ln{i}"), None))
        .collect();
    assert!(validate_seen("waitrose", &many).is_err());
}

#[test]
fn asda_may_report_too_even_though_the_server_can_search_it() {
    // The endpoint is shop-agnostic on purpose: what differs between shops is
    // who can see the page, not what a sighting means.
    let rows = validate_seen("asda", &[seen("9346702", Some("5000169146767"))]).unwrap();
    assert_eq!(rows[0].source, Source::Asda);
}
