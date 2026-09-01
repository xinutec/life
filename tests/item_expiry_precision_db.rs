//! How much of an expiry date is real, and what survives a save that has no
//! opinion. Real MariaDB; runs only when LIFE_TEST_DATABASE_URL is set.
//!
//! `items.expiry` is a DATE, so a medicine box printed 06/2028 has to be stored
//! with a day it does not have. The convention is the month's LAST day, because
//! the box is good through June. `expiry_precision` is what stops that invented
//! 30th from being read back as a printed one (migration 0045).

mod common;

use chrono::NaiveDate;

use life::db;
use life::inventory::repo;
use life::inventory::types::{ExpiryPrecision, ItemCategory, NewItem};

fn june_2028() -> NaiveDate {
    NaiveDate::from_ymd_opt(2028, 6, 30).expect("a real date")
}

fn item(name: &str, expiry: Option<NaiveDate>, precision: Option<ExpiryPrecision>) -> NewItem {
    NewItem {
        name: name.into(),
        category: ItemCategory::Medication,
        quantity: None,
        unit: None,
        expiry,
        expiry_precision: precision,
        location_id: None,
        barcode: None,
        product_id: None,
        name_source: None,
    }
}

#[tokio::test]
async fn a_month_precision_expiry_is_not_downgraded_by_a_save_that_says_nothing() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-expiry-precision";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    // What the form sends when somebody picks a month.
    let box_ = repo::create_item(
        &pool,
        user,
        item("Tablets", Some(june_2028()), Some(ExpiryPrecision::Month)),
    )
    .await
    .expect("create month");
    assert_eq!(box_.expiry, Some(june_2028()));
    assert_eq!(box_.expiry_precision, ExpiryPrecision::Month);

    // What everything else sends: nothing. Sync, a script, the Android app —
    // none of them has seen the box, so none of them can state its precision.
    // Writing `day` here would re-print the invented 30th as a real one, which
    // is the entire fault this column exists to stop.
    repo::update_item(
        &pool,
        user,
        box_.id,
        item("Tablets", Some(june_2028()), None),
    )
    .await
    .expect("update with no statement");
    let got = repo::get_item(&pool, user, box_.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(
        got.expiry_precision,
        ExpiryPrecision::Month,
        "a save with no opinion must not invent day precision"
    );

    // Said explicitly, it changes — the person retyped it off a full date.
    repo::update_item(
        &pool,
        user,
        box_.id,
        item("Tablets", Some(june_2028()), Some(ExpiryPrecision::Day)),
    )
    .await
    .expect("update stating day");
    let got = repo::get_item(&pool, user, box_.id)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(got.expiry_precision, ExpiryPrecision::Day);
}

#[tokio::test]
async fn an_item_created_without_a_statement_is_day_precise() {
    let pool = db::connect(&common::test_db_url()).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");
    let user = "test-user-expiry-precision-default";
    sqlx::query("DELETE FROM items WHERE user_id = ?")
        .bind(user)
        .execute(&pool)
        .await
        .expect("clean");

    // Food is the common case and its date is printed in full, so the default
    // has to be `day`. It is also the recoverable direction: over-stated
    // precision is visible on the screen its owner reads, whereas a wrongly
    // widened food date hides the day you were meant to eat it by.
    let jar = repo::create_item(
        &pool,
        user,
        item(
            "Jar",
            Some(NaiveDate::from_ymd_opt(2026, 9, 14).unwrap()),
            None,
        ),
    )
    .await
    .expect("create default");
    assert_eq!(jar.expiry_precision, ExpiryPrecision::Day);
}
