//! A shop trip as a `VEVENT`. Life keeps no trip table, so all the shop needs is
//! in the event.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use icalendar::{Alarm, Calendar, Component, Event, EventLike, Property};

pub struct ShopTrip {
    /// As typed: life has no shop entity, and validating would refuse the corner shop.
    pub shop: String,
    pub starts_at: DateTime<Utc>,
    pub minutes: i64,
    /// In list order; may be empty.
    pub items: Vec<String>,
}

const REMIND_BEFORE_MINUTES: i64 = 30;

/// A bound on a client's number, so a typo cannot fill next week.
const MAX_MINUTES: i64 = 8 * 60;

/// Or the `icalendar` crate names itself.
const PRODID: &str = "-//Xinutec//life//EN";

/// Past this, the description stops being readable on a lock screen.
const MAX_LISTED: usize = 60;

pub fn summary(shop: &str) -> String {
    format!("Shop at {shop}")
}

/// `now` is passed in, so a test chooses the `DTSTAMP`.
pub fn ics(trip: &ShopTrip, uid: &str, now: DateTime<Utc>) -> Result<String> {
    let shop = trip.shop.trim();
    if shop.is_empty() {
        bail!("a trip needs a shop");
    }
    if trip.minutes <= 0 || trip.minutes > MAX_MINUTES {
        bail!("a trip lasts between a minute and {MAX_MINUTES} minutes");
    }

    let title = summary(shop);
    let mut event = Event::new();
    event
        .uid(uid)
        .timestamp(now)
        .summary(&title)
        // Free text: `GEO` is a map pin in some clients and nothing in others.
        .location(shop)
        .starts(trip.starts_at)
        .ends(trip.starts_at + Duration::minutes(trip.minutes));

    if let Some(list) = shopping_list(&trip.items) {
        event.description(&list);
    }

    // The reminder is the point: an event nobody is told about is just a note.
    event.alarm(Alarm::display(
        &title,
        -Duration::minutes(REMIND_BEFORE_MINUTES),
    ));

    let mut calendar = Calendar::new();
    calendar.push(event.done());
    // Replaced: a second `PRODID` is malformed.
    calendar.properties.retain(|p| p.key() != "PRODID");
    calendar.append_property(Property::new("PRODID", PRODID));
    Ok(calendar.done().to_string())
}

/// Blank entries dropped; past [`MAX_LISTED`] the rest are counted, as a list
/// silently cut would be read in the shop as the whole list.
fn shopping_list(items: &[String]) -> Option<String> {
    let named: Vec<&str> = items
        .iter()
        .map(|i| i.trim())
        .filter(|i| !i.is_empty())
        .collect();
    if named.is_empty() {
        return None;
    }
    let mut out = String::from("From the Buy list:");
    for name in named.iter().take(MAX_LISTED) {
        out.push_str("\n• ");
        out.push_str(name);
    }
    if let Some(rest) = named.len().checked_sub(MAX_LISTED).filter(|r| *r > 0) {
        out.push_str(&format!("\n… and {rest} more on the list"));
    }
    Some(out)
}
