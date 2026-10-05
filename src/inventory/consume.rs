//! Taking an amount out of a stock row — the arithmetic behind "I used 200g of
//! flour", kept pure so the rule is tested without a database.
//!
//! **Units are compared, never converted.** `200 g` out of a row measured in
//! `jar` takes nothing and says so, rather than inventing grams per jar. `g`
//! versus `kg` too: conversion is its own feature (is `oz` mass or fluid?).

/// What a stock row holds, as far as taking from it goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held<'a> {
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

/// What happened when we tried to take `want` out of a row.
#[derive(Debug, Clone, PartialEq)]
pub enum Taken {
    /// Taken in full. The row now holds this much, possibly `0.0`.
    ///
    /// A row that reaches zero is **kept**, not deleted: "we have none" is
    /// knowledge — it's what makes the thing worth putting back on the Buy list
    /// — and deleting the row would throw that away along with its history.
    Left(f64),
    /// There was some, but not enough. The row is emptied and `short` says by
    /// how much the cooking outran the cupboard, because the food really was
    /// used even though the numbers didn't cover it.
    Emptied { short: f64 },
    /// The row doesn't measure itself in the unit asked for, so nothing moved.
    UnitMismatch,
    /// The row carries no quantity at all (a jar of "cumin", no number), so
    /// there is nothing to subtract from. Not an error — most stock is like
    /// this, and it simply isn't the kind of thing this can track.
    Untracked,
}

/// Units agree if they match once trimmed and lower-cased, and two absent units
/// agree with each other (a countable thing — "2 eggs" against "6 eggs"). A
/// blank unit is an absent one. The one unit rule: cooking compares through it
/// too.
pub fn same_unit(a: Option<&str>, b: Option<&str>) -> bool {
    let key = |u: Option<&str>| u.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    key(a) == key(b)
}

/// Take `want` (measured in `want_unit`) out of what a row holds.
///
/// `want` must be positive; zero or negative takes nothing and reports
/// [`Taken::Left`] unchanged, because "use none of it" is a no-op rather than a
/// way to add stock back.
pub fn take(held: Held<'_>, want: f64, want_unit: Option<&str>) -> Taken {
    let Some(have) = held.quantity else {
        return Taken::Untracked;
    };
    if !same_unit(held.unit, want_unit) {
        return Taken::UnitMismatch;
    }
    // Spelled out rather than negating a comparison: a NaN must fall in here
    // too (every comparison against it is false, so a `> 0.0` guard would let
    // it through and poison the stored quantity), and `is_finite` says that
    // out loud where `!(want > 0.0)` left it to be inferred.
    if !want.is_finite() || want <= 0.0 {
        return Taken::Left(have);
    }
    if want >= have {
        // `>=` so taking exactly what's there empties it without a `short`.
        let short = want - have;
        if short > 0.0 {
            return Taken::Emptied { short };
        }
        return Taken::Left(0.0);
    }
    Taken::Left(have - want)
}
