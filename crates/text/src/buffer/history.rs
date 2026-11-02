use std::ops::{Deref, Range};
use std::time::{Duration, Instant};

use crate::offset::ByteOffset;

use super::Buffer;

pub const GROUP_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Edit {
    /// Monotonic across the buffer's life; `Buffer::version` is the id of the last applied edit.
    /// Lives on the edit, not the `Transaction`: `close` merges by extending `previous.edits` in place.
    id: Version,
    at: ByteOffset,
    pub(super) removed: String,
    pub(super) inserted: String,
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
pub(super) struct Transaction {
    pub(super) edits: Vec<Edit>,
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

impl Buffer {
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
