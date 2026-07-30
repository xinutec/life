//! Reconciliation: surfacing where a product's sources disagree with its
//! canonical row, and settling those disagreements by adopting or keeping a
//! value. The `divergences` rule is pure (no DB); the reconcile round-trip runs
//! against a real MariaDB only when LIFE_TEST_DATABASE_URL is set.

use std::collections::HashMap;

use life::db;
use life::products::ids::{Barcode, ExternalId, ProductId};
use life::products::repo::{self, Listing};
use life::products::source::Source;
use life::products::types::{Choice, FieldChoice, Product, ReconcileField};

fn product(name: &str, brand: &str, quantity: &str) -> Product {
    Product {
        id: ProductId(1),
        barcode: Some("5000000000123".parse().unwrap()),
        name: Some(name.into()),
        brand: Some(brand.into()),
        quantity_label: Some(quantity.into()),
        source: Some(Source::Off),
        external_id: None,
        name_source: Some(Source::Off),
        image_source: None,
        has_image: false,
    }
}

fn listing(source: Source, name: &str, brand: &str, quantity: &str) -> Listing {
    Listing {
        source,
        external_id: format!("{source}-cin").parse().unwrap(),
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
    let asda = listing(Source::Asda, "Clean Asda Name", "Asda Brand", "500G");
    let divs = repo::divergences(&p, &[asda], &HashMap::new());

    let fields: Vec<&str> = divs.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(fields, vec!["name", "brand", "quantity_label"]);
    let brand = divs
        .iter()
        .find(|d| d.field == ReconcileField::Brand)
        .unwrap();
    assert_eq!(brand.current.as_deref(), Some("OFF Brand"));
    assert_eq!(brand.candidates.len(), 1);
    assert_eq!(brand.candidates[0].source, Source::Asda);
    assert_eq!(brand.candidates[0].value, "Asda Brand");
}

#[test]
fn a_source_that_agrees_is_not_a_divergence() {
    let p = product("Name", "Brand", "500g");
    // Same brand, same pack; only the name differs.
    let asda = listing(Source::Asda, "A Different Name", "Brand", "500g");
    let divs = repo::divergences(&p, &[asda], &HashMap::new());
    let fields: Vec<&str> = divs.iter().map(|d| d.field.as_str()).collect();
    assert_eq!(fields, vec!["name"], "agreeing fields raise no divergence");
}

#[test]
fn a_settled_field_stays_quiet_until_the_value_set_changes() {
    let p = product("Name", "OFF Brand", "500g");
    let asda = listing(Source::Asda, "Name", "Asda Brand", "500g");
    // Decision recorded the exact value set that's on the table now → suppressed.
    let mut decided: repo::DecisionMap = HashMap::new();
    decided.insert(
        ReconcileField::Brand,
        vec!["Asda Brand".into(), "OFF Brand".into()],
    );
    let divs = repo::divergences(&p, std::slice::from_ref(&asda), &decided);
    assert!(
        divs.iter().all(|d| d.field != ReconcileField::Brand),
        "a settled field is suppressed while its value set is unchanged"
    );

    // A source changes its value → the set differs → it resurfaces.
    let asda2 = listing(Source::Asda, "Name", "Asda Brand v2", "500g");
    let divs = repo::divergences(&p, &[asda2], &decided);
    assert!(
        divs.iter().any(|d| d.field == ReconcileField::Brand),
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

    let barcode: Barcode = "5000000000456".parse().unwrap();
    let (off_ext, asda_ext): (ExternalId, ExternalId) = (
        "rectest-off".parse().unwrap(),
        "rectest-asda".parse().unwrap(),
    );
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [&off_ext, &asda_ext] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // OFF first: seeds the canonical name/brand/pack.
    let p = repo::upsert_external(
        &pool,
        Source::Off,
        &off_ext,
        Some(&barcode),
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
        Source::Asda,
        &asda_ext,
        Some(&barcode),
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
            FieldChoice {
                field: ReconcileField::Brand,
                choice: Choice::Asda,
                value: None,
            },
            FieldChoice {
                field: ReconcileField::QuantityLabel,
                choice: Choice::Asda,
                value: None,
            },
            FieldChoice {
                field: ReconcileField::Name,
                choice: Choice::Keep,
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
        Source::Asda,
        &asda_ext,
        Some(&barcode),
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

    let barcode: Barcode = "5000000000789".parse().unwrap();
    let (off_ext, asda_ext): (ExternalId, ExternalId) = (
        "ourtest-off".parse().unwrap(),
        "ourtest-asda".parse().unwrap(),
    );
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    for ext in [&off_ext, &asda_ext] {
        sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
            .bind(ext)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Both sources spell it wrong: OFF a crowd title, Asda a genuine typo.
    repo::upsert_external(
        &pool,
        Source::Off,
        &off_ext,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("the original oat-ly barista"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let p = repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_ext,
        Some(&barcode),
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
        &[FieldChoice {
            field: ReconcileField::Name,
            choice: Choice::User,
            value: Some("Oatly Barista Edition 1L".into()),
        }],
    )
    .await
    .unwrap();

    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(after.name.as_deref(), Some("Oatly Barista Edition 1L"));
    assert_eq!(after.name_source, Some(Source::User), "marked our own");

    // Our own name settled the divergence — no nagging even though both shops
    // still disagree.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    assert!(
        repo::divergences(&after, &listings, &decisions)
            .iter()
            .all(|d| d.field != ReconcileField::Name),
        "our own name settles the name divergence"
    );

    // The shops' honest spellings are untouched — we corrected our layer, not theirs.
    let asda = listings.iter().find(|l| l.source == Source::Asda).unwrap();
    assert_eq!(
        asda.raw_name.as_deref(),
        Some("Oalty Oat Drink Barista Edition 1L"),
        "Asda's listing still stores Asda's spelling, faithfully"
    );

    // Re-pulling a shop must never clobber our own name.
    repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_ext,
        Some(&barcode),
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
async fn our_own_brand_and_pack_win_over_sources_and_survive_a_refresh() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping our-own-brand/pack DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let barcode: Barcode = "5000000000790".parse().unwrap();
    let asda_ext: ExternalId = "ourdetails-asda".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE barcode = ?")
        .bind(&barcode)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM product_listings WHERE external_id = ?")
        .bind(&asda_ext)
        .execute(&pool)
        .await
        .unwrap();

    // Asda seeds the canonical row with its own casing — "250ML", the very thing
    // that started this: no other source disagrees, so only our layer can fix it.
    let p = repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_ext,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Some Oat Drink"),
            brand: Some("asda brand"),
            quantity_label: Some("250ML"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Type our own brand + pack — no source offered these values.
    repo::reconcile(
        &pool,
        p.id,
        &[
            FieldChoice {
                field: ReconcileField::Brand,
                choice: Choice::User,
                value: Some("Oatly".into()),
            },
            FieldChoice {
                field: ReconcileField::QuantityLabel,
                choice: Choice::User,
                value: Some("250ml".into()),
            },
        ],
    )
    .await
    .unwrap();

    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(after.brand.as_deref(), Some("Oatly"), "our own brand");
    assert_eq!(
        after.quantity_label.as_deref(),
        Some("250ml"),
        "our own pack size"
    );

    // Our own values settle their divergences even though Asda still says
    // otherwise on its listing.
    let listings = repo::listings_for(&pool, p.id).await.unwrap();
    let decisions = repo::field_decisions(&pool, p.id).await.unwrap();
    assert!(
        repo::divergences(&after, &listings, &decisions)
            .iter()
            .all(|d| d.field != ReconcileField::Brand && d.field != ReconcileField::QuantityLabel),
        "our own brand/pack settle their divergences"
    );

    // Asda's listing keeps its honest casing.
    let asda = listings.iter().find(|l| l.source == Source::Asda).unwrap();
    assert_eq!(asda.quantity_label.as_deref(), Some("250ML"));
    assert_eq!(asda.brand.as_deref(), Some("asda brand"));

    // A refresh from Asda (barcoded → find_or_create leaves canonical untouched)
    // must not clobber our own brand/pack.
    repo::upsert_external(
        &pool,
        Source::Asda,
        &asda_ext,
        Some(&barcode),
        &repo::ListingFields {
            raw_name: Some("Some Oat Drink"),
            brand: Some("asda brand"),
            quantity_label: Some("250ML"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let after = repo::get_by_id(&pool, p.id).await.unwrap().unwrap();
    assert_eq!(
        after.brand.as_deref(),
        Some("Oatly"),
        "brand survived refresh"
    );
    assert_eq!(
        after.quantity_label.as_deref(),
        Some("250ml"),
        "pack survived refresh"
    );
}

#[tokio::test]
async fn a_barcodeless_source_refresh_keeps_our_own_brand() {
    let Ok(url) = std::env::var("LIFE_TEST_DATABASE_URL") else {
        eprintln!("LIFE_TEST_DATABASE_URL unset — skipping barcodeless-brand-override DB test");
        return;
    };
    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let ext: ExternalId = "ourtest-bl-brand".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();

    // A barcodeless product whose single owner refreshes name + brand on re-import.
    let p = repo::upsert_external(
        &pool,
        Source::Waitrose,
        &ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Shop Name"),
            brand: Some("Shop Brand"),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Make the brand our own, but leave the name to the source.
    repo::reconcile(
        &pool,
        p.id,
        &[FieldChoice {
            field: ReconcileField::Brand,
            choice: Choice::User,
            value: Some("Our Brand".into()),
        }],
    )
    .await
    .unwrap();

    // Re-import with a fresh shop name AND brand: the name refreshes (not ours),
    // the brand is protected (ours).
    let after = repo::upsert_external(
        &pool,
        Source::Waitrose,
        &ext,
        None,
        &repo::ListingFields {
            raw_name: Some("Shop Name v2"),
            brand: Some("Shop Brand v2"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        after.brand.as_deref(),
        Some("Our Brand"),
        "a barcodeless refresh must not clobber our own brand"
    );
    assert_eq!(
        after.name.as_deref(),
        Some("Shop Name v2"),
        "the un-owned name still refreshes from the source"
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

    let ext: ExternalId = "ourtest-bl-1".parse().unwrap();
    sqlx::query("DELETE FROM products WHERE external_id = ?")
        .bind(&ext)
        .execute(&pool)
        .await
        .unwrap();

    // A barcodeless shop product (Waitrose by lineNumber) — the branch that
    // refreshes a single owner's name on re-import.
    let p = repo::upsert_external(
        &pool,
        Source::Waitrose,
        &ext,
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
        &[FieldChoice {
            field: ReconcileField::Name,
            choice: Choice::User,
            value: Some("Our Corrected Name".into()),
        }],
    )
    .await
    .unwrap();

    // Re-import the same barcodeless source with a fresh shop name: normally it
    // refreshes the single owner's name, but our own name is protected.
    let after = repo::upsert_external(
        &pool,
        Source::Waitrose,
        &ext,
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
            .find(|l| l.source == Source::Waitrose)
            .unwrap()
            .raw_name
            .as_deref(),
        Some("Shop Spelling v2"),
    );
}
