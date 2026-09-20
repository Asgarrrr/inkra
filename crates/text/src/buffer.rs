use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use crate::offset::{ByteOffset, Utf16Offset};

/// Two transactions closer together than this merge into one undo step, so a
/// typing run undoes as a unit. Matches Zed's default.
pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

/// One replacement applied to the text: `removed` used to sit at `at`, and
/// `inserted` sits there now. Storing both sides keeps undo proportional to the
/// size of the edit rather than the size of the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    at: ByteOffset,
    removed: String,
    inserted: String,
}

impl Edit {
    fn inverted(&self) -> Self {
        Self {
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
        self.record(Edit {
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
}
