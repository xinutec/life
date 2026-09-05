//! Every string-backed enum survives the round trip it is stored under.
//!
//! ⚠ **This is a regression guard, not the guarantee.** The guarantee is that
//! `str_enum!` generates `Display`, `FromStr` and `ALL` from ONE table, so they
//! cannot disagree — a hand-written `ALL` would make this test as forgettable as
//! the arm it is checking (see the macro's own docs). What this catches is
//! someone hand-rolling one of these pairs again, or the macro itself going
//! wrong.
//!
//! Two properties, and the second is the one a reader might not expect:
//!
//! 1. `parse(to_string(v)) == v` for every variant.
//! 2. No two variants share a string. They would both write happily and one
//!    would read back as the other — a silent relabelling of stored rows, which
//!    is worse than a parse failure because nothing ever reports it.

use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::str::FromStr;

fn round_trips<T>(type_name: &str, all: &'static [T])
where
    T: Copy + Debug + Display + PartialEq + FromStr,
    <T as FromStr>::Err: Debug,
{
    assert!(!all.is_empty(), "{type_name}: ALL is empty");
    let mut claimed: HashMap<String, String> = HashMap::new();
    for variant in all {
        let written = variant.to_string();
        let read = T::from_str(&written).unwrap_or_else(|e| {
            panic!("{type_name}::{variant:?} writes {written:?}, which does not parse back: {e:?}")
        });
        assert_eq!(
            read, *variant,
            "{type_name}: {written:?} was written by {variant:?} but reads back as {read:?}"
        );
        if let Some(other) = claimed.insert(written.clone(), format!("{variant:?}")) {
            panic!("{type_name}: {other} and {variant:?} both store as {written:?}");
        }
    }
}

/// One line per type, so adding a `str_enum!` and forgetting this is the only
/// way left to go uncovered — and that is a missing line in one file rather than
/// a missing arm in a match nobody re-reads.
macro_rules! check {
    ($($ty:ty),+ $(,)?) => {
        $( round_trips(stringify!($ty), <$ty>::ALL); )+
    };
}

#[test]
fn every_string_backed_enum_round_trips() {
    use life::conflicts::ConflictKind;
    use life::inventory::types::{
        ExpiryPrecision, ItemCategory, ItemEvent, ItemNameSource, LocationKind,
    };
    use life::products::nutrition::{Claim, Presence};
    use life::products::source::Source;
    use life::products::types::ReconcileField;
    use life::todo::types::{LinkKind, TargetKind, TodoPriority, TodoStatus, TodoType};
    use life::trash::TrashKind;

    check!(
        LocationKind,
        ItemNameSource,
        ExpiryPrecision,
        ItemCategory,
        ItemEvent,
        Source,
        Presence,
        Claim,
        ReconcileField,
        ConflictKind,
        TrashKind,
        TodoType,
        TodoStatus,
        TodoPriority,
        LinkKind,
        TargetKind,
    );
}
