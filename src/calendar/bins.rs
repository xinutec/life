//! When the bins go out, from the council's public iCal feed, whose URL is
//! configuration. Parsed with [`icalendar`]: iCal folds lines and escapes `,` `;`.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use icalendar::{CalendarDateTime, Component, DatePerhapsTime, Event};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct BinDay {
    /// The council's own name, verbatim: a collection we did not recognise must
    /// still appear.
    pub kind: String,
    /// All-day: no time is invented.
    pub date: NaiveDate,
}

/// Soonest first, from `on`: the feed reaches back into last week.
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
    // A stable order within a day, so a reload never seems to change anything.
    days.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.kind.cmp(&b.kind)));
    Ok(days)
}

/// `None` without a summary or a start: a row reading "collection, sometime" is
/// worse than none.
fn day_of(event: &Event) -> Option<BinDay> {
    let kind = event.get_summary()?.trim();
    if kind.is_empty() {
        return None;
    }
    // All-day in practice; `Utc` takes its UTC date, a day off near midnight.
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
