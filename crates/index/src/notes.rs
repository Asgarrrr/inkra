use std::collections::HashMap;
use std::sync::Arc;

use inkra_vault::EntryId;
use unicode_normalization::UnicodeNormalization;

use crate::key::Key;
use crate::target::{NoteRef, Style};

/// What a link resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A note, or an asset when no note matches.
    File(EntryId),
    /// A file link no file matches.
    Unresolved,
    External(String),
    SelfLink,
}

/// The vault's notes, or its assets, keyed for resolution. Folders are never candidates.
#[derive(Default)]
pub(crate) struct NoteTable {
    notes: HashMap<EntryId, Note>,
    by_stem: HashMap<Key, Vec<EntryId>>,
    by_path: HashMap<Key, Vec<EntryId>>,
}

/// A link's target, NFC, joined onto its source's folder when relative.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Query {
    style: Style,
    written: String,
    /// The link named a note by its `.md` file name, which no asset has.
    pub(crate) md: bool,
}

struct Note {
    /// On disk, NFC, `/`-separated; a note's `.md` stripped.
    path: Arc<str>,
    key: Key,
    stem: Key,
    order: usize,
}

impl Note {
    fn components(&self) -> usize {
        self.path.split('/').count()
    }

    fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }

    /// Whether `written`, NFC, is this path's tail on a component boundary, case included.
    fn ends_with_exactly(&self, written: &str) -> bool {
        self.path
            .strip_suffix(written)
            .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
    }
}

impl NoteTable {
    /// `notes` in `Vault::entries()` order, which breaks the last tie; paths as `prepare` gives.
    pub(crate) fn new(notes: &[(EntryId, Arc<str>)]) -> Self {
        let mut table = Self {
            notes: HashMap::with_capacity(notes.len()),
            by_stem: HashMap::with_capacity(notes.len()),
            by_path: HashMap::with_capacity(notes.len()),
        };
        for (order, (id, path)) in notes.iter().enumerate() {
            let note = Note {
                key: Key::path(path),
                stem: Key::stem(path),
                path: Arc::clone(path),
                order,
            };
            table
                .by_stem
                .entry(note.stem.clone())
                .or_default()
                .push(*id);
            table.by_path.entry(note.key.clone()).or_default().push(*id);
            table.notes.insert(*id, note);
        }
        table
    }

    pub(crate) fn contains(&self, id: EntryId) -> bool {
        self.notes.contains_key(&id)
    }

    pub(crate) fn stem(&self, id: EntryId) -> Option<&Key> {
        self.notes.get(&id).map(|note| &note.stem)
    }

    pub(crate) fn order(&self, id: EntryId) -> usize {
        self.notes.get(&id).map_or(usize::MAX, |note| note.order)
    }

    /// What `search` needs: it no longer depends on the source, so results can be shared.
    /// `None` for a folder, or a relative path no join reaches: the link is unresolved.
    pub(crate) fn query(&self, source: Option<EntryId>, note: NoteRef<'_>) -> Option<Query> {
        let NoteRef { path, md, style } = note;
        // `sub/`, `sub/.` and `sub/..` name a folder, and a folder is never a candidate.
        if matches!(path.rsplit('/').next(), Some("" | "." | "..")) {
            return None;
        }
        let written: String = path.nfc().collect();
        let written = match style {
            Style::Name | Style::Suffix => written,
            Style::Relative => self.join(source, &written)?,
        };
        Some(Query { style, written, md })
    }

    pub(crate) fn search(&self, query: &Query) -> Resolution {
        let Query { style, written, .. } = query;
        let style = *style;
        let candidates = match style {
            Style::Name | Style::Suffix => self.by_stem.get(&Key::stem(written)),
            Style::Relative => self.by_path.get(&Key::path(written)),
        };
        let key = Key::path(written);
        candidates
            .into_iter()
            .flatten()
            .filter_map(|id| self.notes.get(id).map(|note| (*id, note)))
            .filter(|(_, note)| style != Style::Suffix || is_suffix(&note.key, &key))
            .min_by_key(|(_, note)| {
                (
                    !note.ends_with_exactly(written),
                    note.components(),
                    note.order,
                )
            })
            .map_or(Resolution::Unresolved, |(id, _)| Resolution::File(id))
    }

    /// `path` from the source's folder, or from the root after a leading `/`; `None` above the
    /// root, or when the source is not a note of this vault.
    fn join(&self, source: Option<EntryId>, path: &str) -> Option<String> {
        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            let folder = source.and_then(|source| self.notes.get(&source))?.folder();
            parts.extend(folder.split('/').filter(|part| !part.is_empty()));
        }
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                part => parts.push(part),
            }
        }
        Some(parts.join("/"))
    }
}

fn is_suffix(key: &Key, suffix: &Key) -> bool {
    key.as_str()
        .strip_suffix(suffix.as_str())
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
}

#[cfg(test)]
mod tests;
