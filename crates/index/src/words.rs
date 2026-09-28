//! The word index: which notes hold a word starting with a term (smart quick open spec S1, S2).

mod fold;
mod lookup;
mod name_words;
mod note_words;
mod ordinal;
mod policy;
mod query;
mod score;
mod segment;
mod shards;
mod store;
mod varint;

pub(crate) use fold::{cap, fold, word_spans};
pub use lookup::{Lookup, TermHit};
pub(crate) use name_words::NameWords;
pub(crate) use note_words::NoteWords;
#[cfg(test)]
pub(crate) use policy::FRESH_MIN;
pub use policy::MergeWhen;
pub use query::NoteQuery;
#[cfg(test)]
pub(crate) use score::{bm25, idf};
pub(crate) use store::Words;
pub use store::{MergedWords, RetiredWords, WordsMerge, WordsSnapshot};

/// Above `TERM_MAX_CHARS`, so a capped word keeps every prefix a term can ask for and never
/// equals a term.
pub(crate) const WORD_MAX_CHARS: usize = TERM_MAX_CHARS + 1;
pub(crate) const TERM_MAX_CHARS: usize = 64;

fn wide(n: u32) -> usize {
    let Ok(n) = usize::try_from(n) else {
        panic!("a u32 fits a usize on every supported target");
    };
    n
}

fn narrow(offset: usize) -> u32 {
    let Ok(offset) = u32::try_from(offset) else {
        panic!("the word index holds over 4 GiB of words or postings: {offset}");
    };
    offset
}

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
