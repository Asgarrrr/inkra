//! Smart quick open: notes with a word of their name or text starting each term (S3).

mod excerpt;
mod highlight;
mod plain;
mod preview;
mod rank;
mod run;
mod terms;

use std::ops::Range;
use std::sync::Arc;

use inkra_vault::EntryId;

use rank::Rank;

pub use excerpt::{Excerpt, excerpt};
#[cfg(test)]
pub(crate) use highlight::{boundaries, prefix};
pub(crate) use highlight::{light, word_prefix};
pub use run::{find_notes, merge_notes};
pub(crate) use terms::{Terms, each_word};

/// Where a note's terms matched; the groups list in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// Every term starts a word of the stem or of the folders.
    Name,
    /// Some terms in the name, the rest in the text only.
    NameAndText,
    /// Every term in the text only.
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteMatch {
    pub id: EntryId,
    /// The path `name_highlights` index: NFC, `/`-separated, `.md` stripped.
    pub path: Arc<str>,
    pub group: Group,
    /// Ranges into `path`, whole graphemes, ascending; one may cross the last `/`, so clamp.
    pub name_highlights: Vec<Range<usize>>,
    rank: Rank,
}

/// A note `find_notes` kept; `merge_notes` builds the `NoteMatch` of the ones it lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ranked(Rank);

#[cfg(test)]
mod excerpt_tests;
#[cfg(test)]
mod highlight_tests;
#[cfg(test)]
mod markup_tests;
#[cfg(test)]
pub(crate) mod model_cases;
#[cfg(test)]
mod model_seed_tests;
#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod terms_tests;
#[cfg(test)]
mod tests;
