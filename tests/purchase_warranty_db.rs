//! Recording a purchase after the fact, and what a warranty is measured from.
//! Real MariaDB; runs only when LIFE_TEST_DATABASE_URL is set.
//!
//! Until 2026-09-03 the only writer of `purchases` was the buy-list flow, so
//! anything not bought through the app could never carry a price or a date —
//! which is most of a house. A warranty needs a start, and this is where it
//! comes from.

mod common;

use chrono::{Datelike, Duration, Months, NaiveDate, Utc};

use life::db;
use life::inventory::repo as inv_repo;
use life::inventory::types::{ItemCategory, NewItem};
use life::purchases::repo::{self as purchases_repo, BoughtItem};
use life::purchases::types::NewPurchase;

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

fn purchase(shop: &str, pence: i64) -> NewPurchase {
    NewPurchase {
        shop: shop.into(),
        amount_minor: pence,
        currency: "GBP".into(),
        bought_on: None,
        warranty_months: None,
    }
}

async fn setup(user: &str) -> sqlx::MySqlPool {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean purchases");
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean items");
    pool
}

#[tokio::test]
async fn a_warranty_runs_in_calendar_months_from_the_day_it_was_bought() {
    let user = "test-user-warranty";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Dishwasher"))
        .await
        .expect("create");

    // Bought two years ago with two years of cover: the interesting case,
    // because it is the one where the answer is "just ran out" rather than
    // "obviously fine", and it cannot be answered without a real start date.
    let bought = Utc::now().date_naive() - Duration::days(730);
    let new = NewPurchase {
        bought_on: Some(bought),
        warranty_months: Some(24),
        ..purchase("Currys", 34_999)
    };
    let bought_item = BoughtItem {
        id: item.id,
        product_id: None,
        barcode: None,
        name: &item.name,
        quantity: None,
        unit: None,
    };
    purchases_repo::record(&pool, user, &bought_item, &new)
        .await
        .expect("record");

    let got = purchases_repo::for_item(&pool, user, item.id)
        .await
        .expect("read");
    assert_eq!(got.len(), 1);
    let p = &got[0];
    assert_eq!(p.amount_minor, 34_999);
    assert_eq!(p.warranty_months, Some(24));
    assert_eq!(
        p.bought_at.date_naive(),
        bought,
        "the stated day must survive the round trip — stored at midday so no \
         zone offset can walk it into the day before"
    );
    // Calendar months, not 30-day blocks: two years from 3 March is 3 March.
    let until = p.warranty_until.expect("a length gives an end");
    let expected = bought
        .checked_add_months(Months::new(24))
        .expect("two years on is a real day");
    assert_eq!(until.date_naive(), expected);
    assert_eq!(
        (until.year(), until.month(), until.day()),
        (expected.year(), expected.month(), expected.day()),
    );
}

#[tokio::test]
async fn a_purchase_with_no_warranty_claims_no_cover() {
    let user = "test-user-warranty-none";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Kettle"))
        .await
        .expect("create");
    let bought_item = BoughtItem {
        id: item.id,
        product_id: None,
        barcode: None,
        name: &item.name,
        quantity: None,
        unit: None,
    };
    purchases_repo::record(&pool, user, &bought_item, &purchase("Argos", 1_999))
        .await
        .expect("record");

    let got = purchases_repo::for_item(&pool, user, item.id)
        .await
        .expect("read");
    // Most things in a cupboard have no warranty, and "not recorded" must not
    // render as an end date somebody could act on.
    assert_eq!(got[0].warranty_months, None);
    assert_eq!(got[0].warranty_until, None);
    // Absent `bought_on` means now — the buy-list flow's every call.
    assert_eq!(got[0].bought_at.date_naive(), Utc::now().date_naive());
}

#[tokio::test]
async fn a_future_purchase_and_an_absurd_warranty_are_refused_not_stored() {
    let user = "test-user-warranty-bad";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Oven"))
        .await
        .expect("create");
    let bought_item = BoughtItem {
        id: item.id,
        product_id: None,
        barcode: None,
        name: &item.name,
        quantity: None,
        unit: None,
    };

    // A date that has not happened would report cover nobody has.
    let tomorrow = Utc::now().date_naive() + Duration::days(1);
    let err = purchases_repo::record(
        &pool,
        user,
        &bought_item,
        &NewPurchase {
            bought_on: Some(tomorrow),
            ..purchase("Currys", 100)
        },
    )
    .await
    .expect_err("a future purchase must be refused");
    assert!(
        err.to_string().contains("future"),
        "the error should say what is wrong: {err}"
    );

    // 24 typed into a box that means months is a warranty; 24 typed into it
    // meaning YEARS is the mistake this field invites, and 288 is not it —
    // 2400 is. The bound exists to catch the order-of-magnitude slip.
    let err = purchases_repo::record(
        &pool,
        user,
        &bought_item,
        &NewPurchase {
            warranty_months: Some(2400),
            ..purchase("Currys", 100)
        },
    )
    .await
    .expect_err("an absurd warranty must be refused");
    assert!(err.to_string().contains("2400"), "{err}");

    // Nothing was stored by either attempt.
    let got = purchases_repo::for_item(&pool, user, item.id)
        .await
        .expect("read");
    assert!(
        got.is_empty(),
        "a refused purchase must leave no row: {got:?}"
    );
}

