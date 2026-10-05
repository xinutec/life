//! Taking an amount out of a stock row, pure. Units are compared, never converted:
//! `200 g` out of a `jar` takes nothing, and `g` against `kg` too.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held<'a> {
    pub quantity: Option<f64>,
    pub unit: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Taken {
    /// Taken in full; the row now holds this, perhaps `0.0`. A row at zero is
    /// kept: "we have none" is what puts it back on the Buy list.
    Left(f64),
    /// Not enough: emptied, and `short` by this much.
    Emptied { short: f64 },
    /// The row is measured in another unit; nothing moved.
    UnitMismatch,
    /// The row has no quantity ("cumin", no number); most stock is like this.
    Untracked,
}

/// Trimmed and lower-cased; a blank unit is no unit, and two absent units agree
/// ("2 eggs" against "6 eggs"). Cooking compares through this too.
pub fn same_unit(a: Option<&str>, b: Option<&str>) -> bool {
    let key = |u: Option<&str>| u.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    key(a) == key(b)
}

/// Zero or negative `want` takes nothing: "use none" never adds stock back.
pub fn take(held: Held<'_>, want: f64, want_unit: Option<&str>) -> Taken {
    let Some(have) = held.quantity else {
        return Taken::Untracked;
    };
    if !same_unit(held.unit, want_unit) {
        return Taken::UnitMismatch;
    }
    // Spelled out so a NaN falls in here too: every comparison with it is false.
    if !want.is_finite() || want <= 0.0 {
        return Taken::Left(have);
    }
    if want >= have {
        // `>=`, so taking exactly what is there leaves no `short`.
        let short = want - have;
        if short > 0.0 {
            return Taken::Emptied { short };
        }
        return Taken::Left(0.0);
    }
    Taken::Left(have - want)
}
