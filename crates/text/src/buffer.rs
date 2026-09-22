//! The text and its history: edits, transactions, undo, and the anchors that
//! survive them.
//!
//! No layout, no filesystem, no clock — the caller supplies the `Instant` that
//! decides whether two edits group. `docs/buffer-model.md` records the model
//! and the alternatives already rejected.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::anchor::{Anchor, AnchorState, Bias, transform};
use crate::offset::{ByteOffset, Utf16Offset};

/// Two transactions closer together than this merge into one undo step, so a
/// typing run undoes as a unit. Matches Zed's default.
pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

/// One replacement applied to the text: `removed` used to sit at `at`, and
/// `inserted` sits there now. Storing both sides keeps undo proportional to the
/// size of the edit rather than the size of the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    /// Monotonic across this buffer's whole life, assigned when the edit is
    /// applied. [`Buffer::version`] is the id of the last applied edit.
    ///
    /// On the edit, not on the [`Transaction`]: [`Buffer::close`] merges by
    /// extending `previous.edits` in place, so a transaction's identity does
    /// not move when the text does.
    id: u64,
    at: ByteOffset,
    removed: String,
    inserted: String,
}

impl Edit {
    /// Keeps the same id: this is one point in history, undone. Only
    /// [`Buffer::undo`] builds an inversion, and it drops it after applying —
    /// the version survives undo and redo because the original transaction
    /// moves between the stacks intact.
    fn inverted(&self) -> Self {
        Self {
            id: self.id,
            at: self.at,
            removed: self.inserted.clone(),
            inserted: self.removed.clone(),
        }
    }

    /// The range the inserted text occupies once this edit is applied — where a
    /// caller should put its cursor.
    fn range_after(&self) -> Range<ByteOffset> {
        self.at..ByteOffset(self.at.0 + self.inserted.len())
    }
}

/// A group of edits that undo and redo as one step.
#[derive(Debug)]
struct Transaction {
    edits: Vec<Edit>,
    /// When this transaction last recorded an edit. `None` means it must never
    /// absorb a later one — a bare edit, or an explicitly ungrouped `transact`.
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
    /// Every anchor ever created here, `None` once forgotten. Slots are never
    /// reused — see [`Buffer::forget`].
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

