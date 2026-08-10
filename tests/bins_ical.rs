//! Reading the council's bin calendar. Pure: a string in, a list of days out,
//! so every awkward shape iCal permits can be pinned without a network.
//!
//! The fixture is the real feed's structure — all-day `VALUE=DATE` events with
//! the council's own summaries — with the property id and the content-hashed
//! UIDs replaced, since those identify one address.

use chrono::NaiveDate;
use life::calendar::bins::{BinDay, upcoming};

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("a real date")
}

/// The feed's shape: a header, then all-day events in no particular order.
fn feed(events: &str) -> String {
    format!(
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         CALSCALE:GREGORIAN\r\n\
         METHOD:PUBLISH\r\n\
         PRODID://FixMyStreet//Bin Collection Calendars//EN\r\n\
         REFRESH-INTERVAL;VALUE=DURATION:P1D\r\n\
         X-WR-CALNAME:Bin calendar\r\n\
         X-WR-TIMEZONE:Europe/London\r\n\
         {events}\
         END:VCALENDAR\r\n"
    )
}

fn event(summary: &str, start: &str, end: &str) -> String {
    format!(
        "BEGIN:VEVENT\r\n\
         DESCRIPTION:\r\n\
         DTEND;VALUE=DATE:{end}\r\n\
         DTSTAMP:20260810T150645Z\r\n\
         DTSTART;VALUE=DATE:{start}\r\n\
         SUMMARY:{summary}\r\n\
         UID:0000000000000000000000000000000000000000@example.invalid\r\n\
         END:VEVENT\r\n"
    )
}

#[test]
fn collections_come_back_soonest_first() {
    let ics = feed(&format!(
        "{}{}{}",
        event("Rubbish collection", "20260827", "20260828"),
        event("Recycling collection", "20260820", "20260821"),
        event("Food waste collection", "20260813", "20260814"),
    ));

    let days = upcoming(&ics, day(2026, 8, 1)).expect("parses");

    assert_eq!(
        days,
        vec![
            BinDay {
                kind: "Food waste collection".into(),
                date: day(2026, 8, 13)
            },
            BinDay {
                kind: "Recycling collection".into(),
                date: day(2026, 8, 20)
            },
            BinDay {
                kind: "Rubbish collection".into(),
                date: day(2026, 8, 27)
            },
        ]
    );
}

#[test]
fn two_collections_on_one_morning_keep_a_stable_order() {
    // They really do land together, and a list that reshuffled them between
    // loads would read as though the schedule had changed.
    let ics = feed(&format!(
        "{}{}",
        event("Rubbish collection", "20260813", "20260814"),
        event("Food waste collection", "20260813", "20260814"),
    ));

    let days = upcoming(&ics, day(2026, 8, 1)).expect("parses");

    assert_eq!(
        days.iter().map(|d| d.kind.as_str()).collect::<Vec<_>>(),
        vec!["Food waste collection", "Rubbish collection"]
    );
}

#[test]
fn collections_already_gone_are_not_upcoming() {
    // The feed keeps a rolling window that reaches back into last week.
    let ics = feed(&format!(
        "{}{}",
        event("Rubbish collection", "20260806", "20260807"),
        event("Rubbish collection", "20260813", "20260814"),
    ));

    let days = upcoming(&ics, day(2026, 8, 10)).expect("parses");

    assert_eq!(days.len(), 1);
    assert_eq!(days[0].date, day(2026, 8, 13));
}

#[test]
fn a_collection_today_is_still_upcoming() {
    // The bins go out in the morning and this is read over breakfast. Dropping
    // today would hide the one that matters most.
    let ics = feed(&event("Food waste collection", "20260810", "20260811"));

    let days = upcoming(&ics, day(2026, 8, 10)).expect("parses");

    assert_eq!(days.len(), 1, "today counts");
}

#[test]
fn the_councils_own_name_is_what_comes_back() {
    // Not mapped onto an enum of ours: the set of collections is the council's
    // to change, and one we did not recognise must still show rather than
    // disappear into an "Other".
    let ics = feed(&event(
        "Paper and cardboard (blue sacks) collection",
        "20260813",
        "20260814",
    ));

    let days = upcoming(&ics, day(2026, 8, 1)).expect("parses");

    assert_eq!(days[0].kind, "Paper and cardboard (blue sacks) collection");
}

#[test]
fn a_folded_line_reads_as_one_value() {
    // iCal folds at 75 octets, continuing with a leading space that is part of
    // the FOLD and not of the value — so a space in the text needs a second
    // one. A reader that took lines literally would truncate a long collection
    // name, and the real feed already folds its own header.
    let ics = feed(
        "BEGIN:VEVENT\r\n\
         DTSTART;VALUE=DATE:20260813\r\n\
         SUMMARY:Garden waste and Christmas tree collection\\, second\r\n  \
         fortnight\r\n\
         UID:1@example.invalid\r\n\
         END:VEVENT\r\n",
    );

    let days = upcoming(&ics, day(2026, 8, 1)).expect("parses");

    assert_eq!(
        days[0].kind, "Garden waste and Christmas tree collection, second fortnight",
        "unfolded, and the escaped comma is a comma"
    );
}

#[test]
fn an_event_that_says_nothing_useful_is_skipped_not_guessed_at() {
    // A row reading "collection, sometime" is worse than no row.
    let ics = feed(
        "BEGIN:VEVENT\r\n\
         DTSTART;VALUE=DATE:20260813\r\n\
         UID:1@example.invalid\r\n\
         END:VEVENT\r\n",
    );

    assert!(upcoming(&ics, day(2026, 8, 1)).expect("parses").is_empty());
}

#[test]
fn an_empty_calendar_is_an_empty_list_not_a_failure() {
    // A council with nothing scheduled is a fact, not a fault.
    assert!(
        upcoming(&feed(""), day(2026, 8, 1))
            .expect("parses")
            .is_empty()
    );
}

#[test]
fn something_that_is_not_a_calendar_fails_rather_than_reading_as_empty() {
    // The feed is fetched over the network from a council's website; an error
    // page arriving with a 200 must not read as "no collections this month".
    let err = upcoming(
        "<html><body>Service unavailable</body></html>",
        day(2026, 8, 1),
    )
    .expect_err("HTML is not a calendar");
    assert!(
        err.to_string().contains("iCalendar"),
        "the error says what it wanted: {err}"
    );
}
