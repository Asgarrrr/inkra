use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::GraphemeCursor;

use crate::anchor::{Anchor, AnchorState, Bias, transform};
use crate::offset::{ByteOffset, Utf16Offset};

pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    /// Monotonic across the buffer's life; `Buffer::version` is the id of the last applied edit.
    /// Lives on the edit, not the `Transaction`: `close` merges by extending `previous.edits` in place.
    id: u64,
    at: ByteOffset,
    removed: String,
    inserted: String,
}

impl Edit {
    fn inverted(&self) -> Self {
        Self {
            id: self.id,
            at: self.at,
            removed: self.inserted.clone(),
            inserted: self.removed.clone(),
        }
    }

    fn range_after(&self) -> Range<ByteOffset> {
        self.at..ByteOffset(self.at.0 + self.inserted.len())
    }
}

#[derive(Debug)]
struct Transaction {
    edits: Vec<Edit>,
    last_edit_at: Option<Instant>,
}

pub struct Buffer {
    text: String,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    open: Option<Vec<Edit>>,
    depth: usize,
    grouping_blocked: bool,
    next_edit_id: u64,
    /// Every anchor ever created, `None` once forgotten; slots are never reused.
    anchors: Vec<Option<AnchorState>>,
}

impl Buffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            open: None,
            depth: 0,
            grouping_blocked: false,
            next_edit_id: 1,
            anchors: Vec::new(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn len(&self) -> ByteOffset {
        ByteOffset(self.text.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        self.undo_stack
            .last()
            .and_then(|transaction| transaction.edits.last())
            .map_or(0, |edit| edit.id)
    }

    pub fn transact<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.transact_inner(None, f)
    }

    pub fn transact_at<R>(&mut self, now: Instant, f: impl FnOnce(&mut Self) -> R) -> R {
        self.transact_inner(Some(now), f)
    }

    pub fn finalize_transaction(&mut self) {
        self.grouping_blocked = true;
    }

    fn transact_inner<R>(&mut self, now: Option<Instant>, f: impl FnOnce(&mut Self) -> R) -> R {
        if self.depth == 0 {
            self.open = Some(Vec::new());
        }
        self.depth += 1;

        let result = f(self);

        self.depth -= 1;
        if self.depth == 0 {
            self.close(now);
        }
        result
    }

    fn close(&mut self, now: Option<Instant>) {
        let Some(edits) = self.open.take() else {
            return;
        };
        if edits.is_empty() {
            return;
        }
        self.redo_stack.clear();

        if let Some(now) = now
            && !self.grouping_blocked
            && let Some(previous) = self.undo_stack.last_mut()
            && let Some(previous_at) = previous.last_edit_at
            && now.saturating_duration_since(previous_at) < GROUP_INTERVAL
        {
            previous.edits.extend(edits);
            previous.last_edit_at = Some(now);
            return;
        }

        self.grouping_blocked = false;
        self.undo_stack.push(Transaction {
            edits,
            last_edit_at: now,
        });
    }

    /// The one editing primitive: everything else is a `replace`.
    pub fn replace(&mut self, range: Range<ByteOffset>, text: &str) {
        debug_assert!(
            range.start <= range.end,
            "replace range start {} must not exceed end {}",
            range.start,
            range.end
        );
        debug_assert!(
            self.text.is_char_boundary(range.start.0) && self.text.is_char_boundary(range.end.0),
            "replace range {}..{} must fall on char boundaries",
            range.start,
            range.end
        );

        let removed = self.text[range.start.0..range.end.0].to_string();
        if removed.is_empty() && text.is_empty() {
            return;
        }

        self.text.replace_range(range.start.0..range.end.0, text);
        self.shift_anchors(range.start, removed.len(), text.len());
        let id = self.next_edit_id;
        self.next_edit_id += 1;
        self.record(Edit {
            id,
            at: range.start,
            removed,
            inserted: text.to_string(),
        });
    }

    pub fn insert(&mut self, at: ByteOffset, text: &str) {
        self.replace(at..at, text);
    }

    pub fn delete(&mut self, range: Range<ByteOffset>) {
        self.replace(range, "");
    }

    fn record(&mut self, edit: Edit) {
        if let Some(edits) = &mut self.open {
            edits.push(edit);
        } else {
            self.undo_stack.push(Transaction {
                edits: vec![edit],
                last_edit_at: None,
            });
            self.redo_stack.clear();
        }
    }

    /// # Panics
    /// If a transaction is open — its edits aren't on the undo stack yet.
    pub fn undo(&mut self) -> Option<Range<ByteOffset>> {
        assert!(self.depth == 0, "undo() inside an open transaction");
        let transaction = self.undo_stack.pop()?;
        let mut affected = None;
        for edit in transaction.edits.iter().rev() {
            affected = Some(self.apply(&edit.inverted()));
        }
        self.redo_stack.push(transaction);
        affected
    }

    /// # Panics
    /// If a transaction is open — see [`Buffer::undo`].
    pub fn redo(&mut self) -> Option<Range<ByteOffset>> {
        assert!(self.depth == 0, "redo() inside an open transaction");
        let transaction = self.redo_stack.pop()?;
        let mut affected = None;
        for edit in &transaction.edits {
            affected = Some(self.apply(edit));
        }
        self.undo_stack.push(transaction);
        affected
    }

    /// Assumes `edit` was captured from this exact text state; only the *last* call's range stays valid.
    fn apply(&mut self, edit: &Edit) -> Range<ByteOffset> {
        let end = edit.at.0 + edit.removed.len();
        self.text.replace_range(edit.at.0..end, &edit.inserted);
        self.shift_anchors(edit.at, edit.removed.len(), edit.inserted.len());
        edit.range_after()
    }

    #[must_use]
    pub fn to_utf16(&self, offset: ByteOffset) -> Utf16Offset {
        debug_assert!(
            self.text.is_char_boundary(offset.0),
            "offset {offset} must fall on a char boundary"
        );
        Utf16Offset(self.text[..offset.0].chars().map(char::len_utf16).sum())
    }

    #[must_use]
    pub fn from_utf16(&self, offset: Utf16Offset) -> ByteOffset {
        let mut utf16 = 0;
        for (byte, ch) in self.text.char_indices() {
            if utf16 >= offset.0 {
                return ByteOffset(byte);
            }
            utf16 += ch.len_utf16();
        }
        self.len()
    }

    #[must_use]
    pub fn to_utf16_range(&self, range: &Range<ByteOffset>) -> Range<usize> {
        self.to_utf16(range.start).0..self.to_utf16(range.end).0
    }

    /// Normalized so the result is never inverted: independent snapping can otherwise cross the ends.
    #[must_use]
    pub fn from_utf16_range(&self, range: &Range<usize>) -> Range<ByteOffset> {
        let start = self.from_utf16(Utf16Offset(range.start));
        let end = self.from_utf16(Utf16Offset(range.end));
        start.min(end)..start.max(end)
    }

    /// The last grapheme start before `offset`, or 0. Cost is local to `offset`.
    #[must_use]
    pub fn previous_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        let offset = offset.0.min(self.text.len());
        let floor = (0..=offset)
            .rev()
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(0);
        let mut cursor = GraphemeCursor::new(floor, self.text.len(), true);
        if floor < offset && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset(floor);
        }
        ByteOffset(
            cursor
                .prev_boundary(&self.text, 0)
                .ok()
                .flatten()
                .unwrap_or(0),
        )
    }

    /// The first grapheme start after `offset`, or the end. Cost is local to `offset`.
    #[must_use]
    pub fn next_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        let len = self.text.len();
        if offset.0 >= len {
            return self.len();
        }
        let ceil = (offset.0..=len)
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(len);
        let mut cursor = GraphemeCursor::new(ceil, len, true);
        if ceil > offset.0 && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset(ceil);
        }
        ByteOffset(
            cursor
                .next_boundary(&self.text, 0)
                .ok()
                .flatten()
                .unwrap_or(len),
        )
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.newlines_before(self.text.len()) + 1
    }

    /// Past the end, answers for the last line rather than panicking; layout lags the text by a frame.
    #[must_use]
    pub fn line_for_offset(&self, offset: ByteOffset) -> usize {
        self.newlines_before(offset.0.min(self.text.len()))
    }

    /// Excludes the terminating newline: `line_range(0).end` and `line_range(1).start` differ,
    /// which is what makes `End` then `Right` cross to the next line.
    ///
    /// # Panics
    /// If `line >= line_count()`.
    #[must_use]
    pub fn line_range(&self, line: usize) -> Range<ByteOffset> {
        let start = if line == 0 {
            0
        } else {
            let mut seen = 0;
            let found = self.text.bytes().position(|byte| {
                seen += usize::from(byte == b'\n');
                seen == line
            });
            assert!(
                found.is_some(),
                "line {line} is past the last line {}",
                self.line_count() - 1
            );
            found.map_or(0, |index| index + 1)
        };
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |index| start + index);
        ByteOffset(start)..ByteOffset(end)
    }

    pub fn lines(&self) -> impl Iterator<Item = (ByteOffset, &str)> {
        let mut start = 0;
        self.text.split('\n').map(move |line| {
            let at = ByteOffset(start);
            start += line.len() + 1;
            (at, line)
        })
    }

    /// Counts by byte, not char: a `\n` byte never appears inside a multi-byte sequence.
    // clippy wants the `bytecount` crate; a dependency to count one byte fails the dependency rule,
    // and the char-boundary-free scan is exactly the point, so the lint is allowed instead.
    #[allow(clippy::naive_bytecount)]
    fn newlines_before(&self, end: usize) -> usize {
        self.text.as_bytes()[..end]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    }

    #[must_use]
    pub fn clip(&self, offset: ByteOffset) -> ByteOffset {
        let mut index = offset.0.min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        ByteOffset(index)
    }

    /// # Panics
    /// If `offset` is past the end or splits a char — a real `assert!`, since no slicing underneath would catch it in release.
    #[must_use]
    pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor {
        assert!(
            offset.0 <= self.text.len(),
            "anchor offset {offset} past end of buffer"
        );
        assert!(
            self.text.is_char_boundary(offset.0),
            "anchor offset {offset} not a char boundary"
        );
        self.anchors.push(Some(AnchorState { offset, bias }));
        Anchor(self.anchors.len() - 1)
    }

    /// # Panics
    /// If `anchor` has been forgotten, or was created by another buffer.
    #[must_use]
    pub fn resolve(&self, anchor: Anchor) -> ByteOffset {
        self.anchors
            .get(anchor.0)
            .and_then(Option::as_ref)
            .unwrap_or_else(|| panic!("anchor {} is not live in this buffer", anchor.0))
            .offset
    }

    /// Drops an anchor; the slot is tombstoned, never reused, so a stale handle can't silently resolve to a new one.
    ///
    /// # Panics
    /// If `anchor` was already forgotten, or was created by another buffer.
    pub fn forget(&mut self, anchor: Anchor) {
        let state = self.anchors.get_mut(anchor.0).and_then(Option::take);
        assert!(
            state.is_some(),
            "anchor {} is not live in this buffer",
            anchor.0
        );
    }

    fn shift_anchors(&mut self, at: ByteOffset, removed_len: usize, inserted_len: usize) {
        for state in self.anchors.iter_mut().flatten() {
            state.offset = transform(state.offset, state.bias, at, removed_len, inserted_len);
        }
    }
}

#[cfg(test)]
mod tests;
