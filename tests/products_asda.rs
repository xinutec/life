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
      "PRICES": {
        "EN": { "PRICE": 3.57, "PRICEPERUOM": 8.925, "PRICEPERUOMFORMATTED": "£8.93/KG" },
        "SC": { "PRICE": 3.57 }
      }
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
    // Structured price: minor units (pence), never a float; per-unit for compare.
    let price = hit.price.as_ref().expect("has a structured price");
    assert_eq!(price.amount_minor, 357);
    assert_eq!(price.currency, "GBP");
    // 8.925 is stored as just-over-8.925 in f64, so round(*100) = 893 — which
    // also matches Asda's own displayed "£8.93/KG".
    assert_eq!(price.unit_amount_minor, Some(893));
    assert_eq!(price.unit_measure.as_deref(), Some("KG"));
    assert_eq!(price.region.as_deref(), Some("EN"));
    assert_eq!(
        hit.image_url.as_deref(),
        Some("https://asdagroceries.scene7.com/is/image/asdagroceries/5740900404465?$ProdList$")
    );
}

// A real hit's NUTRITIONAL_INFO block: Asda ships ALL its lifestyle tags on
// every product and sets the ones it claims. Trimmed here, values verbatim.
const OAT_DRINK: &str = r#"{
  "results": [{
    "hits": [{
      "CIN": "9100001",
      "NAME": "Chocolate High Fibre Vegan Oat Drink 330ml",
      "IMAGE_ID": "5050854000001",
      "PACK_SIZE": "330ML",
      "NUTRITIONAL_INFO": {
        "Vegan": 1, "Vegetarian": 1, "Halal": 0, "Kosher": 0,
        "NoGluten": 0, "NoLactose": 1, "NoNuts": 1,
        "LowSalt": 1, "LowFat": 1, "HighFibre": 0
      }
    }]
  }]
}"#;

#[test]
fn lifestyle_tags_become_dietary_flags() {
    let hits = asda::parse_hits(OAT_DRINK).expect("parses");
    let flags: Vec<(&str, &str)> = hits[0]
        .dietary
        .iter()
        .map(|d| (d.flag.as_str(), d.value.as_str()))
        .collect();
    // Only the claimed tags we have a slug for. NoNuts/LowSalt/LowFat have no
    // slug in our vocabulary yet and are dropped rather than invented; Halal,
    // Kosher and NoGluten are 0 — not claimed — so they assert nothing.
    assert_eq!(
        flags,
        vec![
            ("vegan", "yes"),
            ("vegetarian", "yes"),
            ("lactose_free", "yes")
        ]
    );
    assert_eq!(hits[0].quantity_label.as_deref(), Some("330ML"));
}

#[test]
fn an_unset_lifestyle_tag_is_never_read_as_a_no() {
    // Quaker Oat So Simple really does ship Vegetarian: 0 — oats plainly are
    // vegetarian, so 0 means "not claimed", not "no". Asserting a negative here
    // would have the app contradicting the pack.
    let hits = asda::parse_hits(
        r#"{ "results": [{ "hits": [{ "CIN": "9346702", "NAME": "Oat So Simple",
             "NUTRITIONAL_INFO": { "Vegetarian": 0, "Vegan": 0 } }] }] }"#,
    )
    .expect("parses");
    assert!(
        hits[0].dietary.is_empty(),
        "an unclaimed tag must assert nothing, got {:?}",
        hits[0].dietary
    );
}

#[test]
fn a_hit_with_no_lifestyle_block_has_no_flags() {
    let hits = asda::parse_hits(RESPONSE).expect("parses");
    assert!(hits[0].dietary.is_empty());
}

#[test]
fn a_hit_without_a_price_has_none() {
    let hits =
        asda::parse_hits(r#"{ "results": [{ "hits": [{ "CIN": "1", "NAME": "No price" }] }] }"#)
            .expect("parses");
    assert!(hits[0].price.is_none());
    assert!(hits[0].price_label.is_none());
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
