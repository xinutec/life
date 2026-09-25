//! When the bins go out, read off the council's own calendar.
//!
//! The council publishes a property's collection schedule as a public iCal
//! subscription, the same feed a calendar client would subscribe to. Its URL
//! identifies one address, so it is configuration ([`crate::config`]).
//!
//! Parsed with [`icalendar`], not by line: iCal folds lines and escapes `,` `;`.

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
    // The feed is all-day (`VALUE=DATE`), the first arm. The others keep the
    // day if a time ever appears; `Utc` takes its UTC date, which can be a day
    // off from London's just before midnight in summer.
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
