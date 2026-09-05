//! "I cooked this" — working out what a recipe takes out of the cupboard.
//!
//! Pure: this decides, the repo writes. Everything here is the rule
//! [[crate::inventory::consume]] applies to one row, spread across the several
//! rows an ingredient might be satisfied from.
//!
//! **A line that can't be settled is reported, never skipped.** Most ingredients
//! won't be decrementable — "salt" with no quantity, a jar measured in jars
//! against a recipe measured in grams — and that is fine. What isn't fine is a
//! cook button that silently does a third of what it looks like it does: you'd
//! trust the numbers afterwards, and they'd be wrong in a direction you couldn't
//! see. So every line comes back with what happened to it.

use std::collections::HashMap;

use super::matching::{norm, stock_for};
use super::types::{Recipe, RecipeIngredient};
use crate::inventory::types::Item;
use serde::Serialize;
use ts_rs::TS;

/// How much to take off one stock row.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Take {
    #[ts(type = "number")]
    pub item_id: u64,
    /// What the row is called, so the report can name it without a second read.
    pub name: String,
    pub amount: f64,
    /// What the row holds once this is taken off.
    pub left: f64,
}

/// Why an ingredient line came away untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Untouched {
    /// Nothing in the cupboard matches this ingredient at all.
    NoStock,
    /// The recipe doesn't say how much (just "salt"), so there is no amount to
    /// subtract. The commonest case by far, and not a problem.
    NoAmount,
    /// Stock matched, but none of it is measured comparably — a jar against a
    /// recipe's grams, or a row carrying no quantity. Units are never converted
    /// (see [[crate::inventory::consume]]).
    NoComparableStock,
}

/// What cooking does to one ingredient line.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[ts(export)]
pub enum LineOutcome {
    /// Taken in full, off these rows.
    Took {
        from: Vec<Take>,
    },
    /// Took everything comparable there was and still came up `short`. The food
    /// was cooked either way; the cupboard's number was just behind.
    Short {
        from: Vec<Take>,
        short: f64,
    },
    Untouched {
        why: Untouched,
    },
}

/// One line of the report the cook button hands back.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CookedLine {
    pub ingredient: String,
    #[serde(flatten)]
    pub outcome: LineOutcome,
}

/// Which rows can serve an ingredient, in the order to drain them.
///
/// **Soonest expiry first, then the smallest amount.** Both halves are how you
/// would actually cook: use the thing that's about to go off, and finish the
/// nearly-empty packet before opening a full one. It also leaves fewer
/// part-used rows behind, and every one of those is a row you'd later have to
/// think about. Rows with no expiry sort after rows with one — a date is a
/// reason to hurry and its absence isn't.
///
/// `remaining` is what each row holds *so far in this plan*, which is not
/// necessarily what the database says: two ingredient lines can name the same
/// thing, and the second must see what the first already took.
fn drain_order<'a>(
    matches: &[&'a Item],
    unit: Option<&str>,
    remaining: &HashMap<u64, f64>,
) -> Vec<&'a Item> {
    let want = unit.map(norm);
    let mut usable: Vec<&Item> = matches
        .iter()
        .copied()
        .filter(|it| it.unit.as_deref().map(norm) == want && left_of(it, remaining) > 0.0)
        .collect();
    usable.sort_by(|a, b| {
        // A date is a reason to hurry and its absence is not, so a row carrying
        // an expiry sorts ahead of one without.
        (a.expiry.is_none(), a.expiry)
            .cmp(&(b.expiry.is_none(), b.expiry))
            // `total_cmp`, not the bit pattern: it is a total order for EVERY
            // f64. Comparing bits only agrees with the numbers while they are
            // non-negative, which holds here solely because of the `> 0.0` in
            // the filter above — a coupling nothing stated, and one an edit to
            // that filter would quietly break.
            .then_with(|| left_of(a, remaining).total_cmp(&left_of(b, remaining)))
            // Then the id, so the order is total and stable between runs.
            .then_with(|| a.id.cmp(&b.id))
    });
    usable
}

