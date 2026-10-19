//! The day a purchase is recorded against. Pure: `now` is passed in.

use chrono::{NaiveDate, TimeZone, Utc};

use life::purchases::repo::bought_at;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn today_picked_in_the_morning_is_today_not_the_future() {
    // Before midday UTC, so "today at midday" has not happened yet.
    let now = Utc.with_ymd_and_hms(2026, 10, 19, 9, 0, 0).unwrap();
    let at = bought_at(Some(day(2026, 10, 19)), now).expect("today is not the future");
    assert_eq!(at.date_naive(), day(2026, 10, 19));
    assert!(at <= now, "stored after it was recorded: {at}");
}

#[test]
fn an_earlier_day_is_stored_at_midday_and_tomorrow_is_refused() {
    let now = Utc.with_ymd_and_hms(2026, 10, 19, 9, 0, 0).unwrap();
    assert_eq!(
        bought_at(Some(day(2026, 10, 18)), now).unwrap(),
        Utc.with_ymd_and_hms(2026, 10, 18, 12, 0, 0).unwrap()
    );
    let err = bought_at(Some(day(2026, 10, 20)), now).expect_err("tomorrow");
    assert!(err.to_string().contains("future"), "{err}");
}
