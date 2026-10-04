//! What one source's account of a product may change. Pure: `repo::ingest`
//! applies these answers in one transaction.

use life::products::ingest::{
    CanonicalWrites, FactsUpdate, Held, SourceAccount, best_name, canonical_writes,
    picture_to_fetch,
};
use life::products::repo::Listing;
use life::products::source::Source;
use life::products::types::Product;

fn account(source: Source) -> SourceAccount {
    SourceAccount {
        source,
        external_id: "SRC-1".parse().unwrap(),
        barcode: None,
        name: Some("Shop Name".into()),
        brand: Some("Shop Brand".into()),
        quantity_label: Some("400G".into()),
        url: None,
        image_url: Some("https://asdagroceries.scene7.com/is/image/asdagroceries/1".into()),
        raw_json: None,
        price: None,
        facts: FactsUpdate::None,
    }
}

fn held(name: &str, brand: &str, pack: &str) -> Held {
    let some = |v: &str| (!v.is_empty()).then(|| v.to_string());
    Held {
        name: some(name),
        brand: some(brand),
        quantity_label: some(pack),
        ..Held::default()
    }
}

fn set(v: &str) -> Option<Option<String>> {
    Some(Some(v.into()))
}

#[test]
fn a_shared_product_only_has_its_gaps_filled() {
    // A barcoded product is listed by several sources; one disagreeing is a
    // divergence to approve, not a write.
    let a = account(Source::Asda);
    assert_eq!(
        canonical_writes(&held("Ours", "Our Brand", "500g"), &a),
        CanonicalWrites::default()
    );
    assert_eq!(
        canonical_writes(&held("Ours", "", "  "), &a),
        CanonicalWrites {
            name: None,
            brand: set("Shop Brand"),
            quantity_label: set("400G"),
        },
        "the name is left to the listing ranking, even when blank"
    );
}

#[test]
fn a_barcodeless_product_follows_its_one_source_except_where_it_is_ours() {
    let a = account(Source::Waitrose);
    let own = |name_ours, brand_ours| Held {
        single_owner: true,
        name_ours,
        brand_ours,
        ..held("Old", "Old Brand", "500g")
    };
    assert_eq!(
        canonical_writes(&own(false, false), &a),
        CanonicalWrites {
            name: set("Shop Name"),
            brand: set("Shop Brand"),
            quantity_label: None,
        }
    );
    assert_eq!(canonical_writes(&own(true, false), &a).name, None);
    assert_eq!(canonical_writes(&own(false, true), &a).brand, None);
    // The source is the sole authority: a brand it stops stating goes.
    let unbranded = SourceAccount {
        brand: None,
        ..account(Source::Waitrose)
    };
    assert_eq!(
        canonical_writes(&own(false, false), &unbranded).brand,
        Some(None)
    );
}

fn product(has_image: bool) -> Product {
    Product {
        id: life::products::ids::ProductId(1),
        barcode: None,
        name: None,
        brand: None,
        quantity_label: None,
        pack: None,
        source: None,
        external_id: None,
        name_source: None,
        image_source: None,
        has_image,
    }
}

#[test]
fn a_picture_is_fetched_only_for_a_product_without_one() {
    let a = account(Source::Asda);
    assert!(picture_to_fetch(None, &a).is_some(), "a new product");
    assert!(picture_to_fetch(Some(&product(false)), &a).is_some());
    assert!(picture_to_fetch(Some(&product(true)), &a).is_none());
    let user = SourceAccount {
        source: Source::User,
        ..account(Source::User)
    };
    assert!(
        picture_to_fetch(None, &user).is_none(),
        "no host to fetch from"
    );
    let blank = SourceAccount {
        image_url: Some("  ".into()),
        ..account(Source::Asda)
    };
    assert!(picture_to_fetch(None, &blank).is_none());
}

fn listing(source: Source, name: Option<&str>) -> Listing {
    Listing {
        source,
        external_id: "X".parse().unwrap(),
        url: None,
        raw_name: name.map(Into::into),
        brand: None,
        quantity_label: None,
        image_url: None,
    }
}

#[test]
fn the_best_ranked_source_names_a_product_with_no_name() {
    let listings = [
        listing(Source::Off, Some("crowd name")),
        listing(Source::Asda, Some("  ")),
        listing(Source::Waitrose, Some("Waitrose Name")),
    ];
    assert_eq!(
        best_name(&listings),
        Some(("Waitrose Name", Source::Waitrose))
    );
    // A blank title is no title, and our own layer does not compete.
    assert_eq!(
        best_name(&[
            listing(Source::Asda, Some(" ")),
            listing(Source::User, Some("ours"))
        ]),
        None
    );
}
