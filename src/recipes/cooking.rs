//! "I cooked this": what a recipe takes out of the cupboard. Pure. Every line is
//! reported: many cannot be settled ("salt", jars against grams), and a report of
//! successes only would overstate what changed.

use std::collections::HashMap;

use super::matching::stock_for;
use super::types::{Recipe, RecipeIngredient};
use crate::inventory::consume::same_unit;
use crate::inventory::types::{Item, ItemId};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Take {
    #[ts(type = "number")]
    pub item_id: ItemId,
    /// So the report can name it without a second read.
    pub name: String,
    pub amount: f64,
    /// What the row holds afterwards.
    pub left: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Untouched {
    NoStock,
    /// No amount ("salt"): the commonest case, and not a problem.
    NoAmount,
    /// Matching stock, none of it in a comparable unit or with a quantity.
    NoComparableStock,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[ts(export)]
pub enum LineOutcome {
    Took {
        from: Vec<Take>,
    },
    /// Took all there was and came up `short`; the food was cooked either way.
    Short {
        from: Vec<Take>,
        short: f64,
    },
    Untouched {
        why: Untouched,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CookedLine {
    pub ingredient: String,
    /// Shared by every take; `None` for a countable line.
    pub unit: Option<String>,
    #[serde(flatten)]
    pub outcome: LineOutcome,
}

/// Rows in draining order: soonest expiry first (none last), then the smallest
/// amount, as you would cook. `remaining` threads the plan so far.
fn drain_order<'a>(
    matches: &[&'a Item],
    unit: Option<&str>,
    remaining: &HashMap<ItemId, f64>,
) -> Vec<&'a Item> {
    let mut usable: Vec<&Item> = matches
        .iter()
        .copied()
        .filter(|it| same_unit(it.unit.as_deref(), unit) && left_of(it, remaining) > 0.0)
        .collect();
    usable.sort_by(|a, b| {
        (a.expiry.is_none(), a.expiry)
            .cmp(&(b.expiry.is_none(), b.expiry))
            .then_with(|| left_of(a, remaining).total_cmp(&left_of(b, remaining)))
            // Then the id, for a stable order.
            .then_with(|| a.id.cmp(&b.id))
    });
    usable
}

fn left_of(item: &Item, remaining: &HashMap<ItemId, f64>) -> f64 {
    remaining
        .get(&item.id)
        .copied()
        .unwrap_or(item.quantity.unwrap_or(0.0))
}

fn plan_line(
    ingredient: &RecipeIngredient,
    inventory: &[Item],
    remaining: &mut HashMap<ItemId, f64>,
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

/// One line per ingredient, including those nothing happened to.
pub fn plan(recipe: &Recipe, inventory: &[Item]) -> Vec<CookedLine> {
    plan_ingredients(&recipe.ingredients, inventory)
}

pub(super) fn plan_ingredients(
    ingredients: &[RecipeIngredient],
    inventory: &[Item],
) -> Vec<CookedLine> {
    // Threaded across lines, so two naming one thing drain it once between them.
    let mut remaining: HashMap<ItemId, f64> = HashMap::new();
    ingredients
        .iter()
        .map(|ing| CookedLine {
            ingredient: ing.name.clone(),
            unit: ing
                .unit
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_string),
            outcome: plan_line(ing, inventory, &mut remaining),
        })
        .collect()
}

/// Every take, in order; the two readers below fold it differently.
fn takes(lines: &[CookedLine]) -> impl Iterator<Item = &Take> {
    lines.iter().flat_map(|l| match &l.outcome {
        LineOutcome::Took { from } | LineOutcome::Short { from, .. } => from.as_slice(),
        LineOutcome::Untouched { .. } => &[],
    })
}

/// What each touched row should hold afterwards; the last word wins.
pub fn settled(lines: &[CookedLine]) -> Vec<(ItemId, f64)> {
    let mut out: Vec<(ItemId, f64)> = Vec::new();
    for take in takes(lines) {
        match out.iter_mut().find(|(id, _)| *id == take.item_id) {
            Some(entry) => entry.1 = take.left,
            None => out.push((take.item_id, take.left)),
        }
    }
    out
}

/// How much came off each row: what the history records.
pub fn taken_per_row(lines: &[CookedLine]) -> Vec<(ItemId, f64)> {
    let mut out: Vec<(ItemId, f64)> = Vec::new();
    for take in takes(lines) {
        match out.iter_mut().find(|(id, _)| *id == take.item_id) {
            Some(entry) => entry.1 += take.amount,
            None => out.push((take.item_id, take.amount)),
        }
    }
    out
}
