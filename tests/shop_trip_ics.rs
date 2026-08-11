//! The shop trip as it leaves for Nextcloud.
//!
//! Everything asserted here is read in a shop, on a phone, by someone who is
//! not looking at this app: the title, the time, the reminder that got them out
//! of the house, and the list they are pushing a trolley against. The `VEVENT`
//! is the whole delivery, so this is where it is checked.

use chrono::{DateTime, TimeZone, Utc};
use life::calendar::trip::{self, ShopTrip};

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, hour, minute, 0).unwrap()
}

fn trip_of(shop: &str, items: &[&str]) -> ShopTrip {
    ShopTrip {
        shop: shop.to_string(),
        starts_at: at(10, 0),
        minutes: 60,
        items: items.iter().map(|i| i.to_string()).collect(),
    }
}

/// Unfolds iCal's 75-octet line wrapping, so an assertion is about the content
/// and not about where the serializer happened to break the line.
fn unfolded(ics: &str) -> String {
    ics.replace("\r\n ", "").replace("\r\n\t", "")
}

fn render(trip: &ShopTrip) -> String {
    unfolded(&trip::ics(trip, "trip-1@life", at(9, 0)).expect("a valid trip renders"))
}

#[test]
fn the_event_says_where_and_when() {
    let ics = render(&trip_of("Asda", &[]));
    assert!(ics.contains("SUMMARY:Shop at Asda"), "{ics}");
    // LOCATION as well as the title: it is the field a calendar app offers to
    // navigate to, and the title is only text.
    assert!(ics.contains("LOCATION:Asda"), "{ics}");
    assert!(ics.contains("DTSTART:20260815T100000Z"), "{ics}");
    assert!(ics.contains("DTEND:20260815T110000Z"), "{ics}");
    assert!(ics.contains("UID:trip-1@life"), "{ics}");
}

#[test]
fn it_carries_a_reminder() {
    // Without the alarm this is a note, and the Buy list is already a better
    // note. Half an hour before is the whole reason to write it to a calendar.
    let ics = render(&trip_of("Asda", &[]));
    assert!(ics.contains("BEGIN:VALARM"), "{ics}");
    assert!(ics.contains("ACTION:DISPLAY"), "{ics}");
    // Serialized in seconds, which is what `dur-time` allows and what the crate
    // emits — 1800 before the start.
    assert!(ics.contains("TRIGGER:-PT1800S"), "{ics}");
}

#[test]
fn the_buy_list_rides_along_in_the_description() {
    let ics = render(&trip_of("Waitrose", &["Milk", "Bread", "Eggs"]));
    assert!(ics.contains("From the Buy list:"), "{ics}");
    for item in ["Milk", "Bread", "Eggs"] {
        assert!(ics.contains(item), "{item} missing from {ics}");
    }
}

#[test]
fn an_empty_list_writes_no_description() {
    // "From the Buy list:" followed by nothing would read as "bring nothing".
    let ics = render(&trip_of("Asda", &[]));
    assert!(!ics.contains("From the Buy list"), "{ics}");
    // One DESCRIPTION remains, and it belongs to the alarm — the event itself
    // has none.
    assert_eq!(ics.matches("DESCRIPTION:").count(), 1, "{ics}");
}

#[test]
fn blank_entries_do_not_become_empty_bullets() {
    let ics = render(&trip_of("Asda", &["Milk", "   ", ""]));
    assert!(ics.contains("Milk"), "{ics}");
    assert_eq!(ics.matches('•').count(), 1, "{ics}");
}

#[test]
fn a_comma_in_an_item_survives_the_wire() {
    // iCal gives `,` `;` and newline meaning inside TEXT. A description built by
    // hand would split "Rice, long grain" into two properties' worth of nonsense
    // — which is why this is the `icalendar` crate's job and not a format!().
    let ics = render(&trip_of("Asda", &["Rice, long grain"]));
    assert!(ics.contains("Rice\\, long grain"), "{ics}");
}

#[test]
fn a_long_list_is_counted_rather_than_cut() {
    // A list that silently stopped would be read in the shop as the whole list.
    let many: Vec<String> = (1..=75).map(|n| format!("Item {n}")).collect();
    let trip = ShopTrip {
        shop: "Asda".to_string(),
        starts_at: at(10, 0),
        minutes: 60,
        items: many,
    };
    let ics = render(&trip);
    assert!(ics.contains("Item 60"), "{ics}");
    assert!(!ics.contains("Item 61"), "{ics}");
    assert!(ics.contains("and 15 more on the list"), "{ics}");
}

#[test]
fn a_trip_needs_a_shop() {
    let trip = trip_of("   ", &[]);
    assert!(trip::ics(&trip, "trip-1@life", at(9, 0)).is_err());
}

#[test]
fn a_nonsense_duration_is_refused_rather_than_written() {
    // Both ends: a zero-length event is not a trip, and a client that sent
    // minutes where it meant seconds would otherwise block out next week.
    for minutes in [0, -30, 60 * 24] {
        let trip = ShopTrip {
            shop: "Asda".to_string(),
            starts_at: at(10, 0),
            minutes,
            items: Vec::new(),
        };
        assert!(
            trip::ics(&trip, "trip-1@life", at(9, 0)).is_err(),
            "{minutes} minutes was accepted"
        );
    }
}

#[test]
fn the_stamp_is_the_one_we_passed_in() {
    // DTSTAMP is "when this was written", and a test that could not fix it would
    // be asserting against the clock.
    let ics = render(&trip_of("Asda", &[]));
    assert!(ics.contains("DTSTAMP:20260815T090000Z"), "{ics}");
}
