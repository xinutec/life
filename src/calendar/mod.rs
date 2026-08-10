//! Calendar: what the household's schedule says, and what life puts on it.
//!
//! Two directions, and only the first exists so far. **In**: the council's bin
//! collections, read from a public iCal subscription ([`bins`]) — an input to
//! planning ("don't schedule a shop the morning the bins go out"), see
//! docs/design/overview.md §5. **Out**: shop-trip `VEVENT`s written over
//! CalDAV, which needs the Nextcloud app password (§2b) and is not built yet.

pub mod bins;
