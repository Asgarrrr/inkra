//! The rows of a result, built once when it lands: a header per note, then a row per hit.

use std::ops::Range;

use gpui::SharedString;
use inkra_index::{Hit, NoteHits, Searched};
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, RelPath};

use crate::one_line::one_line;

pub(super) enum Row {
    /// Not selectable.
    Header(Header),
    Hit(HitRow),
}

pub(super) struct Header {
    pub(super) name: SharedString,
    pub(super) folder: SharedString,
    /// `{n}+` when the note gave more hits than listed.
    pub(super) count: SharedString,
    pub(super) label: SharedString,
}

/// What a refresh keeps selected: the hit, not its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HitKey {
    note: EntryId,
    start: ByteOffset,
}

pub(super) struct HitRow {
    pub(super) note: EntryId,
    pub(super) range: Range<ByteOffset>,
    pub(super) line: SharedString,
    pub(super) context: SharedString,
    /// The hit inside `context`.
    pub(super) lit: Range<usize>,
    pub(super) label: SharedString,
}

#[derive(Default)]
pub(super) struct Results {
    pub(super) rows: Vec<Row>,
    /// Of the largest line number listed: the width of the line column.
    pub(super) digits: u8,
    pub(super) limit_reached: bool,
}

impl Results {
    /// A note `path_of` does not know any more is dropped with its hits.
    pub(super) fn new<'a>(
        searched: Searched,
        path_of: impl Fn(EntryId) -> Option<&'a RelPath>,
    ) -> Self {
        let mut rows = Vec::new();
        let mut digits = 0;
        for NoteHits { note, hits, capped } in searched.notes {
            let Some(path) = path_of(note) else {
                continue;
            };
            rows.push(Row::Header(Header::new(path, hits.len(), capped)));
            for hit in hits {
                let row = HitRow::new(note, hit);
                digits = digits.max(u8::try_from(row.line.len()).unwrap_or(u8::MAX));
                rows.push(Row::Hit(row));
            }
        }
        Self {
            rows,
            digits,
            limit_reached: searched.limit_reached,
        }
    }

    pub(super) fn key(&self, ix: usize) -> Option<HitKey> {
        match self.rows.get(ix)? {
            Row::Hit(hit) => Some(hit.key()),
            Row::Header(_) => None,
        }
    }

    pub(super) fn position(&self, key: HitKey) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::Hit(hit) if hit.key() == key))
    }
}

impl Header {
    fn new(path: &RelPath, hits: usize, capped: bool) -> Self {
        let name: SharedString = one_line(&path.display_name()).into_owned().into();
        let folder = path
            .as_path()
            .parent()
            .map(|folder| one_line(&folder.to_string_lossy()).into_owned())
            .unwrap_or_default();
        let count: SharedString = if capped {
            format!("{hits}+").into()
        } else {
            hits.to_string().into()
        };
        let matches = if hits == 1 && !capped {
            "match"
        } else {
            "matches"
        };
        Self {
            label: format!("{name}, {count} {matches}").into(),
            name,
            folder: folder.into(),
            count,
        }
    }
}

impl HitRow {
    fn key(&self) -> HitKey {
        HitKey {
            note: self.note,
            start: self.range.start,
        }
    }

    fn new(note: EntryId, hit: Hit) -> Self {
        let Hit {
            range,
            line,
            context,
            context_match,
        } = hit;
        // An indent is not part of what the hit says; the match itself is never whitespace.
        let indent = (context.len() - context.trim_start().len()).min(context_match.start);
        let context: SharedString = one_line(&context[indent..]).into_owned().into();
        let line: SharedString = line.to_string().into();
        Self {
            note,
            range,
            label: format!("line {line}: {context}").into(),
            line,
            context,
            lit: context_match.start - indent..context_match.end - indent,
        }
    }
}
