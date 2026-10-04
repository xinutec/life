//! The location tree as data: which rows sit below which. Pure; the repo reads
//! the rows and applies the answer.

use std::collections::{HashMap, HashSet};

use super::types::LocationId;

/// Every id in the subtree rooted at `root`, `root` first, from `(id, parent)`
/// rows. Empty when `root` is not among them. Each id is visited once, so a loop
/// in the parent links ends the walk instead of hanging it.
pub fn subtree(rows: &[(LocationId, Option<LocationId>)], root: LocationId) -> Vec<LocationId> {
    if !rows.iter().any(|(id, _)| *id == root) {
        return Vec::new();
    }
    let mut children: HashMap<LocationId, Vec<LocationId>> = HashMap::new();
    for (id, parent) in rows {
        if let Some(p) = parent {
            children.entry(*p).or_default().push(*id);
        }
    }
    let mut seen = HashSet::from([root]);
    let mut ids = vec![root];
    let mut i = 0;
    while i < ids.len() {
        for kid in children.get(&ids[i]).into_iter().flatten() {
            if seen.insert(*kid) {
                ids.push(*kid);
            }
        }
        i += 1;
    }
    ids
}
