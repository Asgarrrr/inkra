use std::cell::Cell;
use std::ops::{Deref, Range};
use std::time::{Duration, Instant};

use unicode_segmentation::GraphemeCursor;

use crate::anchor::{Anchor, AnchorState, Bias, transform};
use crate::offset::{ByteOffset, Utf16Offset};

pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    /// Monotonic across the buffer's life; `Buffer::version` is the id of the last applied edit.
    /// Lives on the edit, not the `Transaction`: `close` merges by extending `previous.edits` in place.
    id: Version,
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
        self.at..self.at + self.inserted.len()
    }
}

#[derive(Debug)]
struct Transaction {
    edits: Vec<Edit>,
    last_edit_at: Option<Instant>,
}

/// An edit stamp: two equal versions mean the same text, and no version orders another.
///
/// ```compile_fail
/// let buffer = inkra_text::Buffer::new("ab");
/// let (before, after) = (buffer.version(), buffer.version());
/// let _ = before < after;
/// ```
///
/// ```
/// let buffer = inkra_text::Buffer::new("ab");
/// let (before, after) = (buffer.version(), buffer.version());
/// let _ = before == after;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Version(u64);

pub struct Buffer {
    text: String,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
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
            grouping_blocked: false,
            next_edit_id: 1,
            anchors: Vec::new(),
            utf16: Cell::new((ByteOffset::default(), Utf16Offset::default())),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// # Panics
    /// Panics if `range` is out of bounds or does not fall on char boundaries.
    #[must_use]
    pub fn slice(&self, range: Range<ByteOffset>) -> &str {
        &self.text[range.start.get()..range.end.get()]
    }

