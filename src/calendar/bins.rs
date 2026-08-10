//! When the bins go out, read off the council's own calendar.
//!
//! Brent publishes the collection schedule for a property as a public iCal
//! subscription — no auth, no key, `X-PUBLISHED-TTL:P1D`. It is the same feed a
//! calendar client would subscribe to, so reading it is the intended use and
//! not scraping.
//!
//! **The URL is configuration, never a constant here.** It carries a property
//! id that identifies one address, and this repository is not the place for
//! that. Unset means the feature is simply absent — see [`crate::config`].
//!
//! Parsing is [`icalendar`]'s rather than a hand-rolled line reader: iCal folds
//! long lines at 75 octets and escapes `,` `;` and newlines inside text, and a
//! reader that ignores either works right up until a council renames a
//! collection to something with a comma in it.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use icalendar::{CalendarDateTime, Component, DatePerhapsTime, Event};
use serde::Serialize;
use ts_rs::TS;

/// One collection, on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BinDay {
    /// The council's own name for it — "Food waste collection", "Rubbish
    /// collection". Passed through verbatim rather than mapped onto an enum of
    /// our own: the set is the council's to change, and a collection we failed
    /// to recognise must still appear rather than vanish into an `Other`.
    pub kind: String,
    /// The day it is collected, as the feed states it. All-day events, so there
    /// is no time and inventing one ("07:00") would be a claim nobody made.
    pub date: NaiveDate,
}

/// Every collection in the feed, soonest first, from `on` onwards.
///
/// Past collections are dropped here rather than by the caller: the feed keeps
/// a rolling window that reaches into last week, and "when do the bins go out"
/// has no interest in the ones that already went.
pub fn upcoming(ics: &str, on: NaiveDate) -> Result<Vec<BinDay>> {
    let parsed: icalendar::Calendar = ics
        .parse()
        .map_err(|e: String| anyhow::anyhow!("parsing the bins feed: {e}"))
        .context("the feed was not iCalendar")?;
    let mut days: Vec<BinDay> = parsed
        .components
        .iter()
        .filter_map(|c| c.as_event())
        .filter_map(day_of)
        .filter(|d| d.date >= on)
        .collect();
    // Soonest first, and a stable order within a day: two collections often
    // land on the same morning, and a list that reshuffled them between loads
    // would read as though something had changed.
    days.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.kind.cmp(&b.kind)));
    Ok(days)
}

/// One event as a collection, or `None` if it is not one we can place.
///
/// An event with no summary or no start is skipped rather than guessed at —
/// there is nothing useful to say about a collection whose name or day we do
/// not know, and a row reading "collection, sometime" is worse than no row.
fn day_of(event: &Event) -> Option<BinDay> {
    let kind = event.get_summary()?.trim();
    if kind.is_empty() {
        return None;
    }
    // Every event in this feed is all-day (`VALUE=DATE`), which is the first
    // arm. The others are defensive: if the council ever starts stating a time,
    // the DAY is still the part anyone acts on, so take it as written rather
    // than dropping the collection. `Utc` is read as its UTC date, which in
    // British Summer Time can differ from the London date for something just
    // before midnight — the wrong answer to a question nobody is yet asking,
    // and worth revisiting only if the feed ever stops being all-day.
    let date = match event.get_start()? {
        DatePerhapsTime::Date(d) => d,
        DatePerhapsTime::DateTime(CalendarDateTime::Floating(dt)) => dt.date(),
        DatePerhapsTime::DateTime(CalendarDateTime::Utc(dt)) => dt.date_naive(),
        DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone { date_time, .. }) => {
            date_time.date()
        }
    };
    Some(BinDay {
        kind: kind.to_string(),
        date,
    })
}
