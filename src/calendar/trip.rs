//! The shop trip, as a calendar event.
//!
//! Scheduling is delegated to Nextcloud Calendar rather than kept in a life
//! table (docs/design/overview.md §5), so a planned trip has to leave here as a
//! `VEVENT` and nothing else. Everything a phone shows at 09:30 on a Saturday —
//! the shop, where it is, what to get, the nudge beforehand — has to be *in the
//! event*, because by then life is not the app in front of you.
//!
//! Building the text is kept apart from sending it ([`super::caldav`]) so the
//! part with all the judgement in it can be tested without a Nextcloud.

use anyhow::{Result, bail};
use chrono::{DateTime, Duration, Utc};
use icalendar::{Alarm, Calendar, Component, Event, EventLike};

/// A planned trip to one shop.
pub struct ShopTrip {
    /// Named as the Buy list names it — "Asda", "Waitrose", or whatever was
    /// typed. life has no shop entity, and inventing one to validate against
    /// would refuse the corner shop.
    pub shop: String,
    pub starts_at: DateTime<Utc>,
    pub minutes: i64,
    /// What to get, in the order the list has it. May be empty — a trip is
    /// worth putting in the diary before you know what's on it.
    pub items: Vec<String>,
}

/// How long before the trip to nudge. Long enough to put your shoes on, short
/// enough that the reminder is still about *this* trip and not a note for the
/// day ahead.
const REMIND_BEFORE_MINUTES: i64 = 30;

/// The longest trip we'll write. Not a rule about shopping — a bound on a
/// number that arrives from a client, so a fat-fingered field can't write an
/// event across next week.
const MAX_MINUTES: i64 = 8 * 60;

/// The most items to spell out in the description. A Buy list is a trolley, not
/// a catalogue; past this the event stops being readable on a lock screen and
/// the list itself is the better place to look.
const MAX_LISTED: usize = 60;

/// The event's title, and the one line most calendar views will show.
pub fn summary(shop: &str) -> String {
    format!("Shop at {shop}")
}

/// Render the trip as a complete `VCALENDAR` document.
///
/// `now` is passed in rather than read, so the `DTSTAMP` a test asserts on is
/// the one the test chose.
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
        // Free text, deliberately: `GEO` coordinates render as a map pin in some
        // clients and as nothing in others, and the shop's name is the part that
        // is right in all of them. See overview.md §5.
        .location(shop)
        .starts(trip.starts_at)
        .ends(trip.starts_at + Duration::minutes(trip.minutes));

    if let Some(list) = shopping_list(&trip.items) {
        event.description(&list);
    }

    // The reminder is the point of writing this at all: an event nobody is
    // told about is a note, and the Buy list is already a better note.
    event.alarm(Alarm::display(
        &title,
        -Duration::minutes(REMIND_BEFORE_MINUTES),
    ));

    Ok(Calendar::new().push(event.done()).done().to_string())
}

/// The Buy list as description text, or `None` when there is nothing to say.
///
/// Blank entries are dropped rather than rendered as empty bullets, and the
/// tail past [`MAX_LISTED`] is *counted* rather than silently cut — a list that
/// quietly stopped at sixty would be read in the shop as the whole list.
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
