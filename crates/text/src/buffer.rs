use std::cell::Cell;
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
    /// The last conversion's answer; the next walks only from here.
    utf16: Cell<(ByteOffset, Utf16Offset)>,
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
            utf16: Cell::new((ByteOffset(0), Utf16Offset(0))),
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

        self.splice(range.start, &removed, text);
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
        self.splice(edit.at, &edit.removed, &edit.inserted);
        edit.range_after()
    }

    /// The one text mutation, so the anchors and the UTF-16 checkpoint follow every edit.
    fn splice(&mut self, at: ByteOffset, removed: &str, inserted: &str) {
        let (byte, utf16) = self.utf16.get();
        let end = at.0 + removed.len();
        if byte > at {
            self.utf16.set(if byte.0 >= end {
                (
                    ByteOffset(byte.0 - removed.len() + inserted.len()),
                    Utf16Offset(utf16.0 - utf16_len(removed) + utf16_len(inserted)),
                )
            } else {
                (
                    at,
                    Utf16Offset(utf16.0 - utf16_len(&removed[..byte.0 - at.0])),
                )
            });
        }
        self.text.replace_range(at.0..end, inserted);
        self.shift_anchors(at, removed.len(), inserted.len());
    }

    #[must_use]
    pub fn to_utf16(&self, offset: ByteOffset) -> Utf16Offset {
        let answer = self.utf16_from(self.utf16.get(), offset);
        self.utf16.set((offset, answer));
        answer
    }

    #[must_use]
    pub fn from_utf16(&self, target: Utf16Offset) -> ByteOffset {
        let answer = self.byte_from(self.utf16.get(), target);
        self.utf16.set(answer);
        answer.0
    }

    /// The checkpoint lands on `start`; `end` walks from it so repeated calls do not swing the checkpoint.
    #[must_use]
    pub fn to_utf16_range(&self, range: &Range<ByteOffset>) -> Range<usize> {
        let start = self.to_utf16(range.start);
        start.0..self.utf16_from((range.start, start), range.end).0
    }

    /// Normalized so the result is never inverted: independent snapping can otherwise cross the ends.
    #[must_use]
    pub fn from_utf16_range(&self, range: &Range<usize>) -> Range<ByteOffset> {
        let from = self.byte_from(self.utf16.get(), Utf16Offset(range.start));
        self.utf16.set(from);
        let start = from.0;
        let end = self.byte_from(from, Utf16Offset(range.end)).0;
        start.min(end)..start.max(end)
    }

    fn utf16_from(
        &self,
        (byte, utf16): (ByteOffset, Utf16Offset),
        offset: ByteOffset,
    ) -> Utf16Offset {
        debug_assert!(
            self.text.is_char_boundary(offset.0),
            "offset {offset} must fall on a char boundary"
        );
        if offset >= byte {
            Utf16Offset(utf16.0 + utf16_len(&self.text[byte.0..offset.0]))
        } else {
            Utf16Offset(utf16.0 - utf16_len(&self.text[offset.0..byte.0]))
        }
    }

    fn byte_from(
        &self,
        (mut byte, mut utf16): (ByteOffset, Utf16Offset),
        target: Utf16Offset,
    ) -> (ByteOffset, Utf16Offset) {
        if target >= utf16 {
            for (index, ch) in self.text[byte.0..].char_indices() {
                if utf16 >= target {
                    return (ByteOffset(byte.0 + index), utf16);
                }
                utf16.0 += ch.len_utf16();
            }
            byte = self.len();
        } else {
            for (index, ch) in self.text[..byte.0].char_indices().rev() {
                let previous = utf16.0 - ch.len_utf16();
                if previous < target.0 {
                    break;
                }
                (byte, utf16) = (ByteOffset(index), Utf16Offset(previous));
            }
        }
        (byte, utf16)
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

    /// `clip`, then onto the grapheme boundary before (`Left`) or after (`Right`) it.
    #[must_use]
    pub fn clip_grapheme(&self, offset: ByteOffset, bias: Bias) -> ByteOffset {
        let offset = self.clip(offset).0;
        let len = self.text.len();
        let mut cursor = GraphemeCursor::new(offset, len, true);
        if cursor.is_boundary(&self.text, 0).unwrap_or(true) {
            return ByteOffset(offset);
        }
        let snapped = match bias {
            Bias::Left => cursor.prev_boundary(&self.text, 0).ok().flatten(),
            Bias::Right => cursor.next_boundary(&self.text, 0).ok().flatten(),
        };
        ByteOffset(snapped.unwrap_or(offset))
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

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests;
