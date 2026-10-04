//! Which locations a delete or restore of one takes with it. Pure.

use life::inventory::tree::subtree;

#[test]
fn a_location_takes_everything_below_it_and_nothing_beside_it() {
    // house 1 > kitchen 2 > cupboard 3 > shelf 4; hall 5 beside the kitchen.
    let rows = [
        (1, None),
        (2, Some(1)),
        (3, Some(2)),
        (4, Some(3)),
        (5, Some(1)),
    ];
    let mut got = subtree(&rows, 2);
    got.sort_unstable();
    assert_eq!(got, [2, 3, 4]);
    assert_eq!(subtree(&rows, 4), [4], "a leaf is just itself");
}

#[test]
fn a_location_that_is_not_there_takes_nothing() {
    assert!(subtree(&[(1, None)], 9).is_empty());
}

#[test]
fn a_loop_in_the_parent_links_still_ends() {
    // Nothing should write one, but a walk that never ends would hang a request.
    let rows = [(1, Some(2)), (2, Some(1))];
    let mut got = subtree(&rows, 1);
    got.sort_unstable();
    assert_eq!(got, [1, 2]);
}
