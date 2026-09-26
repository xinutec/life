//! Calendar. In: the council's bin collections from a public iCal feed
//! ([`bins`]), an input to planning (docs/design/overview.md §5). Out: shop
//! trips as `VEVENT`s ([`trip`]) written to Nextcloud over CalDAV ([`caldav`]).
//!
//! Neither direction keeps a life table: the council owns the bins, Nextcloud
//! the diary, so a trip shows up in every calendar client.

pub mod bins;
pub mod caldav;
pub mod trip;
