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
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn at(offset: usize) -> ByteOffset {
        ByteOffset(offset)
    }

    #[test]
    fn new_buffer_holds_initial_text() {
        assert_eq!(Buffer::new("hello").text(), "hello");
    }

    #[test]
    fn insert_places_text_at_byte_offset() {
        // "héllo": h@0 é@1..3 l@3 l@4 o@5 — 'é' is 2 bytes.
        let mut buf = Buffer::new("héllo");
        buf.insert(at(3), "X");
        assert_eq!(buf.text(), "héXllo");
    }

    #[test]
    fn delete_removes_a_byte_range() {
        let mut buf = Buffer::new("héllo");
        buf.delete(at(1)..at(4));
        assert_eq!(buf.text(), "hlo");
    }

    #[test]
    #[should_panic(expected = "char boundaries")]
    fn replace_panics_when_a_bound_splits_a_char() {
        let mut buf = Buffer::new("héllo");
        buf.insert(at(2), "X");
    }

    #[test]
    fn no_op_replace_records_nothing() {
        let mut buf = Buffer::new("abc");
        buf.replace(at(1)..at(1), "");
        assert!(buf.undo().is_none());
    }

    #[test]
    fn undo_reverts_the_last_edit_and_reports_the_restored_range() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "d");
        assert_eq!(buf.text(), "abcd");
        assert_eq!(buf.undo(), Some(at(3)..at(3)));
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn undo_with_empty_history_returns_none() {
        assert!(Buffer::new("abc").undo().is_none());
    }

    #[test]
    fn redo_reapplies_the_undone_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "d");
        buf.undo();
        assert_eq!(buf.redo(), Some(at(3)..at(4)));
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn new_edit_after_undo_clears_the_redo_stack() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "d");
        buf.undo();
        buf.insert(at(0), "X");
        assert!(buf.redo().is_none());
    }

    #[test]
    fn a_transaction_undoes_as_one_step() {
        let mut buf = Buffer::new("abc");
        buf.transact(|buf| {
            buf.delete(at(0)..at(1));
            buf.insert(at(0), "XY");
        });
        assert_eq!(buf.text(), "XYbc");
        buf.undo();
        assert_eq!(buf.text(), "abc");
        assert!(buf.undo().is_none());
    }

    #[test]
    fn nested_transactions_join_the_outermost_step() {
        let mut buf = Buffer::new("");
        buf.transact(|buf| {
            buf.insert(at(0), "a");
            buf.transact(|buf| buf.insert(at(1), "b"));
        });
        assert_eq!(buf.text(), "ab");
        buf.undo();
        assert_eq!(buf.text(), "");
        assert!(buf.undo().is_none());
    }

    #[test]
    fn an_empty_transaction_records_nothing() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "d");
        buf.transact(|_| {});
        buf.undo();
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn a_transaction_does_not_clear_redo_until_it_records_an_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "d");
        buf.undo();
        buf.transact(|_| {});
        assert_eq!(buf.redo(), Some(at(3)..at(4)));
    }

    #[test]
    #[should_panic(expected = "undo() inside an open transaction")]
    fn undo_inside_an_open_transaction_is_refused() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "1");
        buf.transact(|buf| {
            buf.insert(at(0), "X");
            // Without the guard this pops the older transaction and applies its
            // now-stale offsets, silently deleting 'c' and leaving "Xab1".
            buf.undo();
        });
    }

    #[test]
    #[should_panic(expected = "redo() inside an open transaction")]
    fn redo_inside_an_open_transaction_is_refused() {
        let mut buf = Buffer::new("abc");
        buf.insert(at(3), "1");
        buf.undo();
        buf.transact(|buf| {
            buf.insert(at(0), "X");
            buf.redo();
        });
    }

    #[test]
    fn a_multi_edit_transaction_reports_a_region_valid_in_the_resulting_text() {
        let mut buf = Buffer::new("abcd");
        buf.transact(|buf| {
            buf.insert(at(0), "<");
            buf.insert(at(5), ">");
        });
        assert_eq!(buf.text(), "<abcd>");

        // The two directions produce different texts, so they report different
        // positions — but each must be addressable in the text it produced.
        let undone = buf.undo().unwrap();
        assert_eq!(buf.text(), "abcd");
        assert!(
            undone.end <= buf.len(),
            "{undone:?} escapes {:?}",
            buf.text()
        );

        let redone = buf.redo().unwrap();
        assert_eq!(buf.text(), "<abcd>");
        assert!(
            redone.end <= buf.len(),
            "{redone:?} escapes {:?}",
            buf.text()
        );
    }

    #[test]
    fn edits_within_the_group_interval_merge_into_one_undo_step() {
        let mut buf = Buffer::new("");
        let start = Instant::now();
        for (index, ch) in "hello".chars().enumerate() {
            let now = start + GROUP_INTERVAL / 2 * u32::try_from(index).unwrap();
            let at = buf.len();
            buf.transact_at(now, |buf| buf.insert(at, &ch.to_string()));
        }
        assert_eq!(buf.text(), "hello");

        buf.undo();
        assert_eq!(buf.text(), "");
        assert!(buf.undo().is_none());
    }

    #[test]
    fn edits_further_apart_than_the_group_interval_stay_separate() {
        let mut buf = Buffer::new("");
        let start = Instant::now();
        buf.transact_at(start, |buf| buf.insert(at(0), "a"));
        buf.transact_at(start + GROUP_INTERVAL * 2, |buf| buf.insert(at(1), "b"));

        buf.undo();
        assert_eq!(buf.text(), "a");
        buf.undo();
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn a_merged_run_redoes_as_one_step() {
        let mut buf = Buffer::new("");
        let start = Instant::now();
        buf.transact_at(start, |buf| buf.insert(at(0), "a"));
        buf.transact_at(start, |buf| buf.insert(at(1), "b"));

        buf.undo();
        assert_eq!(buf.text(), "");
        buf.redo();
        assert_eq!(buf.text(), "ab");
        assert!(buf.redo().is_none());
    }

    #[test]
    fn finalize_transaction_stops_the_next_edit_from_merging() {
        let mut buf = Buffer::new("");
        let start = Instant::now();
        buf.transact_at(start, |buf| buf.insert(at(0), "a"));
        buf.finalize_transaction();
        buf.transact_at(start, |buf| buf.insert(at(1), "b"));

        buf.undo();
        assert_eq!(buf.text(), "a");
    }

    #[test]
    fn an_ungrouped_transaction_never_absorbs_a_later_one() {
        let mut buf = Buffer::new("");
        buf.transact(|buf| buf.insert(at(0), "a"));
        buf.transact_at(Instant::now(), |buf| buf.insert(at(1), "b"));

        buf.undo();
        assert_eq!(buf.text(), "a");
    }

    #[test]
    fn undo_keeps_only_the_edited_text_not_a_buffer_snapshot() {
        let mut buf = Buffer::new("x".repeat(10_000));
        buf.insert(at(0), "a");
        let recorded: usize = buf
            .undo_stack
            .iter()
            .flat_map(|transaction| &transaction.edits)
            .map(|edit| edit.removed.len() + edit.inserted.len())
            .sum();
        assert_eq!(recorded, 1);
    }

    #[test]
    fn version_starts_at_zero_and_moves_with_an_edit() {
        let mut buffer = Buffer::new("abc");
        assert_eq!(buffer.version(), 0);
        buffer.insert(ByteOffset(3), "x");
        assert_ne!(buffer.version(), 0);
    }

    /// The defect that motivated per-edit ids.
    ///
    /// `close` merges an edit that lands within `GROUP_INTERVAL` by extending
    /// the previous transaction in place, so a transaction's identity does not
    /// move even though the text changed. With the id on the transaction, this
    /// reports "saved" over unsaved text and the edit is lost on quit.
    #[test]
    fn version_moves_when_an_edit_merges_into_the_previous_transaction() {
        let mut buffer = Buffer::new("abc");
        let t0 = Instant::now();
        buffer.transact_at(t0, |b| b.insert(ByteOffset(3), "x"));
        let saved = buffer.version();

        buffer.transact_at(t0 + Duration::from_millis(120), |b| {
            b.insert(ByteOffset(4), "y");
        });

        assert_eq!(buffer.text(), "abcxy");
        assert_eq!(buffer.undo_stack.len(), 1, "the edits must have merged");
        assert_ne!(
            buffer.version(),
            saved,
            "a merged edit still changes the text, so the version must move"
        );
    }

    #[test]
    fn undo_returns_the_version_to_its_earlier_value() {
        let mut buffer = Buffer::new("abc");
        buffer.transact(|b| b.insert(ByteOffset(3), "x"));
        let after_first = buffer.version();
        buffer.finalize_transaction();
        buffer.transact(|b| b.insert(ByteOffset(4), "y"));

        buffer.undo();

        assert_eq!(buffer.text(), "abcx");
        assert_eq!(buffer.version(), after_first);
    }

    #[test]
    fn redo_restores_the_same_version() {
        let mut buffer = Buffer::new("abc");
        buffer.transact(|b| b.insert(ByteOffset(3), "x"));
        let after_edit = buffer.version();
        buffer.undo();
        buffer.redo();
        assert_eq!(buffer.version(), after_edit);
    }

    #[test]
    fn a_divergent_edit_after_undo_cannot_collide_with_the_history_it_replaced() {
        let mut buffer = Buffer::new("abc");
        buffer.transact(|b| b.insert(ByteOffset(3), "x"));
        let replaced = buffer.version();
        buffer.undo();
        buffer.transact(|b| b.insert(ByteOffset(3), "z"));

        assert_eq!(buffer.text(), "abcz");
        assert_ne!(buffer.version(), replaced);
    }

    #[test]
    fn utf16_offsets_round_trip_across_a_surrogate_pair() {
        // "a😀b": 'a'@0, '😀'@1..5 (2 UTF-16 units), 'b'@5.
        let buf = Buffer::new("a😀b");
        assert_eq!(buf.to_utf16(at(5)), Utf16Offset(3));
        assert_eq!(buf.from_utf16(Utf16Offset(3)), at(5));
        assert_eq!(buf.to_utf16(buf.len()), Utf16Offset(4));
    }

    #[test]
    fn a_utf16_offset_inside_a_surrogate_pair_snaps_forward() {
        let buf = Buffer::new("a😀b");
        assert_eq!(buf.from_utf16(Utf16Offset(2)), at(5));
    }

    #[test]
    fn utf16_offset_past_the_end_clamps_to_the_end() {
        let buf = Buffer::new("ab");
        assert_eq!(buf.from_utf16(Utf16Offset(99)), at(2));
    }

    #[test]
    fn grapheme_steps_cross_a_combining_mark_as_one_unit() {
        // "e" + U+0301 combining acute: 3 bytes, 2 chars, 1 grapheme.
        let buf = Buffer::new("e\u{301}x");
        assert_eq!(buf.next_grapheme(at(0)), at(3));
        assert_eq!(buf.previous_grapheme(at(3)), at(0));
    }

    #[test]
    fn grapheme_steps_cross_a_zwj_emoji_as_one_unit() {
        let buf = Buffer::new("👨‍👩‍👧");
        assert_eq!(buf.next_grapheme(at(0)), buf.len());
        assert_eq!(buf.previous_grapheme(buf.len()), at(0));
    }

    #[test]
    fn grapheme_steps_saturate_at_the_ends() {
        let buf = Buffer::new("ab");
        assert_eq!(buf.previous_grapheme(at(0)), at(0));
        assert_eq!(buf.next_grapheme(at(2)), at(2));
    }

    #[test]
    fn an_empty_buffer_is_one_empty_line() {
        let buf = Buffer::new("");
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line_range(0), at(0)..at(0));
        assert_eq!(buf.lines().collect::<Vec<_>>(), vec![(at(0), "")]);
    }

    #[test]
    fn a_trailing_newline_opens_an_empty_last_line() {
        let buf = Buffer::new("a\n");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_range(1), at(2)..at(2));
    }

    #[test]
    fn consecutive_newlines_each_open_a_line() {
        let buf = Buffer::new("\n\n");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line_range(0), at(0)..at(0));
        assert_eq!(buf.line_range(1), at(1)..at(1));
        assert_eq!(buf.line_range(2), at(2)..at(2));
    }

    #[test]
    fn a_line_range_excludes_its_terminating_newline() {
        let buf = Buffer::new("ab\ncd");
        assert_eq!(buf.line_range(0), at(0)..at(2));
        assert_eq!(buf.line_range(1), at(3)..at(5));
    }

    #[test]
    fn a_newline_is_a_separator_so_its_two_sides_are_different_offsets() {
        // End of line 0 and start of line 1 are 2 and 3, not both 2. Right from
        // the end of a line must land on the start of the next one.
        let buf = Buffer::new("ab\ncd");
        assert_eq!(buf.line_range(0).end, at(2));
        assert_eq!(buf.line_range(1).start, at(3));
        assert_eq!(buf.next_grapheme(at(2)), at(3));
    }

    #[test]
    fn line_for_offset_answers_at_both_edges_of_a_line() {
        let buf = Buffer::new("ab\ncd");
        assert_eq!(buf.line_for_offset(at(0)), 0);
        assert_eq!(buf.line_for_offset(at(2)), 0);
        // Offset 3 is the newline's far side: the start of line 1.
        assert_eq!(buf.line_for_offset(at(3)), 1);
        assert_eq!(buf.line_for_offset(at(5)), 1);
    }

    #[test]
    fn line_for_offset_past_the_end_answers_for_the_last_line() {
        let buf = Buffer::new("ab\ncd");
        assert_eq!(buf.line_for_offset(at(999)), 1);
    }

    #[test]
    fn a_multi_byte_char_does_not_split_a_line() {
        // "é" is 2 bytes and holds no 0x0a, so counting newlines by byte is safe.
        let buf = Buffer::new("é\né");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_range(0), at(0)..at(2));
        assert_eq!(buf.line_range(1), at(3)..at(5));
    }

    #[test]
    #[should_panic(expected = "past the last line")]
    fn a_line_past_the_end_is_refused() {
        let _ = Buffer::new("a\nb").line_range(2);
    }

    #[test]
    fn lines_agrees_with_line_range() {
        let buf = Buffer::new("ab\n\ncd\n");
        for (line, (start, text)) in buf.lines().enumerate() {
            let range = buf.line_range(line);
            assert_eq!(start, range.start, "line {line} start");
            assert_eq!(text, &buf.text()[range.start.0..range.end.0], "line {line}");
        }
        assert_eq!(buf.lines().count(), buf.line_count());
    }

    #[test]
    fn the_line_index_follows_an_edit() {
        let mut buf = Buffer::new("ab");
        assert_eq!(buf.line_count(), 1);
        buf.insert(at(1), "\n");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line_range(1), at(2)..at(3));
        buf.undo();
        assert_eq!(buf.line_count(), 1);
    }

    #[test]
    fn clip_snaps_back_onto_a_char_boundary_and_into_range() {
        let buf = Buffer::new("héllo");
        assert_eq!(buf.clip(at(2)), at(1));
        assert_eq!(buf.clip(at(999)), buf.len());
    }

    /// The anchor table, exercised through the public API. The pure
    /// `transform` cases live next to the function, in `anchor.rs`.
    mod anchors {
        use proptest::prelude::*;

        use super::*;

        #[test]
        fn an_edit_before_an_anchor_shifts_it_by_the_length_change() {
            let mut buf = Buffer::new("abcde");
            let anchor = buf.anchor(at(4), Bias::Left);

            buf.insert(at(0), "XY");
            assert_eq!(buf.resolve(anchor), at(6));
            buf.delete(at(0)..at(3));
            assert_eq!(buf.resolve(anchor), at(3));
            // Still sitting on 'e', which is what the offsets have to mean.
            assert_eq!(&buf.text()[buf.resolve(anchor).0..], "e");
        }

        #[test]
        fn typing_at_an_anchor_carries_only_the_right_biased_one() {
            let mut buf = Buffer::new("ab");
            let left = buf.anchor(at(1), Bias::Left);
            let right = buf.anchor(at(1), Bias::Right);

            buf.insert(at(1), "XY");
            assert_eq!(buf.resolve(left), at(1));
            assert_eq!(buf.resolve(right), at(3));
        }

        #[test]
        fn an_anchor_between_two_edits_of_one_transaction_survives_undo() {
            let mut buf = Buffer::new("abcd");
            let anchor = buf.anchor(at(2), Bias::Left);

            buf.transact(|buf| {
                buf.insert(at(0), "<");
                buf.insert(at(5), ">");
            });
            assert_eq!(buf.text(), "<abcd>");
            assert_eq!(buf.resolve(anchor), at(3));

            // Undo replays the inverted edits in reverse, so the anchor has to
            // come back through both of them.
            buf.undo();
            assert_eq!(buf.text(), "abcd");
            assert_eq!(buf.resolve(anchor), at(2));

            buf.redo();
            assert_eq!(buf.resolve(anchor), at(3));
        }

        #[test]
        fn undoing_an_edit_before_an_anchor_puts_it_back() {
            let mut buf = Buffer::new("abcde");
            let anchor = buf.anchor(at(4), Bias::Left);

            buf.insert(at(1), "XYZ");
            assert_eq!(buf.resolve(anchor), at(7));
            buf.undo();
            assert_eq!(buf.resolve(anchor), at(4));
        }

        #[test]
        fn an_anchor_stays_addressable_across_undo_and_redo() {
            let mut buf = Buffer::new("héllo");
            let anchor = buf.anchor(at(3), Bias::Right);

            let addressable = |buf: &Buffer| {
                let offset = buf.resolve(anchor);
                assert!(offset <= buf.len(), "{offset} escapes {:?}", buf.text());
                assert!(
                    buf.text().is_char_boundary(offset.0),
                    "{offset} splits a char in {:?}",
                    buf.text()
                );
            };

            buf.transact(|buf| buf.replace(at(1)..at(3), "🎉"));
            addressable(&buf);
            let end = buf.len();
            buf.insert(end, "!");
            addressable(&buf);
            buf.undo();
            addressable(&buf);
            buf.undo();
            addressable(&buf);
            buf.redo();
            addressable(&buf);
            buf.redo();
            addressable(&buf);
        }

        /// Two facts about deletion that look alike and are not, kept side by
        /// side so a later "fix" to one is not quietly applied to the other.
        mod a_deleted_anchor {
            use super::*;

            /// Strictly inside the removed span: it collapses onto a boundary
            /// and does not come back inside the restored text. Accepted, with
            /// the reasoning in `docs/buffer-model.md`.
            #[test]
            fn does_not_return_inside_the_text_on_undo() {
                let mut buf = Buffer::new("abcde");
                let left = buf.anchor(at(2), Bias::Left);
                let right = buf.anchor(at(2), Bias::Right);

                buf.replace(at(1)..at(4), "XY");
                assert_eq!(buf.text(), "aXYe");
                assert_eq!(buf.resolve(left), at(1));
                assert_eq!(buf.resolve(right), at(3));

                buf.undo();
                assert_eq!(buf.text(), "abcde");
                // Boundary to boundary — not back onto 'c' at offset 2.
                assert_eq!(buf.resolve(left), at(1));
                assert_eq!(buf.resolve(right), at(4));
            }

            /// At the exclusive end of a *pure* deletion, `Right` does
            /// round-trip: nothing was inserted, so there is no inserted text
            /// for it to land after.
            #[test]
            fn tracks_the_end_of_a_pure_deletion_exactly() {
                let mut buf = Buffer::new("abc");
                let anchor = buf.anchor(at(1), Bias::Right);

                buf.delete(at(0)..at(1));
                assert_eq!(buf.resolve(anchor), at(0));
                buf.undo();
                assert_eq!(buf.text(), "abc");
                assert_eq!(buf.resolve(anchor), at(1));
            }
        }

        #[test]
        #[should_panic(expected = "not live in this buffer")]
        fn resolving_a_forgotten_anchor_is_refused() {
            let mut buf = Buffer::new("abc");
            let anchor = buf.anchor(at(1), Bias::Left);
            buf.forget(anchor);
            let _ = buf.resolve(anchor);
        }

        #[test]
        #[should_panic(expected = "not live in this buffer")]
        fn forgetting_an_anchor_twice_is_refused() {
            let mut buf = Buffer::new("abc");
            let anchor = buf.anchor(at(1), Bias::Left);
            buf.forget(anchor);
            buf.forget(anchor);
        }

        /// One anchor plus what the properties judge it by: the offset
        /// `transform` predicts without asking `Buffer`, where it was created,
        /// and whether any edit so far cost it its claim to survive a full
        /// undo.
        struct Tracked {
            anchor: Anchor,
            bias: Bias,
            created_at: ByteOffset,
            predicted: ByteOffset,
            live: bool,
            restorable: bool,
        }

        #[derive(Debug, Clone)]
        enum Op {
            Edit(usize, usize, String),
            /// A replace and an insert in one `transact` — the shape that makes
            /// `undo` replay several inverted edits in reverse, which a
            /// single-edit transaction never reaches.
            Transaction(usize, usize, String, usize, String),
            Anchor(usize, Bias),
            Forget(usize),
            Undo,
            Redo,
        }

        /// Multi-byte on purpose: the char-boundary invariant is vacuous over
        /// ASCII-only input.
        fn text_strategy() -> impl Strategy<Value = String> {
            prop_oneof![
                Just(String::new()),
                Just("é".to_string()),
                Just("日本".to_string()),
                Just("🎉".to_string()),
                Just("e\u{301}".to_string()),
                Just("ab".to_string()),
                Just("\n".to_string()),
            ]
        }

        fn bias_strategy() -> impl Strategy<Value = Bias> {
            prop_oneof![Just(Bias::Left), Just(Bias::Right)]
        }

        /// Edits and forgets, but no anchor creation: a property that judges an
        /// anchor against where it was created needs every anchor to predate
        /// every edit.
        fn edit_op_strategy() -> impl Strategy<Value = Op> {
            prop_oneof![
                4 => (any::<usize>(), any::<usize>(), text_strategy())
                    .prop_map(|(a, b, text)| Op::Edit(a, b, text)),
                3 => (
                    any::<usize>(),
                    any::<usize>(),
                    text_strategy(),
                    any::<usize>(),
                    text_strategy(),
                )
                    .prop_map(|(a, b, replaced, c, inserted)| {
                        Op::Transaction(a, b, replaced, c, inserted)
                    }),
                1 => any::<usize>().prop_map(Op::Forget),
            ]
        }

        fn op_strategy() -> impl Strategy<Value = Op> {
            prop_oneof![
                8 => edit_op_strategy(),
                3 => (any::<usize>(), bias_strategy()).prop_map(|(at, bias)| Op::Anchor(at, bias)),
            ]
        }

        fn history_op_strategy() -> impl Strategy<Value = Op> {
            prop_oneof![
                8 => op_strategy(),
                1 => Just(Op::Undo),
                1 => Just(Op::Redo),
            ]
        }

        /// Maps an arbitrary index onto a char boundary of `text`, so a
        /// strategy can generate offsets without knowing the buffer contents.
        fn boundary(text: &str, index: usize) -> usize {
            let boundaries: Vec<usize> = text
                .char_indices()
                .map(|(byte, _)| byte)
                .chain(std::iter::once(text.len()))
                .collect();
            boundaries[index % boundaries.len()]
        }

        fn ordered_boundaries(text: &str, a: usize, b: usize) -> Range<usize> {
            let (a, b) = (boundary(text, a), boundary(text, b));
            a.min(b)..a.max(b)
        }

        /// Whether this edit leaves `offset` able to return to where it was
        /// once the edit is undone.
        ///
        /// Three boundary cases say no, and all three are the asymmetry the
        /// tests above pin: strictly inside the removed span, `Right` at a
        /// removal's start, and `Left` at the exclusive end of a pure
        /// deletion. Everything else comes home exactly.
        fn survives_undo(
            offset: ByteOffset,
            bias: Bias,
            at: ByteOffset,
            removed_len: usize,
            inserted_len: usize,
        ) -> bool {
            if removed_len == 0 {
                return true;
            }
            let touched_end = at.0 + removed_len;
            let strictly_inside = offset.0 > at.0 && offset.0 < touched_end;
            let right_at_start = offset.0 == at.0 && bias == Bias::Right;
            let left_at_end = inserted_len == 0 && offset.0 == touched_end && bias == Bias::Left;
            !(strictly_inside || right_at_start || left_at_end)
        }

        fn apply_edit(buf: &mut Buffer, tracked: &mut [Tracked], range: Range<usize>, text: &str) {
            let removed_len = range.end - range.start;
            if removed_len == 0 && text.is_empty() {
                // `replace` returns early, so nothing may move.
                return;
            }
            let at = ByteOffset(range.start);
            for entry in tracked.iter_mut().filter(|entry| entry.live) {
                entry.restorable &=
                    survives_undo(entry.predicted, entry.bias, at, removed_len, text.len());
                entry.predicted =
                    transform(entry.predicted, entry.bias, at, removed_len, text.len());
            }
            buf.replace(at..ByteOffset(range.end), text);
        }

        fn run(buf: &mut Buffer, tracked: &mut Vec<Tracked>, op: Op) {
            match op {
                Op::Edit(a, b, text) => {
                    let range = ordered_boundaries(buf.text(), a, b);
                    apply_edit(buf, tracked, range, &text);
                }
                Op::Transaction(a, b, replaced, c, inserted) => {
                    let first = ordered_boundaries(buf.text(), a, b);
                    // The second position is chosen against the text the first
                    // edit leaves behind, as a caller composing two edits would.
                    let mut after = buf.text().to_string();
                    after.replace_range(first.clone(), &replaced);
                    let second = boundary(&after, c);
                    buf.transact(|buf| {
                        apply_edit(buf, tracked, first, &replaced);
                        apply_edit(buf, tracked, second..second, &inserted);
                    });
                }
                Op::Anchor(offset, bias) => {
                    let offset = ByteOffset(boundary(buf.text(), offset));
                    let anchor = buf.anchor(offset, bias);
                    tracked.push(Tracked {
                        anchor,
                        bias,
                        created_at: offset,
                        predicted: offset,
                        live: true,
                        restorable: true,
                    });
                }
                Op::Forget(index) => {
                    let live: Vec<usize> = tracked
                        .iter()
                        .enumerate()
                        .filter(|(_, entry)| entry.live)
                        .map(|(index, _)| index)
                        .collect();
                    if live.is_empty() {
                        return;
                    }
                    let chosen = live[index % live.len()];
                    buf.forget(tracked[chosen].anchor);
                    tracked[chosen].live = false;
                }
                Op::Undo => {
                    buf.undo();
                }
                Op::Redo => {
                    buf.redo();
                }
            }
        }

        fn check_addressable(buf: &Buffer, tracked: &[Tracked]) -> Result<(), TestCaseError> {
            for entry in tracked.iter().filter(|entry| entry.live) {
                let offset = buf.resolve(entry.anchor);
                prop_assert!(offset <= buf.len(), "{} escapes {:?}", offset, buf.text());
                prop_assert!(
                    buf.text().is_char_boundary(offset.0),
                    "{} splits a char in {:?}",
                    offset,
                    buf.text()
                );
            }
            Ok(())
        }

        fn live_offsets(buf: &Buffer, tracked: &[Tracked]) -> Vec<Option<ByteOffset>> {
            tracked
                .iter()
                .map(|entry| entry.live.then(|| buf.resolve(entry.anchor)))
                .collect()
        }

        proptest! {
            /// Bounds alone would pass a build that wired `shift_anchors` into
            /// only one of its two call sites, as long as the buffer kept
            /// growing. So every anchor is checked against what `transform`
            /// predicts for the exact edit that was issued.
            #[test]
            fn anchors_move_exactly_as_transform_predicts(
                ops in prop::collection::vec(op_strategy(), 0..24)
            ) {
                let mut buf = Buffer::new("héllo\n日本");
                let mut tracked = Vec::new();

                for op in ops {
                    run(&mut buf, &mut tracked, op);
                    check_addressable(&buf, &tracked)?;
                    for entry in tracked.iter().filter(|entry| entry.live) {
                        prop_assert_eq!(
                            buf.resolve(entry.anchor),
                            entry.predicted,
                            "anchor {:?} drifted in {:?}",
                            entry.anchor,
                            buf.text()
                        );
                    }
                }
            }

            /// Anchors are created before the first edit here, so "back where
            /// it started" is a question with an answer after a full undo.
            #[test]
            fn undoing_every_edit_brings_undisturbed_anchors_home(
                starts in prop::collection::vec((any::<usize>(), bias_strategy()), 0..5),
                ops in prop::collection::vec(edit_op_strategy(), 0..24),
            ) {
                let mut buf = Buffer::new("héllo\n日本");
                let mut tracked = Vec::new();
                for (offset, bias) in starts {
                    run(&mut buf, &mut tracked, Op::Anchor(offset, bias));
                }

                for op in ops {
                    run(&mut buf, &mut tracked, op);
                    check_addressable(&buf, &tracked)?;
                }
                while buf.undo().is_some() {
                    check_addressable(&buf, &tracked)?;
                }

                for entry in tracked.iter().filter(|entry| entry.live && entry.restorable) {
                    prop_assert_eq!(
                        buf.resolve(entry.anchor),
                        entry.created_at,
                        "anchor {:?} did not come home",
                        entry.anchor
                    );
                }
            }

            /// `transform` is monotonic for a fixed bias, so two anchors that
            /// share one can converge but never cross. Two that differ can:
            /// `Left` and `Right` on the same offset are built to part company,
            /// which is why the pairs here are drawn from one bias at a time.
            #[test]
            fn anchors_of_one_bias_never_swap_places(
                ops in prop::collection::vec(history_op_strategy(), 0..24)
            ) {
                let mut buf = Buffer::new("héllo\n日本");
                let mut tracked = Vec::new();

                for op in ops {
                    let before = live_offsets(&buf, &tracked);
                    run(&mut buf, &mut tracked, op);
                    check_addressable(&buf, &tracked)?;
                    let after = live_offsets(&buf, &tracked);

                    for (first, one) in tracked.iter().enumerate().take(before.len()) {
                        for (second, other) in tracked.iter().enumerate().take(before.len()) {
                            if first >= second || one.bias != other.bias {
                                continue;
                            }
                            let (Some(was_one), Some(was_other), Some(now_one), Some(now_other)) =
                                (before[first], before[second], after[first], after[second])
                            else {
                                continue;
                            };
                            if was_one <= was_other {
                                prop_assert!(
                                    now_one <= now_other,
                                    "anchors crossed: {} -> {} against {} -> {}",
                                    was_one,
                                    now_one,
                                    was_other,
                                    now_other
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
