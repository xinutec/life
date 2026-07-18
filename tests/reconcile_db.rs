//! Reconciliation: surfacing where a product's sources disagree with its
//! canonical row, and settling those disagreements by adopting or keeping a
//! value. The `divergences` rule is pure (no DB); the reconcile round-trip runs
//! against a real MariaDB only when LIFE_TEST_DATABASE_URL is set.

use std::collections::HashMap;

use life::db;
use life::products::repo::{self, Listing};
use life::products::types::Product;

fn product(name: &str, brand: &str, quantity: &str) -> Product {
    Product {
        id: 1,
        barcode: Some("5000000000123".into()),
        name: Some(name.into()),
        brand: Some(brand.into()),
        quantity_label: Some(quantity.into()),
        source: Some("off".into()),
        external_id: None,
        name_source: Some("off".into()),
        has_image: false,
    }
}

fn listing(source: &str, name: &str, brand: &str, quantity: &str) -> Listing {
    Listing {
        source: source.into(),
        external_id: format!("{source}-cin"),
        url: None,
        raw_name: Some(name.into()),
        brand: Some(brand.into()),
        quantity_label: Some(quantity.into()),
        image_url: None,
    }
}

#[test]
fn divergences_flag_each_field_a_source_disagrees_on() {
    let p = product("Off Crowd Name", "OFF Brand", "500g");
    // Asda disagrees on all three fields (name, brand, and pack-size casing).
    let asda = listing("asda", "Clean Asda Name", "Asda Brand", "500G");
    let divs = repo::divergences(&p, &[asda], &HashMap::new());

    let fields: Vec<&str> = divs.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(fields, vec!["name", "brand", "quantity_label"]);
    let brand = divs.iter().find(|d| d.field == "brand").unwrap();
    assert_eq!(brand.current.as_deref(), Some("OFF Brand"));
    assert_eq!(brand.candidates.len(), 1);
    assert_eq!(brand.candidates[0].source, "asda");
    assert_eq!(brand.candidates[0].value, "Asda Brand");
}

#[test]
fn a_source_that_agrees_is_not_a_divergence() {
    let p = product("Name", "Brand", "500g");
    // Same brand, same pack; only the name differs.
    let asda = listing("asda", "A Different Name", "Brand", "500g");
    let divs = repo::divergences(&p, &[asda], &HashMap::new());
    let fields: Vec<&str> = divs.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(fields, vec!["name"], "agreeing fields raise no divergence");
}

#[test]
fn a_settled_field_stays_quiet_until_the_value_set_changes() {
    let p = product("Name", "OFF Brand", "500g");
    let asda = listing("asda", "Name", "Asda Brand", "500g");
    // Decision recorded the exact value set that's on the table now → suppressed.
    let mut decided: repo::DecisionMap = HashMap::new();
    decided.insert(
        "brand".into(),
        vec!["Asda Brand".into(), "OFF Brand".into()],
    );
    let divs = repo::divergences(&p, std::slice::from_ref(&asda), &decided);
    assert!(
        divs.iter().all(|d| d.field != "brand"),
        "a settled field is suppressed while its value set is unchanged"
    );

    // A source changes its value → the set differs → it resurfaces.
    let asda2 = listing("asda", "Name", "Asda Brand v2", "500g");
    let divs = repo::divergences(&p, &[asda2], &decided);
    assert!(
        divs.iter().any(|d| d.field == "brand"),
        "a changed source value re-surfaces the divergence"
    );
}

