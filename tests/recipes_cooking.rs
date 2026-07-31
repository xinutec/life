//! What cooking a recipe takes out of the cupboard. Pure — no DB.
//!
//! The rules under test are mostly about restraint: what it refuses to do, and
//! what it insists on telling you about.

use chrono::NaiveDate;
use life::inventory::types::{Item, ItemCategory};
use life::recipes::cooking::{CookedLine, LineOutcome, Take, Untouched, plan, settled};
use life::recipes::types::{Recipe, RecipeIngredient};

fn item(id: u64, name: &str, quantity: Option<f64>, unit: Option<&str>) -> Item {
    Item {
        id,
        product_id: None,
        name: name.into(),
        brand: None,
        category: ItemCategory::Food,
        quantity,
        unit: unit.map(Into::into),
        expiry: None,
        location_id: None,
        barcode: None,
        has_image: false,
    }
}

fn expiring(mut it: Item, on: &str) -> Item {
    it.expiry = Some(on.parse::<NaiveDate>().unwrap());
    it
}

fn ing(name: &str, quantity: Option<f64>, unit: Option<&str>) -> RecipeIngredient {
    RecipeIngredient {
        name: name.into(),
        product_id: None,
        product_name: None,
        quantity,
        unit: unit.map(Into::into),
    }
}

fn recipe(ingredients: Vec<RecipeIngredient>) -> Recipe {
    Recipe {
        id: 1,
        name: "Test bake".into(),
        instructions: None,
        servings: None,
        ingredients,
    }
}

fn outcome(lines: &[CookedLine], ingredient: &str) -> LineOutcome {
    lines
        .iter()
        .find(|l| l.ingredient == ingredient)
        .unwrap_or_else(|| panic!("no line for {ingredient}"))
        .outcome
        .clone()
}

#[test]
fn an_ingredient_comes_off_the_matching_row() {
    let lines = plan(
        &recipe(vec![ing("flour", Some(200.0), Some("g"))]),
        &[item(1, "flour", Some(950.0), Some("g"))],
    );
    assert_eq!(
        outcome(&lines, "flour"),
        LineOutcome::Took {
            from: vec![Take {
                item_id: 1,
                name: "flour".into(),
                amount: 200.0,
                left: 750.0
            }]
        }
    );
    assert_eq!(settled(&lines), [(1, 750.0)]);
}

#[test]
fn every_ingredient_is_reported_even_the_untouched_ones() {
    // The point of the whole module: a cook button that silently does a third
    // of what it looks like it does would leave you trusting wrong numbers.
    let lines = plan(
        &recipe(vec![
            ing("flour", Some(200.0), Some("g")),
            ing("salt", None, None),
            ing("saffron", Some(1.0), Some("pinch")),
        ]),
        &[
            item(1, "flour", Some(950.0), Some("g")),
            item(2, "salt", None, None),
        ],
    );
    assert_eq!(lines.len(), 3, "one line in, one line out");
    assert_eq!(
        outcome(&lines, "salt"),
        LineOutcome::Untouched {
            why: Untouched::NoAmount
        },
        "a recipe that just says 'salt' has no amount to subtract"
    );
    assert_eq!(
        outcome(&lines, "saffron"),
        LineOutcome::Untouched {
            why: Untouched::NoStock
        }
    );
}

#[test]
fn a_jar_is_not_grams() {
    // Matched stock, but nothing measured comparably. Units are never
    // converted, so this takes nothing and says which kind of nothing.
    let lines = plan(
        &recipe(vec![ing("cumin", Some(2.0), Some("g"))]),
        &[item(1, "cumin", Some(1.0), Some("jar"))],
    );
    assert_eq!(
        outcome(&lines, "cumin"),
        LineOutcome::Untouched {
            why: Untouched::NoComparableStock
        }
    );
    assert_eq!(settled(&lines), [], "and writes nothing");
}

