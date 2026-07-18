//! 8f — reconciling the product picture. Unlike the scalar fields, the canonical
//! image is bytes we hold while a listing offers a URL, so the divergence is by
//! *provenance* (which source our picture came from), not by value. The rule is
//! pure (no DB); the settle/provenance round-trip runs against a real MariaDB
//! only when LIFE_TEST_DATABASE_URL is set. Adopting a picture re-fetches through
//! the SSRF gate and is exercised at the route layer, not here.

use std::collections::HashMap;

use life::db;
use life::products::repo::{self, Listing};
use life::products::types::Product;

fn product(image_source: Option<&str>, has_image: bool) -> Product {
    Product {
        id: 1,
        barcode: Some("5000000000789".into()),
        name: Some("Name".into()),
        brand: Some("Brand".into()),
        quantity_label: Some("500g".into()),
        source: Some("off".into()),
        external_id: None,
        name_source: Some("off".into()),
        image_source: image_source.map(str::to_string),
        has_image,
    }
}

fn listing(source: &str, image_url: Option<&str>) -> Listing {
    Listing {
        source: source.into(),
        external_id: format!("{source}-cin"),
        url: None,
        raw_name: Some("Name".into()),
        brand: Some("Brand".into()),
        quantity_label: Some("500g".into()),
        image_url: image_url.map(str::to_string),
    }
}

/// Recompute the live picture divergence from the DB rows (as the detail endpoint
/// does), for the round-trip test.
async fn picture_divergence(
    pool: &sqlx::MySqlPool,
    id: u64,
) -> Option<life::products::types::FieldDivergence> {
    let listings = repo::listings_for(pool, id).await.unwrap();
    let decisions = repo::field_decisions(pool, id).await.unwrap();
    let product = repo::get_by_id(pool, id).await.unwrap().unwrap();
    repo::picture_divergence(&product, &listings, &decisions)
}

#[test]
fn a_picture_from_another_source_surfaces() {
    // We hold OFF's picture; Asda offers its own → a candidate to adopt.
    let p = product(Some("off"), true);
    let asda = listing("asda", Some("https://asda.example/x.jpg"));
    let off = listing("off", Some("https://off.example/y.jpg"));
    let d = repo::picture_divergence(&p, &[off, asda], &HashMap::new())
        .expect("a differing-source picture surfaces");
    assert_eq!(d.field, "picture");
    // Only the OTHER source is a candidate; the one we already hold is not.
    assert_eq!(d.candidates.len(), 1);
    assert_eq!(d.candidates[0].source, "asda");
    assert_eq!(d.candidates[0].value, "https://asda.example/x.jpg");
    assert_eq!(
        d.current.as_deref(),
        Some("off"),
        "provenance of what we hold"
    );
}

#[test]
fn the_source_we_already_hold_is_not_a_candidate() {
    // Only the source our picture came from offers one → nothing to adopt.
    let p = product(Some("asda"), true);
    let asda = listing("asda", Some("https://asda.example/x.jpg"));
    assert!(
        repo::picture_divergence(&p, &[asda], &HashMap::new()).is_none(),
        "same-source picture raises no divergence"
    );
}

#[test]
fn a_hand_uploaded_picture_is_never_nagged() {
    // Our own upload outranks every source and is never asked to be replaced.
    let p = product(Some("user"), true);
    let asda = listing("asda", Some("https://asda.example/x.jpg"));
    assert!(
        repo::picture_divergence(&p, &[asda], &HashMap::new()).is_none(),
        "a user picture is ours — no source can nag it"
    );
}

#[test]
fn with_no_picture_any_source_is_a_candidate() {
    // Nothing held yet: a source picture is an offer to fill, current is None.
    let p = product(None, false);
    let asda = listing("asda", Some("https://asda.example/x.jpg"));
    let d = repo::picture_divergence(&p, &[asda], &HashMap::new())
        .expect("a source picture surfaces when we have none");
    assert_eq!(d.candidates.len(), 1);
    assert_eq!(d.current, None, "no picture held → no current provenance");
}

#[test]
fn a_listing_without_a_picture_offers_nothing() {
    let p = product(Some("off"), true);
    let asda = listing("asda", None);
    assert!(
        repo::picture_divergence(&p, &[asda], &HashMap::new()).is_none(),
        "a source with no image_url is not a picture candidate"
    );
}

#[tokio::test]
async fn settle_picture_quiets_it_until_a_url_changes() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping picture DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000790";
    let (off_ext, asda_ext) = ("pictest-off", "pictest-asda");
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [off_ext, asda_ext] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // OFF seeds the product and lists a picture; Asda lists a different one.
    let p = repo::upsert_external(
        &pool,
        "off",
        off_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("name"),
            image_url: Some("https://off.example/y.jpg"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Name"),
            image_url: Some("https://asda.example/x.jpg"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Pretend we hold OFF's picture (bypassing the SSRF fetch): 1 byte + provenance.
    repo::set_image_by_id(&pool, p.id, &[0x42], "image/jpeg")
        .await
        .unwrap();
    repo::set_image_provenance(&pool, p.id, "off")
        .await
        .unwrap();

    // Asda's picture is a candidate against the OFF one we hold.
    let d = picture_divergence(&pool, p.id)
        .await
        .expect("picture divergence before settle");
    assert_eq!(d.candidates[0].source, "asda");

    // Keep OFF's: settling records the current set → the divergence goes quiet.
    repo::settle_picture(&pool, p.id).await.unwrap();
    assert!(
        picture_divergence(&pool, p.id).await.is_none(),
        "a settled picture stays quiet while nothing changes"
    );

    // Asda changes its picture URL → the set changes → it resurfaces.
    repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Name"),
            image_url: Some("https://asda.example/x2.jpg"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let d = picture_divergence(&pool, p.id)
        .await
        .expect("a changed source picture re-surfaces the divergence");
    assert_eq!(d.candidates[0].value, "https://asda.example/x2.jpg");
}