/// What a row holds at this point in the plan.
fn left_of(item: &Item, remaining: &HashMap<u64, f64>) -> f64 {
    remaining
        .get(&item.id)
        .copied()
        .unwrap_or(item.quantity.unwrap_or(0.0))
}

/// What one ingredient takes, updating `remaining` as it goes.
fn plan_line(
    ingredient: &RecipeIngredient,
    inventory: &[Item],
    remaining: &mut HashMap<u64, f64>,
) -> LineOutcome {
    let matches = stock_for(ingredient, inventory);
    if matches.is_empty() {
        return LineOutcome::Untouched {
            why: Untouched::NoStock,
        };
    }
    let Some(needed) = ingredient.quantity.filter(|q| q.is_finite() && *q > 0.0) else {
        return LineOutcome::Untouched {
            why: Untouched::NoAmount,
        };
    };
    let usable = drain_order(&matches, ingredient.unit.as_deref(), remaining);
    if usable.is_empty() {
        return LineOutcome::Untouched {
            why: Untouched::NoComparableStock,
        };
    }

    let mut owed = needed;
    let mut from = Vec::new();
    for it in usable {
        if owed <= 0.0 {
            break;
        }
        let have = left_of(it, remaining);
        let amount = have.min(owed);
        let left = have - amount;
        remaining.insert(it.id, left);
        from.push(Take {
            item_id: it.id,
            name: it.name.clone(),
            amount,
            left,
        });
        owed -= amount;
    }
    if owed > 0.0 {
        LineOutcome::Short { from, short: owed }
    } else {
        LineOutcome::Took { from }
    }
}

/// What cooking this recipe takes out of the cupboard, line by line.
///
/// Every ingredient appears in the result, including the ones nothing happened
/// to — see the module docs for why silence would be the wrong answer.
pub fn plan(recipe: &Recipe, inventory: &[Item]) -> Vec<CookedLine> {
    // Threaded across lines so two ingredients naming the same thing drain it
    // once between them rather than twice each from the original amount.
    let mut remaining: HashMap<u64, f64> = HashMap::new();
    recipe
        .ingredients
        .iter()
        .map(|ing| CookedLine {
            ingredient: ing.name.clone(),
            outcome: plan_line(ing, inventory, &mut remaining),
        })
        .collect()
}

/// Every take the plan makes, in order — the rows an outcome actually moved.
///
/// The two readers below fold this differently (one keeps the last level, one
/// sums the amounts) but walk it identically, so the walk lives once and the
/// two folds stay written out, which is the half worth reading.
fn takes(lines: &[CookedLine]) -> impl Iterator<Item = &Take> {
    lines.iter().flat_map(|l| match &l.outcome {
        LineOutcome::Took { from } | LineOutcome::Short { from, .. } => from.as_slice(),
        LineOutcome::Untouched { .. } => &[],
    })
}

/// Every row the plan touches and what it should hold afterwards, one entry per
/// row — the last word wins, which is the running total `plan` already threaded.
pub fn settled(lines: &[CookedLine]) -> Vec<(u64, f64)> {
    let mut out: Vec<(u64, f64)> = Vec::new();
    for take in takes(lines) {
        match out.iter_mut().find(|(id, _)| *id == take.item_id) {
            Some(entry) => entry.1 = take.left,
            None => out.push((take.item_id, take.left)),
        }
    }
    out
}

/// How much came off each row in total — what the history rows record.
pub fn taken_per_row(lines: &[CookedLine]) -> Vec<(u64, f64)> {
    let mut out: Vec<(u64, f64)> = Vec::new();
    for take in takes(lines) {
        match out.iter_mut().find(|(id, _)| *id == take.item_id) {
            Some(entry) => entry.1 += take.amount,
            None => out.push((take.item_id, take.amount)),
        }
    }
    out
}
