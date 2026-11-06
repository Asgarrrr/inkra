use inkra_vault::Stamp;

use crate::source::Source;
use crate::words::NoteWords;

/// What the index keeps of a note's text. Parse it off the foreground: `insert` only stores it.
pub struct NoteContent {
    pub(crate) source: Source,
    pub(crate) words: NoteWords,
    /// The stamp of the parsed text: the loaded file's, or the save's.
    pub(crate) stamp: Stamp,
}

impl NoteContent {
    #[must_use]
    pub fn parse(text: &str, stamp: Stamp) -> Self {
        Self {
            source: Source::parse(text),
            words: NoteWords::parse(text),
            stamp,
        }
    }
}

#[cfg(test)]
mod tests;
