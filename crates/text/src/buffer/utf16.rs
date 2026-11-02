use std::ops::Range;

use crate::offset::{ByteOffset, Utf16Offset};

use super::Buffer;

impl Buffer {
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
}

pub(super) fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}
