//! Calendar: what the household's schedule says, and what life puts on it.
//!
//! Two directions. **In**: the council's bin collections, read from a public
//! iCal subscription ([`bins`]) — an input to planning ("don't schedule a shop
//! the morning the bins go out"), see docs/design/overview.md §5. **Out**:
//! shop trips, built as `VEVENT`s ([`trip`]) and written to Nextcloud Calendar
//! over CalDAV ([`caldav`]) with the app password from Login Flow v2 (§2b).
//!
//! Neither direction keeps a life table. The council owns the bin schedule and
//! Nextcloud owns the diary; life reads one, writes the other, and stores
//! nothing in between — which is what makes a trip show up in every calendar
//! client for free.

pub mod bins;
pub mod caldav;
pub mod trip;
