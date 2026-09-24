use std::collections::HashMap;

use inkra_vault::{Entry, EntryId, Kind};
use unicode_normalization::UnicodeNormalization;

use crate::key::{Key, strip_md};
use crate::target::{Style, Target};

/// What a link resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Note(EntryId),
    /// A note link no note matches.
    Unresolved,
    External(String),
    SelfLink,
}

/// The vault's notes, keyed for resolution. Folders are never candidates.
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
}

struct Note {
    /// On disk, NFC, `/`-separated, `.md` stripped.
    path: String,
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
    /// `entries` in `Vault::entries()` order, which breaks the last tie.
    pub(crate) fn new<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Self {
        let mut table = Self::default();
        let notes = entries.into_iter().filter(|entry| entry.kind == Kind::Note);
        for (order, entry) in notes.enumerate() {
            let on_disk = entry.path.as_path().to_string_lossy();
            let path: String = strip_md(&on_disk).nfc().collect();
            let note = Note {
                key: Key::path(&path),
                stem: Key::stem(&path),
                path,
                order,
            };
            table
                .by_stem
                .entry(note.stem.clone())
                .or_default()
                .push(entry.id);
            table
                .by_path
                .entry(note.key.clone())
                .or_default()
                .push(entry.id);
            table.notes.insert(entry.id, note);
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

    pub(crate) fn resolve(&self, source: EntryId, target: &Target) -> Resolution {
        match self.query(source, target) {
            Ok(query) => self.search(&query),
            Err(resolution) => resolution,
        }
    }

    /// What `search` needs: it no longer depends on the source, so results can be shared.
    pub(crate) fn query(&self, source: EntryId, target: &Target) -> Result<Query, Resolution> {
        let (path, style) = match target {
            Target::Note { path, style, .. } => (path, *style),
            Target::External(url) => return Err(Resolution::External(url.clone())),
            Target::SelfLink => return Err(Resolution::SelfLink),
        };
        // `sub/`, `sub/.` and `sub/..` name a folder, and a folder is never a candidate.
        if matches!(path.rsplit('/').next(), Some("" | "." | "..")) {
            return Err(Resolution::Unresolved);
        }
        let written: String = path.nfc().collect();
        let written = match style {
            Style::Name | Style::Suffix => written,
            Style::Relative => self.join(source, &written).ok_or(Resolution::Unresolved)?,
        };
        Ok(Query { style, written })
    }

    pub(crate) fn search(&self, query: &Query) -> Resolution {
        let Query { style, written } = query;
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
            .map_or(Resolution::Unresolved, |(id, _)| Resolution::Note(id))
    }

    /// `path` from the source's folder, or from the root after a leading `/`; `None` above the
    /// root, or when the source is not a note of this vault.
    fn join(&self, source: EntryId, path: &str) -> Option<String> {
        let mut parts: Vec<&str> = Vec::new();
        if !path.starts_with('/') {
            let folder = self.notes.get(&source)?.folder();
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