#[tokio::test]
async fn reconcile_adopts_keeps_and_settles_against_the_db() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping reconcile DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000456";
    let (off_ext, asda_ext) = ("rectest-off", "rectest-asda");
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

    // OFF first: seeds the canonical name/brand/pack.
    let p = repo::upsert_external(
        &pool,
        "off",
        off_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("off crowd name"),
            brand: Some("OFF Brand"),
            quantity_label: Some("500g"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Asda lists the same barcode, disagreeing on all three fields.
    repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Clean Asda Name"),
            brand: Some("Asda Brand"),
            quantity_label: Some("500G"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Fill-if-empty left the canonical fields as OFF seeded them; all three now
    // diverge from Asda.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    let cur = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    let divs = repo::divergences(&cur, &listings, &decisions);
    assert_eq!(
        divs.len(),
        3,
        "name, brand, pack all diverge before reconcile"
    );

    // Adopt Asda's brand + pack; keep the name.
    repo::reconcile(
        &pool,
        p.id,
        &[
            repo::FieldChoice {
                field: "brand".into(),
                choice: "asda".into(),
                value: None,
            },
            repo::FieldChoice {
                field: "quantity_label".into(),
                choice: "asda".into(),
                value: None,
            },
            repo::FieldChoice {
                field: "name".into(),
                choice: repo::KEEP.into(),
                value: None,
            },
        ],
    )
    .await
    .unwrap();

    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(after.brand.as_deref(), Some("Asda Brand"), "brand adopted");
    assert_eq!(
        after.quantity_label.as_deref(),
        Some("500G"),
        "pack adopted"
    );
    assert_eq!(after.name.as_deref(), Some("off crowd name"), "name kept");

    // Everything is settled now — nothing left to review.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    let divs = repo::divergences(&after, &listings, &decisions);
    assert!(divs.is_empty(), "all fields settled, got {divs:?}");

    // Asda changes its brand → that divergence returns, the settled ones stay quiet.
    repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Clean Asda Name"),
            brand: Some("Asda Brand Refreshed"),
            quantity_label: Some("500G"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    let divs = repo::divergences(&after, &listings, &decisions);
    let fields: Vec<&str> = divs.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(
        fields,
        vec!["brand"],
        "only the changed field re-surfaces; kept/adopted stay quiet"
    );
}

#[tokio::test]
async fn our_own_name_wins_over_every_source_and_survives_a_refresh() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping our-own-name DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode = "5000000000789";
    let (off_ext, asda_ext) = ("ourtest-off", "ourtest-asda");
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

    // Both sources spell it wrong: OFF a crowd title, Asda a genuine typo.
    repo::upsert_external(
        &pool,
        "off",
        off_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("the original oat-ly barista"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let p = repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oalty Oat Drink Barista Edition 1L"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Neither source offers the right name, so we type our own.
    repo::reconcile(
        &pool,
        p.id,
        &[repo::FieldChoice {
            field: "name".into(),
            choice: repo::USER.into(),
            value: Some("Oatly Barista Edition 1L".into()),
        }],
    )
    .await
    .unwrap();

    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(after.name.as_deref(), Some("Oatly Barista Edition 1L"));
    assert_eq!(after.name_source.as_deref(), Some("user"), "marked our own");

    // Our own name settled the divergence — no nagging even though both shops
    // still disagree.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    assert!(
        repo::divergences(&after, &listings, &decisions)
            .iter()
            .all(|d| d.field != "name"),
        "our own name settles the name divergence"
    );

    // The shops' honest spellings are untouched — we corrected our layer, not theirs.
    let asda = listings.iter().find(|l| l.source == "asda").unwrap();
    assert_eq!(
        asda.raw_name.as_deref(),
        Some("Oalty Oat Drink Barista Edition 1L"),
        "Asda's listing still stores Asda's spelling, faithfully"
    );

    // Re-pulling a shop must never clobber our own name.
    repo::upsert_external(
        &pool,
        "asda",
        asda_ext,
        Some(barcode),
        &repo::ListingFields {
            raw_name: Some("Oalty Oat Drink Barista Edition 1L"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(
        after.name.as_deref(),
        Some("Oatly Barista Edition 1L"),
        "a source refresh keeps our own name"
    );
}

#[tokio::test]
async fn a_barcodeless_source_refresh_keeps_our_own_name() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping barcodeless-override DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let ext = "ourtest-bl-1";
    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(ext)
        .execute(&pool)
        .await
        .unwrap();

    // A barcodeless shop product (Waitrose by lineNumber) — the branch that
    // refreshes a single owner's name on re-import.
    let p = repo::upsert_external(
        &pool,
        "waitrose",
        ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Shop Spelling"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    repo::reconcile(
        &pool,
        p.id,
        &[repo::FieldChoice {
            field: "name".into(),
            choice: repo::USER.into(),
            value: Some("Our Corrected Name".into()),
        }],
    )
    .await
    .unwrap();

    // Re-import the same barcodeless source with a fresh shop name: normally it
    // refreshes the single owner's name, but our own name is protected.
    let after = repo::upsert_external(
        &pool,
        "waitrose",
        ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Shop Spelling v2"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        after.name.as_deref(),
        Some("Our Corrected Name"),
        "a barcodeless refresh must not clobber our own name"
    );
    // And the shop's own line still tracks the shop's latest spelling.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    assert_eq!(
        listings
            .iter()
            .find(|l| l.source == "waitrose")
            .unwrap()
            .raw_name
            .as_deref(),
        Some("Shop Spelling v2"),
    );
}