#[test]
fn a_line_spreads_across_rows_until_it_is_satisfied() {
    let lines = plan(
        &recipe(vec![ing("flour", Some(700.0), Some("g"))]),
        &[
            item(1, "flour", Some(500.0), Some("g")),
            item(2, "flour", Some(300.0), Some("g")),
        ],
    );
    // Smallest first, so the nearly-empty packet is finished rather than left.
    assert_eq!(
        outcome(&lines, "flour"),
        LineOutcome::Took {
            from: vec![
                Take {
                    item_id: 2,
                    name: "flour".into(),
                    amount: 300.0,
                    left: 0.0
                },
                Take {
                    item_id: 1,
                    name: "flour".into(),
                    amount: 400.0,
                    left: 100.0
                },
            ]
        }
    );
}

#[test]
fn the_soonest_to_expire_goes_first() {
    // Ahead of "smallest": using up the thing about to go off is the point of
    // knowing its date at all. The 500g bag expires first, so it goes first
    // even though the 300g one is smaller.
    let lines = plan(
        &recipe(vec![ing("cream", Some(200.0), Some("ml"))]),
        &[
            item(1, "cream", Some(300.0), Some("ml")),
            expiring(item(2, "cream", Some(500.0), Some("ml")), "2026-08-02"),
        ],
    );
    assert_eq!(
        outcome(&lines, "cream"),
        LineOutcome::Took {
            from: vec![Take {
                item_id: 2,
                name: "cream".into(),
                amount: 200.0,
                left: 300.0
            }]
        }
    );
}

#[test]
fn cooking_more_than_you_had_empties_the_rows_and_says_by_how_much() {
    let lines = plan(
        &recipe(vec![ing("flour", Some(1200.0), Some("g"))]),
        &[item(1, "flour", Some(950.0), Some("g"))],
    );
    assert_eq!(
        outcome(&lines, "flour"),
        LineOutcome::Short {
            from: vec![Take {
                item_id: 1,
                name: "flour".into(),
                amount: 950.0,
                left: 0.0
            }],
            short: 250.0
        }
    );
    assert_eq!(
        settled(&lines),
        [(1, 0.0)],
        "the row is emptied, not skipped"
    );
}

#[test]
fn two_lines_naming_the_same_thing_drain_it_once_between_them() {
    // A recipe really can list "flour" twice (dough and dusting). Planning each
    // line against the ORIGINAL amount would take 400g off a 500g bag twice and
    // report 300g left both times — a number that was never true.
    let lines = plan(
        &recipe(vec![
            ing("flour", Some(400.0), Some("g")),
            ing("flour", Some(200.0), Some("g")),
        ]),
        &[item(1, "flour", Some(500.0), Some("g"))],
    );
    let takes: Vec<&CookedLine> = lines.iter().collect();
    assert_eq!(
        takes[0].outcome,
        LineOutcome::Took {
            from: vec![Take {
                item_id: 1,
                name: "flour".into(),
                amount: 400.0,
                left: 100.0
            }]
        }
    );
    assert_eq!(
        takes[1].outcome,
        LineOutcome::Short {
            from: vec![Take {
                item_id: 1,
                name: "flour".into(),
                amount: 100.0,
                left: 0.0
            }],
            short: 100.0
        },
        "the second line sees what the first already took"
    );
    assert_eq!(settled(&lines), [(1, 0.0)], "one row, one final amount");
}

#[test]
fn an_empty_row_is_not_drawn_from() {
    // Zero-quantity rows are kept deliberately (see inventory::consume), so the
    // planner has to step over them rather than emit a pointless no-op take.
    let lines = plan(
        &recipe(vec![ing("flour", Some(100.0), Some("g"))]),
        &[
            item(1, "flour", Some(0.0), Some("g")),
            item(2, "flour", Some(400.0), Some("g")),
        ],
    );
    assert_eq!(
        outcome(&lines, "flour"),
        LineOutcome::Took {
            from: vec![Take {
                item_id: 2,
                name: "flour".into(),
                amount: 100.0,
                left: 300.0
            }]
        }
    );
}

#[test]
fn a_recipe_with_nothing_in_the_cupboard_writes_nothing() {
    let lines = plan(&recipe(vec![ing("flour", Some(200.0), Some("g"))]), &[]);
    assert_eq!(
        outcome(&lines, "flour"),
        LineOutcome::Untouched {
            why: Untouched::NoStock
        }
    );
    assert_eq!(settled(&lines), []);
}
