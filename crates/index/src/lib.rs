mod chunks;
mod content;
mod corpus;
mod index;
mod key;
mod names;
mod note_set;
mod notes;
mod search;
mod source;
mod target;
#[cfg(test)]
mod test_support;
mod words;

pub use content::NoteContent;
pub use corpus::CorpusSnapshot;
pub use index::{Backlink, NoteIndex};
pub use names::{NameMatch, NameQuery, NamesSnapshot, match_names, merge_names};
pub use note_set::NoteSet;
pub use notes::Resolution;
pub use search::{HITS_PER_NOTE, Hit, Searched, TextQuery, search};
pub use source::{Source, SourceLink};
pub use target::{Style, Target};
pub use words::{
    Lookup, MergeWhen, MergedWords, NoteQuery, RetiredWords, TermHit, WordsMerge, WordsSnapshot,
};
