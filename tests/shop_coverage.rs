//! Where a Buy-list row is known to be sold — the pure fold behind
//! POST /api/shopping/coverage. No DB: these pin the rule, not the queries.

use life::products::coverage::{CoverageQuery, barcodes, combine, product_ids};

fn row(key: &str, barcode: Option<&str>, product_id: Option<u64>) -> CoverageQuery {
    CoverageQuery {
        key: key.to_string(),
        barcode: barcode.map(str::to_string),
        product_id,
    }
}

#[test]
fn a_held_listing_says_the_shop_sells_it() {
    let q = [row("a", None, Some(42))];
    let got = combine(&q, &[(42, "asda".into())], &[]);
    assert_eq!(got[0].key, "a");
    assert_eq!(got[0].sources, ["asda"]);
}

#[test]
fn a_sighting_counts_too_even_with_no_listing_of_our_own() {
    // The whole point of remembering shop queries: a barcode we've seen at a
    // shop answers the trip question without anything being attached.
    let q = [row("a", Some("5000169146767"), None)];
    let got = combine(&q, &[], &[("5000169146767".into(), "waitrose".into())]);
    assert_eq!(got[0].sources, ["waitrose"]);
}

#[test]
fn a_shop_that_both_holds_and_has_been_seen_is_one_answer() {
    let q = [row("a", Some("5000169146767"), Some(42))];
    let got = combine(
        &q,
        &[(42, "asda".into())],
        &[("5000169146767".into(), "asda".into())],
    );
    assert_eq!(got[0].sources, ["asda"]);
}

#[test]
fn shops_come_back_sorted_so_the_display_does_not_shuffle() {
    let q = [row("a", Some("5000169146767"), Some(42))];
    let got = combine(
        &q,
        &[(42, "waitrose".into())],
        &[("5000169146767".into(), "asda".into())],
    );
    assert_eq!(got[0].sources, ["asda", "waitrose"]);
}

#[test]
fn a_row_we_know_nothing_about_gets_an_empty_list_not_a_missing_row() {
    // "We don't know" and "nowhere sells it" are different claims; the row has
    // to come back either way so the client can say which.
    let q = [row("hand-typed", None, None)];
    let got = combine(&q, &[(42, "asda".into())], &[]);
    assert_eq!(got.len(), 1);
    assert!(got[0].sources.is_empty());
}

#[test]
fn every_row_is_answered_in_the_order_it_was_asked() {
    let q = [
        row("a", None, Some(42)),
        row("b", None, None),
        row("c", Some("5000169146767"), None),
    ];
    let got = combine(
        &q,
        &[(42, "asda".into())],
        &[("5000169146767".into(), "waitrose".into())],
    );
    assert_eq!(
        got.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert!(got[1].sources.is_empty());
}

#[test]
fn one_products_shops_never_leak_onto_another_row() {
    let q = [row("a", None, Some(42)), row("b", None, Some(43))];
    let got = combine(&q, &[(42, "asda".into()), (43, "waitrose".into())], &[]);
    assert_eq!(got[0].sources, ["asda"]);
    assert_eq!(got[1].sources, ["waitrose"]);
}

#[test]
fn the_keys_to_query_are_deduped() {
    // Two rows for the same product ask the DB once, not twice.
    let q = [
        row("a", Some("5000169146767"), Some(42)),
        row("b", Some("5000169146767"), Some(42)),
    ];
    assert_eq!(product_ids(&q), [42]);
    assert_eq!(barcodes(&q), ["5000169146767"]);
}

#[test]
fn a_blank_barcode_is_never_queried() {
    // `barcode = ''` would match every other blank row in the cache, so an empty
    // string has to be absence, not a value.
    let q = [row("a", Some("   "), None), row("b", Some(""), None)];
    assert!(barcodes(&q).is_empty());
}

#[test]
fn nothing_to_ask_about_means_no_query_at_all() {
    let q = [row("a", None, None)];
    assert!(product_ids(&q).is_empty());
    assert!(barcodes(&q).is_empty());
}