    #[must_use]
    pub fn len(&self) -> ByteOffset {
        ByteOffset::new(self.text.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    #[must_use]
    pub fn version(&self) -> Version {
        self.undo_stack
            .last()
            .and_then(|transaction| transaction.edits.last())
            .map_or(Version::default(), |edit| edit.id)
    }

    /// The only way to edit: the closure's `Txn` edits and reads, and can neither undo, redo,
    /// nor open another transaction.
    ///
    /// ```
    /// use inkra_text::ByteOffset;
    /// let mut buffer = inkra_text::Buffer::new("ab");
    /// let before = buffer.version();
    /// buffer.transact(|txn| txn.text().len());
    /// assert_eq!(buffer.version(), before);
    /// buffer.transact(|txn| {
    ///     txn.insert(ByteOffset::new(2), "c");
    ///     assert_eq!(txn.text(), "abc");
    /// });
    /// ```
    ///
    /// ```compile_fail
    /// use inkra_text::ByteOffset;
    /// let mut buffer = inkra_text::Buffer::new("ab");
    /// buffer.insert(ByteOffset::new(2), "c");
    /// ```
    ///
    /// ```compile_fail
    /// let mut buffer = inkra_text::Buffer::new("ab");
    /// buffer.transact(|txn| txn.undo());
    /// ```
    ///
    /// ```compile_fail
    /// let mut buffer = inkra_text::Buffer::new("ab");
    /// buffer.transact(|txn| txn.redo());
    /// ```
    ///
    /// ```compile_fail
    /// let mut buffer = inkra_text::Buffer::new("ab");
    /// buffer.transact(|txn| txn.transact(|_| {}));
    /// ```
    pub fn transact<R>(&mut self, f: impl FnOnce(&mut Txn<'_>) -> R) -> R {
        self.transact_inner(None, f)
    }

    pub fn transact_at<R>(&mut self, now: Instant, f: impl FnOnce(&mut Txn<'_>) -> R) -> R {
        self.transact_inner(Some(now), f)
    }

    pub fn finalize_transaction(&mut self) {
        self.grouping_blocked = true;
    }

    fn transact_inner<R>(&mut self, now: Option<Instant>, f: impl FnOnce(&mut Txn<'_>) -> R) -> R {
        let mut txn = Txn {
            buffer: self,
            edits: Vec::new(),
        };
        let result = f(&mut txn);
        let edits = txn.edits;
        self.close(now, edits);
        result
    }

    fn close(&mut self, now: Option<Instant>, edits: Vec<Edit>) {
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

    /// The one editing primitive: `None` when the edit changes nothing.
    fn edit(&mut self, range: Range<ByteOffset>, text: &str) -> Option<Edit> {
        debug_assert!(
            range.start <= range.end,
            "replace range start {} must not exceed end {}",
            range.start,
            range.end
        );
        debug_assert!(
            self.text.is_char_boundary(range.start.get())
                && self.text.is_char_boundary(range.end.get()),
            "replace range {}..{} must fall on char boundaries",
            range.start,
            range.end
        );

        let removed = self.slice(range.clone()).to_string();
        if removed.is_empty() && text.is_empty() {
            return None;
        }

        self.splice(range.start, &removed, text);
        let id = Version(self.next_edit_id);
        self.next_edit_id += 1;
        Some(Edit {
            id,
            at: range.start,
            removed,
            inserted: text.to_string(),
        })
    }

    pub fn undo(&mut self) -> Option<Range<ByteOffset>> {
        let transaction = self.undo_stack.pop()?;
        let mut affected = None;
        for edit in transaction.edits.iter().rev() {
            affected = Some(self.apply(&edit.inverted()));
        }
        self.redo_stack.push(transaction);
        affected
    }

    pub fn redo(&mut self) -> Option<Range<ByteOffset>> {
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
        let end = at + removed.len();
        if byte > at {
            self.utf16.set(if byte >= end {
                (
                    byte - removed.len() + inserted.len(),
                    utf16 - utf16_len(removed) + utf16_len(inserted),
                )
            } else {
                (at, utf16 - utf16_len(&removed[..byte - at]))
            });
        }
        self.text.replace_range(at.get()..end.get(), inserted);
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
    pub fn to_utf16_range(&self, range: &Range<ByteOffset>) -> Range<Utf16Offset> {
        let start = self.to_utf16(range.start);
        start..self.utf16_from((range.start, start), range.end)
    }

    /// Normalized so the result is never inverted: independent snapping can otherwise cross the ends.
    ///
    /// Takes UTF-16 positions, never a raw `usize` range:
    ///
    /// ```compile_fail
    /// use inkra_text::Utf16Offset;
    /// let buffer = inkra_text::Buffer::new("ab");
    /// let _ = buffer.from_utf16_range(&(0..1));
    /// ```
    ///
    /// ```
    /// use inkra_text::Utf16Offset;
    /// let buffer = inkra_text::Buffer::new("ab");
    /// let _ = buffer.from_utf16_range(&(Utf16Offset::new(0)..Utf16Offset::new(1)));
    /// ```
    #[must_use]
    pub fn from_utf16_range(&self, range: &Range<Utf16Offset>) -> Range<ByteOffset> {
        let from = self.byte_from(self.utf16.get(), range.start);
        self.utf16.set(from);
        let start = from.0;
        let end = self.byte_from(from, range.end).0;
        start.min(end)..start.max(end)
    }

    fn utf16_from(
        &self,
        (byte, utf16): (ByteOffset, Utf16Offset),
        offset: ByteOffset,
    ) -> Utf16Offset {
        debug_assert!(
            self.text.is_char_boundary(offset.get()),
            "offset {offset} must fall on a char boundary"
        );
        if offset >= byte {
            utf16 + utf16_len(self.slice(byte..offset))
        } else {
            utf16 - utf16_len(self.slice(offset..byte))
        }
    }

    fn byte_from(
        &self,
        (mut byte, mut utf16): (ByteOffset, Utf16Offset),
        target: Utf16Offset,
    ) -> (ByteOffset, Utf16Offset) {
        if target >= utf16 {
            for (index, ch) in self.text[byte.get()..].char_indices() {
                if utf16 >= target {
                    return (byte + index, utf16);
                }
                utf16 = utf16 + ch.len_utf16();
            }
            byte = self.len();
        } else {
            for (index, ch) in self.text[..byte.get()].char_indices().rev() {
                let previous = utf16 - ch.len_utf16();
                if previous < target {
                    break;
                }
                (byte, utf16) = (ByteOffset::new(index), previous);
            }
        }
        (byte, utf16)
    }

    /// The last grapheme start before `offset`, or 0. Cost is local to `offset`.
    #[must_use]
    pub fn previous_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        let offset = offset.get().min(self.text.len());
        let floor = (0..=offset)
            .rev()
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(0);
        let mut cursor = GraphemeCursor::new(floor, self.text.len(), true);
        if floor < offset && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset::new(floor);
        }
        ByteOffset::new(
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
        if offset.get() >= len {
            return self.len();
        }
        let ceil = (offset.get()..=len)
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(len);
        let mut cursor = GraphemeCursor::new(ceil, len, true);
        if ceil > offset.get() && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset::new(ceil);
        }
        ByteOffset::new(
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
        self.newlines_before(offset.get().min(self.text.len()))
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
        ByteOffset::new(start)..ByteOffset::new(end)
    }

    pub fn lines(&self) -> impl Iterator<Item = (ByteOffset, &str)> {
        let mut start = 0;
        self.text.split('\n').map(move |line| {
            let at = ByteOffset::new(start);
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
        let mut index = offset.get().min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        ByteOffset::new(index)
    }

    /// `clip`, then onto the grapheme boundary before (`Left`) or after (`Right`) it.
    #[must_use]
    pub fn clip_grapheme(&self, offset: ByteOffset, bias: Bias) -> ByteOffset {
        let offset = self.clip(offset).get();
        let len = self.text.len();
        let mut cursor = GraphemeCursor::new(offset, len, true);
        if cursor.is_boundary(&self.text, 0).unwrap_or(true) {
            return ByteOffset::new(offset);
        }
        let snapped = match bias {
            Bias::Left => cursor.prev_boundary(&self.text, 0).ok().flatten(),
            Bias::Right => cursor.next_boundary(&self.text, 0).ok().flatten(),
        };
        ByteOffset::new(snapped.unwrap_or(offset))
    }

    /// # Panics
    /// If `offset` is past the end or splits a char — a real `assert!`, since no slicing underneath would catch it in release.
    #[must_use]
    pub fn anchor(&mut self, offset: ByteOffset, bias: Bias) -> Anchor {
        assert!(
            offset.get() <= self.text.len(),
            "anchor offset {offset} past end of buffer"
        );
        assert!(
            self.text.is_char_boundary(offset.get()),
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

/// One open transaction's edits; derefs to the buffer for reads, and holds the only edit methods.
pub struct Txn<'a> {
    buffer: &'a mut Buffer,
    edits: Vec<Edit>,
}

impl Deref for Txn<'_> {
    type Target = Buffer;

    fn deref(&self) -> &Buffer {
        self.buffer
    }
}

impl Txn<'_> {
    /// # Panics
    /// If `range` is out of bounds or a bound splits a char; in debug builds, also if it is inverted.
    pub fn replace(&mut self, range: Range<ByteOffset>, text: &str) {
        self.edits.extend(self.buffer.edit(range, text));
    }

    /// # Panics
    /// See [`Txn::replace`].
    pub fn insert(&mut self, at: ByteOffset, text: &str) {
        self.replace(at..at, text);
    }

    /// # Panics
    /// See [`Txn::replace`].
    pub fn delete(&mut self, range: Range<ByteOffset>) {
        self.replace(range, "");
    }
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests;
