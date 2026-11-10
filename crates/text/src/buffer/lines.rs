use std::ops::Range;

use crate::offset::ByteOffset;

use super::Buffer;

impl Buffer {
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
    #[expect(
        clippy::naive_bytecount,
        reason = "the `bytecount` crate is a dependency to count one byte"
    )]
    fn newlines_before(&self, end: usize) -> usize {
        self.text.as_bytes()[..end]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
    }
}
