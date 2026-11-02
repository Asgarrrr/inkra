use std::borrow::Cow;
use std::ops::Range;

use inkra_text::{Bias, ByteOffset};

/// A UTF-8 byte index into the text a line shows. The index is private, so only `new` builds one:
///
/// ```compile_fail
/// let _ = inkra_markdown::DisplayOffset(3);
/// ```
///
/// ```
/// let _ = inkra_markdown::DisplayOffset::new(3);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayOffset(usize);

impl DisplayOffset {
    #[must_use]
    pub const fn new(offset: usize) -> Self {
        Self(offset)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// One buffer line with some ranges hidden or replaced (live-preview spec, D5).
#[derive(Debug, Clone)]
pub struct DisplayLine {
    line: Range<ByteOffset>,
    /// Sorted, non-overlapping, inside `line`; each replaced by its `&str`.
    hidden: Vec<(Range<ByteOffset>, &'static str)>,
}

/// A run of the line: replaced when `replacement` is set, else shown byte for byte.
struct Segment {
    buffer: Range<usize>,
    display: Range<usize>,
    replacement: Option<&'static str>,
}

impl DisplayLine {
    /// `line` excludes its `\n`; `hidden` is sorted and non-overlapping, inside `line`.
    #[must_use]
    pub fn new(
        line: Range<ByteOffset>,
        hidden: impl IntoIterator<Item = (Range<ByteOffset>, &'static str)>,
    ) -> Self {
        let hidden: Vec<_> = hidden.into_iter().collect();
        debug_assert!(line.start <= line.end);
        debug_assert!(hidden.iter().all(|(range, _)| {
            line.start <= range.start && range.start <= range.end && range.end <= line.end
        }));
        debug_assert!(
            hidden
                .windows(2)
                .all(|pair| pair[0].0.end <= pair[1].0.start)
        );
        Self { line, hidden }
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.hidden.is_empty()
    }

    pub fn hidden(&self) -> impl Iterator<Item = &Range<ByteOffset>> {
        self.hidden.iter().map(|(range, _)| range)
    }

    /// `line` is the buffer line's text, without its `\n`.
    #[must_use]
    pub fn text<'a>(&self, line: &'a str) -> Cow<'a, str> {
        if self.is_identity() {
            return Cow::Borrowed(line);
        }
        let start = self.line.start.get();
        let mut shown = String::with_capacity(line.len());
        for segment in self.segments() {
            let buffer = segment.buffer.start - start..segment.buffer.end - start;
            shown.push_str(segment.replacement.unwrap_or_else(|| &line[buffer]));
        }
        Cow::Owned(shown)
    }

    /// An offset strictly inside a hidden range clamps to that range's display start.
    #[must_use]
    pub fn to_display(&self, offset: ByteOffset) -> DisplayOffset {
        let offset = offset
            .get()
            .clamp(self.line.start.get(), self.line.end.get());
        let index = match self.segments().find(|s| offset <= s.buffer.end) {
            Some(s) if s.replacement.is_none() => s.display.start + (offset - s.buffer.start),
            Some(s) if offset == s.buffer.end => s.display.end,
            Some(s) => s.display.start,
            None => 0,
        };
        DisplayOffset(index)
    }

    /// The smallest (`Left`) or largest (`Right`) offset that maps to `index`
    /// and is not strictly inside a hidden range.
    #[must_use]
    pub fn to_buffer(&self, index: DisplayOffset, bias: Bias) -> ByteOffset {
        if index.0 > self.display_len() {
            return self.line.end;
        }
        let index = self.snap(index.0, bias);
        let segment = match bias {
            Bias::Left => self.segments().find(|s| index <= s.display.end),
            Bias::Right => self
                .segments()
                .take_while(|s| s.display.start <= index)
                .last(),
        };
        let offset = match segment {
            Some(s) if s.replacement.is_none() => s.buffer.start + (index - s.display.start),
            Some(s) if bias == Bias::Left && index == s.display.start => s.buffer.start,
            Some(s) if bias == Bias::Right && index != s.display.end => s.buffer.start,
            Some(s) => s.buffer.end,
            None => self.line.end.get(),
        };
        ByteOffset::new(offset)
    }

    fn display_len(&self) -> usize {
        self.segments().last().map_or(0, |s| s.display.end)
    }

    /// An index strictly inside a replacement moves to its start (`Left`) or end (`Right`).
    fn snap(&self, index: usize, bias: Bias) -> usize {
        let inside = self
            .segments()
            .find(|s| s.replacement.is_some() && s.display.start < index && index < s.display.end);
        match (inside, bias) {
            (Some(s), Bias::Left) => s.display.start,
            (Some(s), Bias::Right) => s.display.end,
            (None, _) => index,
        }
    }

    fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        let mut buffer = self.line.start.get();
        let mut display = 0;
        let mut hidden = self.hidden.iter().peekable();
        std::iter::from_fn(move || {
            let (range, replacement) = match hidden.peek() {
                Some((range, _)) if buffer < range.start.get() => (buffer..range.start.get(), None),
                Some((range, replacement)) => {
                    (range.start.get()..range.end.get(), Some(*replacement))
                }
                None if buffer < self.line.end.get() => (buffer..self.line.end.get(), None),
                None => return None,
            };
            if replacement.is_some() {
                hidden.next();
            }
            let len = replacement.map_or(range.len(), str::len);
            let segment = Segment {
                buffer: range.clone(),
                display: display..display + len,
                replacement,
            };
            (buffer, display) = (range.end, display + len);
            Some(segment)
        })
    }
}

#[cfg(test)]
mod tests;
