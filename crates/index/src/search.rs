//! Vault search: each word start of the query's terms in the notes holding them all.

mod hit;
mod run;

pub use hit::{Hit, Searched};
pub use run::{HITS_PER_NOTE, search};

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
