//! Literal full-text search over the saved notes (spec D3, D4).

mod hit;
mod query;
mod run;

pub use hit::{Hit, Searched};
pub use query::TextQuery;
pub use run::{HITS_PER_NOTE, search};

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod tests;
