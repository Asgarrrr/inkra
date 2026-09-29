//! The rows of a result, built once when it lands: a header per note, then a row per hit.

use std::ops::Range;

use gpui::SharedString;
use inkra_index::{Hit, NoteHits, Searched};
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, RelPath};

use crate::row_text::{LitText, PathText};

pub(super) enum Row {
    /// Not selectable.
    Header(Header),
    Hit(HitRow),
}

pub(super) struct Header {
    pub(super) path: PathText,
    /// `{n}+` when the note gave more hits than listed.
    pub(super) count: SharedString,
    pub(super) label: SharedString,
}

/// What a refresh keeps selected: the hit, not its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HitKey {
    note: EntryId,
    start: ByteOffset,
}

pub(super) struct HitRow {
    pub(super) note: EntryId,
    pub(super) range: Range<ByteOffset>,
    pub(super) line: SharedString,
    /// The hit lit inside it.
    pub(super) context: LitText,
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
}

impl Header {
    fn new(path: &RelPath, hits: usize, capped: bool) -> Self {
        // Notes are all `.md`, so this cuts the extension only.
        let path = PathText::new(&path.as_path().with_extension("").to_string_lossy(), &[]);
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
            label: format!("{}, {count} {matches}", path.name.text()).into(),
            path,
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
        let context = LitText::context(&context, [context_match]);
        let line: SharedString = line.to_string().into();
        Self {
            note,
            range,
            label: format!("line {line}: {}", context.text()).into(),
            line,
            context,
        }
    }
}
