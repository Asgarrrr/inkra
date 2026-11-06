use inkra_vault::{FileIdentity, Stamp};

use crate::source::Source;
use crate::words::NoteWords;

/// The file a text was read from, as it was when read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRead {
    pub stamp: Stamp,
    pub identity: FileIdentity,
}

/// What the index keeps of a note's text. Parse it off the foreground: `insert` only stores it.
pub struct NoteContent {
    pub(crate) source: Source,
    pub(crate) words: NoteWords,
    /// The file of the parsed text: the loaded one, or the save's.
    pub(crate) read: FileRead,
}

impl NoteContent {
    #[must_use]
    pub fn parse(text: &str, file: FileRead) -> Self {
        Self {
            source: Source::parse(text),
            words: NoteWords::parse(text),
            read: file,
        }
    }
}

#[cfg(test)]
mod tests;
