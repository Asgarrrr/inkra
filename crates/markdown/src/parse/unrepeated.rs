use std::ops::Range;

use pulldown_cmark::Event;

/// pulldown-cmark 0.13.4 emits the subtrees after an empty wikilink alias inside the link too;
/// this drops each copy that starts at or past the innermost open element's end.
pub(crate) struct Unrepeated<I> {
    events: I,
    /// The end of each open element yielded so far, innermost last.
    ends: Vec<usize>,
    /// Depth of the subtree being skipped.
    skipped: usize,
}

impl<I> Unrepeated<I> {
    pub(crate) fn new(events: I) -> Self {
        Self {
            events,
            ends: Vec::new(),
            skipped: 0,
        }
    }
}

impl<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> Iterator for Unrepeated<I> {
    type Item = (Event<'a>, Range<usize>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (event, range) = self.events.next()?;
            match event {
                Event::End(_) if self.skipped > 0 => self.skipped -= 1,
                Event::End(_) => {
                    self.ends.pop();
                    return Some((event, range));
                }
                _ if self.ends.last().is_some_and(|&end| range.start >= end) => {
                    if let Event::Start(_) = event {
                        self.skipped += 1;
                    }
                }
                Event::Start(_) => {
                    self.ends.push(range.end);
                    return Some((event, range));
                }
                _ => return Some((event, range)),
            }
        }
    }
}
