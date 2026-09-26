//! Every string-backed enum round-trips (`parse(to_string(v)) == v`), and no two
//! variants share a string, which would silently relabel stored rows.
//!
//! `str_enum!` already derives both directions from one table; this guards
//! against hand-rolled pairs and the macro going wrong.

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
