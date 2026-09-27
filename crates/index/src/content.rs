use std::sync::Arc;

use inkra_vault::Mtime;

use crate::source::Source;
use crate::words::NoteWords;

/// What the index keeps of a note's text. Parse it off the foreground: `insert` only stores it.
pub struct NoteContent {
    pub(crate) source: Source,
    pub(crate) words: NoteWords,
    pub(crate) text: Arc<str>,
    /// The mtime of `text`: the loaded file's, or the save's.
    pub(crate) modified: Mtime,
}

impl NoteContent {
    #[must_use]
    pub fn parse(text: Arc<str>, modified: Mtime) -> Self {
        Self {
            source: Source::parse(&text),
            words: NoteWords::parse(&text),
            text,
            modified,
        }
    }
}

#[cfg(test)]
mod tests;
