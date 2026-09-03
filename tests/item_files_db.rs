//! Attaching receipts and manuals to a thing you own. Real MariaDB; runs only
//! when LIFE_TEST_DATABASE_URL is set.
//!
//! The catalogue stores one image per PRODUCT, keyed on a barcode. That shape
//! could not hold any of this: products are shared reference data and a receipt
//! is personal, and an appliance entered by hand has neither a barcode nor a
//! product (migration 0047).

mod common;

use life::db;
use life::files::repo as files_repo;
use life::files::types::sniff_mime;
use life::inventory::repo as inv_repo;
use life::inventory::types::{ItemCategory, NewItem};
use life::purchases::repo::{self as purchases_repo, BoughtItem};
use life::purchases::types::NewPurchase;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";

fn appliance(name: &str) -> NewItem {
    NewItem {
        name: name.into(),
        category: ItemCategory::Appliance,
        quantity: None,
        unit: None,
        expiry: None,
        expiry_precision: None,
        location_id: None,
        barcode: None,
        product_id: None,
        name_source: None,
    }
}

async fn setup(user: &str) -> sqlx::MySqlPool {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    for u in [user, "test-user-files-other"] {
        sqlx::query("DELETE FROM item_files WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .expect("clean files");
        sqlx::query("DELETE FROM purchases WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .expect("clean purchases");
        sqlx::query("DELETE FROM items WHERE user_id = ?")
            .bind(u)
            .execute(&pool)
            .await
            .expect("clean items");
    }
    pool
}

/// What may be stored, decided by BYTES.
///
/// The declared Content-Type is caller-supplied and can lie, and these files are
/// served back on our own origin — so anything with an executable
/// interpretation riding in under an innocent header would be stored XSS. SVG is
/// the case that matters and is refused by construction: it is not in the match.
#[test]
fn only_images_and_pdfs_are_recognised_and_svg_is_not_one() {
    assert_eq!(sniff_mime(PNG), Some("image/png"));
    assert_eq!(sniff_mime(PDF), Some("application/pdf"));
    assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
    // An iPhone photographs a receipt as HEIC by default; refusing it would
    // refuse the commonest way one actually arrives.
    let heic = b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00";
    assert_eq!(sniff_mime(heic), Some("image/heic"));

    // ⚠ The one that must never be accepted, however it is labelled.
    assert_eq!(
        sniff_mime(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script/></svg>"),
        None
    );
    assert_eq!(
        sniff_mime(b"<!DOCTYPE html><script>alert(1)</script>"),
        None
    );
    assert_eq!(sniff_mime(b""), None);
    // A PDF signature has to be at the START; a file that merely mentions one
    // is not a PDF.
    assert_eq!(sniff_mime(b"not a pdf %PDF- honest"), None);
}

#[tokio::test]
async fn a_receipt_knows_its_purchase_and_a_manual_does_not() {
    let user = "test-user-files";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Dishwasher"))
        .await
        .expect("create");
    let purchase_id = purchases_repo::record(
        &pool,
        user,
        &BoughtItem {
            id: item.id,
            product_id: None,
            barcode: None,
            name: &item.name,
            quantity: None,
            unit: None,
        },
        &NewPurchase {
            shop: "Currys".into(),
            amount_minor: 34_999,
            currency: "GBP".into(),
            bought_on: None,
            warranty_months: Some(24),
        },
    )
    .await
    .expect("record purchase");

    let receipt = files_repo::add(
        &pool,
        user,
        item.id,
        Some(purchase_id),
        "receipt.pdf",
        "application/pdf",
        PDF,
    )
    .await
    .expect("add receipt");
    files_repo::add(
        &pool,
        user,
        item.id,
        None,
        "manual.pdf",
        "application/pdf",
        PDF,
    )
    .await
    .expect("add manual");

    let files = files_repo::for_item(&pool, user, item.id)
        .await
        .expect("list");
    assert_eq!(files.len(), 2);
    let r = files.iter().find(|f| f.id == receipt).expect("receipt");
    assert_eq!(
        r.purchase_id,
        Some(purchase_id),
        "a receipt proves one purchase"
    );
    assert_eq!(
        r.size_bytes,
        PDF.len() as u64,
        "size is stored, not derived"
    );
    let m = files
        .iter()
        .find(|f| f.name == "manual.pdf")
        .expect("manual");
    assert_eq!(
        m.purchase_id, None,
        "a manual belongs to the thing, not to a purchase"
    );

    // ⚠ Deleting a mistyped purchase must NOT take the scanned receipt with it —
    // ON DELETE SET NULL, not CASCADE. The receipt is the harder thing to
    // replace of the two.
    assert!(
        purchases_repo::remove(&pool, user, item.id, purchase_id)
            .await
            .expect("remove purchase")
    );
    let files = files_repo::for_item(&pool, user, item.id)
        .await
        .expect("list after purchase delete");
    assert_eq!(files.len(), 2, "the receipt must survive its purchase");
    assert_eq!(
        files
            .iter()
            .find(|f| f.id == receipt)
            .expect("receipt")
            .purchase_id,
        None,
        "it is simply no longer tied to one"
    );
}

#[tokio::test]
async fn a_file_is_only_reachable_through_its_own_item_and_its_own_owner() {
    let user = "test-user-files";
    let other = "test-user-files-other";
    let pool = setup(user).await;
    let dishwasher = inv_repo::create_item(&pool, user, appliance("Dishwasher"))
        .await
        .expect("create");
    let oven = inv_repo::create_item(&pool, user, appliance("Oven"))
        .await
        .expect("create oven");
    let id = files_repo::add(
        &pool,
        user,
        dishwasher.id,
        None,
        "receipt.png",
        "image/png",
        PNG,
    )
    .await
    .expect("add");

    assert!(
        files_repo::read(&pool, other, dishwasher.id, id)
            .await
            .expect("read as other")
            .is_none(),
        "somebody else's file is not served"
    );
    assert!(
        files_repo::read(&pool, user, oven.id, id)
            .await
            .expect("read via wrong item")
            .is_none(),
        "a file must not be served from under the wrong thing"
    );
    let (name, mime, bytes) = files_repo::read(&pool, user, dishwasher.id, id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!((name.as_str(), mime.as_str()), ("receipt.png", "image/png"));
    assert_eq!(bytes, PNG, "the bytes come back exactly as stored");

    // Same two refusals on the way out.
    assert!(
        !files_repo::remove(&pool, other, dishwasher.id, id)
            .await
            .expect("del as other")
    );
    assert!(
        !files_repo::remove(&pool, user, oven.id, id)
            .await
            .expect("del wrong item")
    );
    assert_eq!(
        files_repo::for_item(&pool, user, dishwasher.id)
            .await
            .expect("list")
            .len(),
        1
    );
    assert!(
        files_repo::remove(&pool, user, dishwasher.id, id)
            .await
            .expect("del")
    );
    assert!(
        files_repo::for_item(&pool, user, dishwasher.id)
            .await
            .expect("list")
            .is_empty()
    );
}

/// Deleting the ITEM does take its files — unlike the purchase link. A file
/// attached to nothing has no route to reach it and no screen to show it.
#[tokio::test]
async fn deleting_the_item_takes_its_files() {
    let user = "test-user-files";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Kettle"))
        .await
        .expect("create");
    files_repo::add(&pool, user, item.id, None, "r.png", "image/png", PNG)
        .await
        .expect("add");
    // The app soft-deletes, so reach past it to the real thing the FK guards.
    sqlx::query("DELETE FROM items WHERE id = ? AND user_id = ?")
        .bind(item.id)
        .bind(user)
        .execute(&pool)
        .await
        .expect("hard delete");
    assert!(
        files_repo::for_item(&pool, user, item.id)
            .await
            .expect("list")
            .is_empty()
    );
}
