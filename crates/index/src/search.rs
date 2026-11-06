//! Vault search: each word start of the query's terms in the notes holding them all.

mod hit;
mod run;
mod text;

pub use hit::{Hit, NoteHits, Searched};
pub use run::HITS_PER_NOTE;
pub(crate) use run::search_in;
pub use text::{Candidate, READ_FAILURES, Read, TextFound, TextSearch};

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod text_tests;
