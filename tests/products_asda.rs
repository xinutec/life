//! Asda storefront search normalization — exercised through the public
//! `parse_hits`, the pure half of `products::asda::search`. Fixtures are real
//! `ASDA_PRODUCTS` Algolia responses (trimmed to the fields we read), so these
//! assertions are hermetic: no network, no DB.

use life::products::asda;

// A real multi-query response: one product hit (Lurpak). IMAGE_ID is the EAN;
// PRICES is region-keyed (EN/SC/…).
const RESPONSE: &str = r#"{
  "results": [{
    "hits": [{
      "CIN": "7690049",
      "objectID": "7690049",
      "NAME": "Slightly Salted Spreadable Blend of Butter and Rapeseed Oil 400g",
      "BRAND": "Lurpak",
      "IMAGE_ID": "5740900404465",
      "PACK_SIZE": "400G",
      "PRICES": { "EN": { "PRICE": 3.57 }, "SC": { "PRICE": 3.57 } }
    }]
  }]
}"#;

#[test]
fn maps_a_real_response() {
    let hits = asda::parse_hits(RESPONSE).expect("parses");
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.external_id, "7690049");
    assert_eq!(
        hit.name,
        "Slightly Salted Spreadable Blend of Butter and Rapeseed Oil 400g"
    );
    assert_eq!(hit.brand.as_deref(), Some("Lurpak"));
    // IMAGE_ID is the primary EAN — a genuine barcode we can carry.
    assert_eq!(hit.barcode.as_deref(), Some("5740900404465"));
    assert_eq!(hit.quantity_label.as_deref(), Some("400G"));
    assert_eq!(hit.price_label.as_deref(), Some("£3.57")); // England price, formatted
    assert_eq!(
        hit.image_url.as_deref(),
        Some("https://asdagroceries.scene7.com/is/image/asdagroceries/5740900404465?$ProdList$")
    );
}

#[test]
fn falls_back_to_object_id_and_tolerates_missing_price() {
    let hits = asda::parse_hits(
        r#"{ "results": [{ "hits": [{ "objectID": "42", "NAME": "Thing", "PRICES": {} }] }] }"#,
    )
    .expect("parses");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].external_id, "42");
    assert!(hits[0].barcode.is_none());
    assert!(hits[0].price_label.is_none());
}

#[test]
fn drops_hits_without_a_usable_identity() {
    // No id; blank name; and an id that would fail POST /api/products/import's
    // [A-Za-z0-9_-] check (external_id is spliced into that call) — all dropped.
    let hits = asda::parse_hits(
        r#"{ "results": [{ "hits": [
            { "NAME": "No id" },
            { "CIN": "1", "NAME": "  " },
            { "CIN": "a/b", "NAME": "Slashy" }
        ] }] }"#,
    )
    .expect("parses");
    assert!(hits.is_empty());
}

#[test]
fn keeps_the_image_but_not_the_barcode_for_non_ean_image_ids() {
    // Promo/asset image ids aren't EANs: usable as an image key, not a barcode.
    let hits = asda::parse_hits(
        r#"{ "results": [{ "hits": [{ "CIN": "9", "NAME": "Promo", "IMAGE_ID": "250130_Banner" }] }] }"#,
    )
    .expect("parses");
    assert_eq!(hits.len(), 1);
    assert!(hits[0].barcode.is_none());
    assert_eq!(
        hits[0].image_url.as_deref(),
        Some("https://asdagroceries.scene7.com/is/image/asdagroceries/250130_Banner?$ProdList$")
    );
}

#[test]
fn empty_results_yield_no_hits() {
    assert!(
        asda::parse_hits(r#"{ "results": [] }"#)
            .expect("parses")
            .is_empty()
    );
    assert!(
        asda::parse_hits(r#"{ "results": [{ "hits": [] }] }"#)
            .expect("parses")
            .is_empty()
    );
}