#[tokio::test]
async fn an_old_purchase_does_not_disturb_the_day_it_names() {
    let user = "test-user-warranty-tz";
    let pool = setup(user).await;
    let item = inv_repo::create_item(&pool, user, appliance("Washing machine"))
        .await
        .expect("create");
    let bought_item = BoughtItem {
        id: item.id,
        product_id: None,
        barcode: None,
        name: &item.name,
        quantity: None,
        unit: None,
    };
    // The 1st and the 31st: the two days a midnight-stored date can walk off,
    // in opposite directions, and the two that would show as a different MONTH
    // when they did.
    for day in [
        NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
    ] {
        let id = purchases_repo::record(
            &pool,
            user,
            &bought_item,
            &NewPurchase {
                bought_on: Some(day),
                ..purchase("Currys", 500)
            },
        )
        .await
        .expect("record");
        let got = purchases_repo::for_item(&pool, user, item.id)
            .await
            .expect("read");
        let p = got.iter().find(|p| p.id == id).expect("present");
        assert_eq!(
            p.bought_at.date_naive(),
            day,
            "{day} must read back as itself"
        );
    }
}

/// Unmaking a purchase, and the two ways a delete can reach too far.
///
/// This is what makes the WRITE path testable at all: without an inverse,
/// exercising the success path against production leaves a fabricated number in
/// the money history forever, so the route shipped verified by its refusals
/// only. See the task on purchase correction.
#[tokio::test]
async fn a_purchase_is_deletable_but_only_its_owners_and_only_on_its_own_item() {
    let user = "test-user-purchase-delete";
    let other = "test-user-purchase-delete-other";
    let pool = setup(user).await;
    sqlx::query("DELETE FROM purchases WHERE user_id = ?")
        .bind(other)
        .execute(&pool)
        .await
        .expect("clean other");

    let kettle = inv_repo::create_item(&pool, user, appliance("Kettle"))
        .await
        .expect("create kettle");
    let oven = inv_repo::create_item(&pool, user, appliance("Oven"))
        .await
        .expect("create oven");
    let bought = |item: &life::inventory::types::Item| BoughtItem {
        id: item.id,
        product_id: None,
        barcode: None,
        name: "thing",
        quantity: None,
        unit: None,
    };
    let on_kettle =
        purchases_repo::record(&pool, user, &bought(&kettle), &purchase("Argos", 1_999))
            .await
            .expect("record on kettle");

    // Someone else's delete must not reach it, even with the right ids.
    assert!(
        !purchases_repo::remove(&pool, other, kettle.id, on_kettle)
            .await
            .expect("remove as other"),
        "a purchase is only its owner's to delete"
    );
    // Nor may the right owner delete it through the WRONG item. The id alone
    // would be enough for ownership; reaching it through an item it does not
    // belong to is a client bug, and deleting anyway would take a row off a
    // list nobody was looking at.
    assert!(
        !purchases_repo::remove(&pool, user, oven.id, on_kettle)
            .await
            .expect("remove via wrong item"),
        "a purchase must not be reachable through another item"
    );
    assert_eq!(
        purchases_repo::for_item(&pool, user, kettle.id)
            .await
            .expect("read")
            .len(),
        1,
        "neither refused delete may have removed anything"
    );

    // The real one works, once.
    assert!(
        purchases_repo::remove(&pool, user, kettle.id, on_kettle)
            .await
            .expect("remove")
    );
    assert!(
        purchases_repo::for_item(&pool, user, kettle.id)
            .await
            .expect("read")
            .is_empty()
    );
    // And is idempotent in the honest direction: a second delete reports that it
    // removed nothing, which is what lets the route answer 404 rather than
    // pretending it did something.
    assert!(
        !purchases_repo::remove(&pool, user, kettle.id, on_kettle)
            .await
            .expect("remove again")
    );
}