    /// Identifies the text's position in its own history.
    ///
    /// Two buffers are not comparable through this; it answers one question,
    /// for one buffer: has the text moved since a version I previously held.
    /// `app` stores the version it last wrote to disk and compares.
    ///
    /// Zed reaches the same property with a `clock::Global` version vector and
    /// a memoized `has_unsaved_edits`
    /// (`vendor/zed-reference/crates/language/src/buffer.rs:139`, `:2493`). It
    /// counts operations, which is the granularity adopted here. Ours is a
    /// single `u64` because we have one writer.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.undo_stack
            .last()
            .and_then(|transaction| transaction.edits.last())
            .map_or(0, |edit| edit.id)
    }

    /// Runs `f` with every edit it makes grouped into a single undo step that
    /// stands alone. Nested calls join the outermost group.
    ///
    /// A panic inside `f` leaves the group open; that is a caller bug, and the
    /// process is going down anyway.
    pub fn transact<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        self.transact_inner(None, f)
    }

    /// Like [`Buffer::transact`], but merges into the previous transaction when
    /// the two happen within [`GROUP_INTERVAL`] — so a typing run undoes as one
    /// step rather than one step per character.
    ///
    /// The caller supplies `now` rather than the buffer reading a clock: time is
    /// the application's concern, and injecting it keeps this deterministic
    /// under test instead of needing a different code path there.
    pub fn transact_at<R>(&mut self, now: Instant, f: impl FnOnce(&mut Self) -> R) -> R {
        self.transact_inner(Some(now), f)
    }

    /// Seals the most recent transaction so the next [`Buffer::transact_at`]
    /// starts a fresh undo step. Call it around anything that should not be
    /// swallowed by adjacent typing — a paste, a formatting pass.
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

    /// Reverts the last transaction, returning the region the restored text now
    /// occupies so the caller can place its cursor.
    ///
    /// # Panics
    /// If a transaction is open. The open edits are not yet on the undo stack,
    /// so the transaction below them no longer describes the newest change to
    /// the text, and applying its offsets would corrupt the buffer.
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

    /// Relies on `edit` having been captured from this exact text state: the
    /// range it names is valid by construction, not by check.
    ///
    /// The range returned by the *last* call in an undo or redo pass is the
    /// only one still valid in the resulting text — every earlier one may have
    /// been shifted by the edits applied after it. That is why `undo` and
    /// `redo` keep the last value rather than combining them, and why the two
    /// directions legitimately report different positions: they produce
    /// different texts.
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

    /// Converts back from UTF-16. An offset landing inside a surrogate pair
    /// snaps forward to the next char boundary, matching what platform input
    /// methods expect when they probe a position.
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

    /// A whole range, in the units the platform input protocol speaks.
    #[must_use]
    pub fn to_utf16_range(&self, range: &Range<ByteOffset>) -> Range<usize> {
        self.to_utf16(range.start).0..self.to_utf16(range.end).0
    }

    /// The inverse, normalized so the result is never inverted.
    ///
    /// An input method may hand back a range whose ends arrive in either order,
    /// and `from_utf16` snaps each into a char boundary independently — so two
    /// ends one surrogate apart can cross. Every caller wants a range it can
    /// slice with.
    #[must_use]
    pub fn from_utf16_range(&self, range: &Range<usize>) -> Range<ByteOffset> {
        let start = self.from_utf16(Utf16Offset(range.start));
        let end = self.from_utf16(Utf16Offset(range.end));
        start.min(end)..start.max(end)
    }

    /// Start of the grapheme cluster before `offset` — one cursor step left.
    /// Graphemes, not chars: "é" as e+combining-accent is one step, and so is a
    /// multi-codepoint emoji.
    #[must_use]
    pub fn previous_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset.0).then_some(ByteOffset(index)))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn next_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        self.text
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset.0).then_some(ByteOffset(index)))
            .unwrap_or_else(|| self.len())
    }

    /// Number of logical lines. A newline separates two lines rather than
    /// belonging to either, so an empty buffer is one line and `"a\n"` is two —
    /// the second empty.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.newlines_before(self.text.len()) + 1
    }

    /// The line containing `offset`, counting from zero.
    ///
    /// An offset past the end answers for the last line rather than panicking:
    /// callers reach this from layout, which lags the text by a frame.
    #[must_use]
    pub fn line_for_offset(&self, offset: ByteOffset) -> usize {
        self.newlines_before(offset.0.min(self.text.len()))
    }

    /// The line's text, *excluding* its terminating newline.
    ///
    /// So `line_range(0).end` and `line_range(1).start` are different offsets in
    /// `"a\nb"`, and both are places a cursor may sit. That is what makes `End`
    /// then `Right` cross to the next line instead of standing still.
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

    /// Every line with the offset it starts at, in one pass.
    ///
    /// The element calls this once per frame. Going through
    /// [`Buffer::line_range`] per line instead would rescan from the top each
    /// time, turning an O(n) frame into O(n · lines).
    pub fn lines(&self) -> impl Iterator<Item = (ByteOffset, &str)> {
        let mut start = 0;
        self.text.split('\n').map(move |line| {
            let at = ByteOffset(start);
            start += line.len() + 1;
            (at, line)
        })
    }

    /// Counts by byte, not by char: a `\n` byte can never appear inside a
    /// multi-byte sequence, so this is correct at any index and needs no char
    /// boundary from the caller.
    // clippy suggests the `bytecount` crate. A whole dependency to count one
    // byte value fails the project's dependency rule, and the alternative that
    // avoids the lint — slicing to `&str` and counting `matches` — would need a
    // char boundary from the caller, which is exactly what this is built not to
    // need. Revisit if a profile ever shows this scan mattering.
    #[allow(clippy::naive_bytecount)]
    fn newlines_before(&self, end: usize) -> usize {
        self.text.as_bytes()[..end]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    }

    /// Snaps an offset into range and back onto a char boundary.
    #[must_use]
    pub fn clip(&self, offset: ByteOffset) -> ByteOffset {
        let mut index = offset.0.min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        ByteOffset(index)
    }

    /// A handle to `offset` that this buffer moves with the text. `bias`
    /// decides which side of an edit landing on exactly this offset it ends up
    /// on.
    ///
    /// The handle means nothing to another buffer: resolving it there answers
    /// for whatever sits in the same slot, or panics.
    ///
    /// # Panics
    /// If `offset` is past the end of the text or splits a char. A real
    /// `assert!`, unlike [`Buffer::replace`]'s: this path only stores the
    /// offset, so there is no stdlib slicing underneath to catch a bad one in
    /// release, and the damage would surface later inside `resolve`.
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

    /// Where `anchor` sits in the text as it stands now.
    ///
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

    /// Drops an anchor. Its slot is tombstoned, never reused: handing it to a
    /// later anchor would turn the panic below into a silent answer about a
    /// different position.
    ///
    /// # Panics
    /// If `anchor` has already been forgotten, or was created by another
    /// buffer.
    pub fn forget(&mut self, anchor: Anchor) {
        let state = self.anchors.get_mut(anchor.0).and_then(Option::take);
        assert!(
            state.is_some(),
            "anchor {} is not live in this buffer",
            anchor.0
        );
    }

    /// Walks every slot, tombstones included, so this is O(anchors ever
    /// created) per edit rather than O(live anchors). Same accepted cost shape
    /// as the grapheme scans above, and with no consumer yet to load it.
    fn shift_anchors(&mut self, at: ByteOffset, removed_len: usize, inserted_len: usize) {
        for state in self.anchors.iter_mut().flatten() {
            state.offset = transform(state.offset, state.bias, at, removed_len, inserted_len);
        }
    }
}

#[cfg(test)]
mod tests;
