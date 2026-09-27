use std::cell::LazyCell;
use std::cmp::Reverse;
use std::time::SystemTime;

use inkra_vault::EntryId;

use super::Group;
use crate::index::IndexSnapshot;
use crate::names::NamesSnapshot;
use crate::words::{Lookup, NoteQuery};

/// Smaller is better; the unique `order` makes it total, so merging any split equals one range.
// The field order is S3's key order: the derived `Ord` compares fields in declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Rank {
    pub(super) group: Group,
    exact: Reverse<bool>,
    whole: Reverse<usize>,
    /// Stem terms for `Name`; else the occurrences of the terms placed in the text.
    weight: Reverse<u64>,
    showing: bool,
    recent: usize,
    modified: Reverse<SystemTime>,
    chars: usize,
    pub(super) order: usize,
}

pub(super) fn rank(
    snapshot: &IndexSnapshot,
    lookup: &Lookup,
    query: &NoteQuery,
    order: usize,
) -> Option<Rank> {
    let names = snapshot.names();
    let id = names.id(order);
    let (group, exact, whole, weight) = if query.is_empty() {
        if query.recent(id).is_none() || query.showing() == Some(id) {
            return None;
        }
        (Group::Name, false, 0, 0)
    } else {
        let (group, whole, weight) = place(names, lookup, query, order, id)?;
        // An exact stem holds every term, so only a `Name` row can be exact.
        let exact = group == Group::Name && is_exact(names, query, order);
        (group, exact, whole, weight)
    };
    let Some(modified) = snapshot.corpus().modified(order) else {
        panic!("names and corpus are of one build, so {order} is in both");
    };
    Some(Rank {
        group,
        exact: Reverse(exact),
        whole: Reverse(whole),
        weight: Reverse(weight),
        showing: query.showing() == Some(id),
        recent: query.recent(id).unwrap_or(usize::MAX),
        modified: Reverse(modified.0),
        chars: names.chars(order),
        order,
    })
}

/// Each term goes to the name if a name word starts with it, else the text; `None` if neither.
fn place(
    names: &NamesSnapshot,
    lookup: &Lookup,
    query: &NoteQuery,
    order: usize,
    id: EntryId,
) -> Option<(Group, usize, u64)> {
    let terms = query.terms();
    let ordinal = LazyCell::new(|| lookup.ordinal(id));
    let (mut in_name, mut whole, mut stem, mut occurrences) = (0, 0, 0, 0);
    for (at, term) in terms.iter().enumerate() {
        let (mut found, mut equal, mut in_stem) = (false, false, false);
        for word in names.words().of(order) {
            if word.folded.starts_with(&**term) {
                found = true;
                equal |= word.folded == &**term;
                in_stem |= word.in_stem;
            }
        }
        if found {
            in_name += 1;
            whole += usize::from(equal);
            stem += u64::from(in_stem);
        } else {
            let ordinal = (*ordinal)?;
            let hit = lookup.hit(at, ordinal)?;
            whole += usize::from(hit.whole);
            occurrences += u64::from(hit.count);
        }
    }
    Some(if in_name == terms.len() {
        (Group::Name, whole, stem)
    } else if in_name > 0 {
        (Group::NameAndText, whole, occurrences)
    } else {
        (Group::Text, whole, occurrences)
    })
}

/// Whether the stem's words, in path order, are the typed words: S3's exact name.
fn is_exact(names: &NamesSnapshot, query: &NoteQuery, order: usize) -> bool {
    names
        .words()
        .of(order)
        .filter(|word| word.in_stem)
        .map(|word| word.folded)
        .eq(query.typed())
}
