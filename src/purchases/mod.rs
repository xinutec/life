//! What was paid for a thing, and where — the purchase half of "price is an
//! observation, not a product attribute".
//!
//! The shelf half lives in [`crate::products::prices`]: what a shop charges,
//! scraped, per listing. This half is about a person's own money, so it is
//! user-scoped, append-only, and outlives every key it is filed under
//! (migration 0043).

pub mod repo;
pub mod types;
