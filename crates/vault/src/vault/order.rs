//! The order of a vault's rows, and finding one in it.

use std::cmp::Ordering;
use std::path::Component;

use super::{Entry, Row};
use crate::RelPath;
use crate::fs::Kind;

/// Directories first at each level, each followed by its subtree (vault spec, "Ordering").
pub(super) fn compare(a: &Key<'_>, b: &Key<'_>) -> Ordering {
    let (a_components, b_components) = (&a.components, &b.components);
    for (i, (x, y)) in a_components.iter().zip(b_components.iter()).enumerate() {
        if x == y {
            continue;
        }
        let x_is_dir = i + 1 < a_components.len() || a.kind == Kind::Dir;
        let y_is_dir = i + 1 < b_components.len() || b.kind == Kind::Dir;
        return match (x_is_dir, y_is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => x.cmp(y),
        };
    }
    // An ancestor of the other: the ancestor comes first.
    a_components.len().cmp(&b_components.len())
}

/// A path split once, not once per comparison.
pub(super) struct Key<'a> {
    components: Vec<Component<'a>>,
    kind: Kind,
}

impl<'a> Key<'a> {
    pub(super) fn new(path: &'a RelPath, kind: Kind) -> Self {
        Self {
            components: path.as_path().components().collect(),
            kind,
        }
    }
}

pub(super) fn sorted(rows: Vec<Row>) -> Vec<Row> {
    let order: Vec<usize> = {
        let keys: Vec<Key<'_>> = rows
            .iter()
            .map(|row| Key::new(&row.path, row.kind))
            .collect();
        let mut order: Vec<usize> = (0..rows.len()).collect();
        order.sort_by(|&a, &b| compare(&keys[a], &keys[b]));
        order
    };
    let mut rows: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
    order.into_iter().filter_map(|at| rows[at].take()).collect()
}

/// The index of the row at `path` in `entries`, sorted by `compare`, unless that row is not a `kind`.
pub(super) fn position(entries: &[Entry], path: &RelPath, kind: Kind) -> Option<usize> {
    let query = Key::new(path, kind);
    // `compare` reads a kind only where components diverge, so a row at exactly `path` is `Equal`.
    let at = entries
        .binary_search_by(|entry| compare(&Key::new(&entry.path, entry.kind), &query))
        .ok()?;
    (entries[at].kind == kind).then_some(at)
}
