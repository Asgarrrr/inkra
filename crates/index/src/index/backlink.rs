use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use inkra_vault::EntryId;

use super::{Backlink, NoteIndex};
use crate::notes::{Query, Resolution};
use crate::search::Candidate;
use crate::source::Source;
use crate::target::{NoteRef, Style};

impl NoteIndex {
    /// Grouped by source in vault order; a note linking to itself is included.
    ///
    /// # Panics
    /// If a source with a backlink holds no read: `insert` writes them together.
    #[must_use]
    pub fn backlinks(&self, file: EntryId) -> Vec<Backlink> {
        let Some(rows) = self
            .notes
            .stem(file)
            .or_else(|| self.assets.stem(file))
            .and_then(|stem| self.by_stem.get(stem))
        else {
            return Vec::new();
        };
        let mut grouped: BTreeMap<(usize, EntryId), Vec<u32>> = BTreeMap::new();
        // A shared name such as `index` is written the same way by thousands of links, and
        // only a relative link's resolution depends on its source.
        let mut by_text: HashMap<NoteRef<'_>, Resolution> = HashMap::new();
        let mut by_query: HashMap<Query, Resolution> = HashMap::new();
        for &(source, at) in rows {
            let note = self.source(source).links[at as usize].note();
            let resolution = match note {
                NoteRef { style, .. } if style != Style::Relative => by_text
                    .entry(note)
                    .or_insert_with(|| self.resolve_note(Some(source), note))
                    .clone(),
                _ => match self.notes.query(Some(source), note) {
                    Some(query) => by_query
                        .entry(query)
                        .or_insert_with_key(|query| self.search(query))
                        .clone(),
                    None => Resolution::Unresolved,
                },
            };
            if resolution == Resolution::File(file) {
                grouped
                    .entry((self.notes.order(source), source))
                    .or_default()
                    .push(at);
            }
        }
        grouped
            .into_iter()
            .map(|((order, source), mut at)| {
                at.sort_unstable();
                let Some(read) = self.corpus.read(order) else {
                    panic!("note {source:?} has a backlink but holds no read");
                };
                Backlink {
                    candidate: Candidate {
                        id: source,
                        path: Arc::clone(self.names.file(order)),
                        stamp: read.stamp,
                    },
                    links: Arc::clone(&self.source(source).links),
                    at,
                }
            })
            .collect()
    }

    fn source(&self, id: EntryId) -> &Source {
        let Some(source) = self.sources.get(id) else {
            panic!("note {id:?} has a backlink row but holds no source");
        };
        source
    }
}
