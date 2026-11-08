mod chunks;
mod content;
mod corpus;
mod find;
mod index;
mod key;
pub mod live;
mod names;
mod note_set;
mod notes;
pub mod persist;
mod query;
mod search;
mod seed;
#[cfg(test)]
mod seed_tests;
mod source;
mod stems;
mod target;
#[cfg(test)]
mod test_support;
mod text_cache;
#[cfg(test)]
mod text_cache_tests;
mod varint;
#[cfg(test)]
mod varint_tests;
mod window;
mod words;

pub use content::{FileRead, NoteContent};
pub use corpus::CorpusSnapshot;
pub use find::{Excerpt, Group, NoteMatch, excerpt, word_starts};
pub use index::{Backlink, IndexSnapshot, NoteIndex};
pub use live::LiveIndex;
pub use names::NamesSnapshot;
pub use note_set::NoteSet;
pub use notes::Resolution;
pub use search::{
    Candidate, HITS_PER_NOTE, Hit, NoteHits, READ_FAILURES, Read, Searched, TextFound, TextSearch,
};
pub use source::{Source, SourceLink};
pub use target::{NoteRef, Style, Target};
pub use text_cache::{TEXT_CACHE_BYTES, TextCache};
pub use words::{
    Lookup, MergeWhen, MergedWords, NoteQuery, RetiredWords, TermHit, WordsMerge, WordsSnapshot,
};
