use std::sync::Arc;

use crate::source::Source;
use crate::words::NoteWords;

/// What the index keeps of a note's text. Parse it off the foreground: `insert` only stores it.
pub struct NoteContent {
    pub(crate) source: Source,
    pub(crate) words: NoteWords,
    pub(crate) text: Arc<str>,
}

impl NoteContent {
    #[must_use]
    pub fn parse(text: Arc<str>) -> Self {
        Self {
            source: Source::parse(&text),
            words: NoteWords::parse(&text),
            text,
        }
    }
}

#[cfg(test)]
mod tests;
