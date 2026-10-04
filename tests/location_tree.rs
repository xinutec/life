//! Which locations a delete or restore of one takes with it. Pure.

use life::inventory::tree::subtree;
use life::inventory::types::LocationId;

/// A tree as `(id, parent)` rows, written as plain numbers.
fn rows(edges: &[(u64, Option<u64>)]) -> Vec<(LocationId, Option<LocationId>)> {
    edges
        .iter()
        .map(|(id, parent)| (LocationId(*id), parent.map(LocationId)))
        .collect()
}

fn ids(of: Vec<LocationId>) -> Vec<u64> {
    let mut ids: Vec<u64> = of.into_iter().map(|l| l.0).collect();
    ids.sort_unstable();
    ids
}

#[test]
fn a_location_takes_everything_below_it_and_nothing_beside_it() {
    // house 1 > kitchen 2 > cupboard 3 > shelf 4; hall 5 beside the kitchen.
    let tree = rows(&[
        (1, None),
        (2, Some(1)),
        (3, Some(2)),
        (4, Some(3)),
        (5, Some(1)),
    ]);
    assert_eq!(ids(subtree(&tree, LocationId(2))), [2, 3, 4]);
    assert_eq!(
        ids(subtree(&tree, LocationId(4))),
        [4],
        "a leaf is just itself"
    );
}

#[test]
fn a_location_that_is_not_there_takes_nothing() {
    assert!(subtree(&rows(&[(1, None)]), LocationId(9)).is_empty());
}

#[test]
fn a_loop_in_the_parent_links_still_ends() {
    // Nothing should write one, but a walk that never ends would hang a request.
    let tree = rows(&[(1, Some(2)), (2, Some(1))]);
    assert_eq!(ids(subtree(&tree, LocationId(1))), [1, 2]);
}
