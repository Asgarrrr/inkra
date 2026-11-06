//! The index cache file: a header, then sections (persisted index spec, P2, P3).

mod fingerprint;
mod header;
mod notes;

pub use header::{Decoded, FORMAT_VERSION, MAX_BYTES, Rejected, decode, encode};
pub use notes::CachedNote;

#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod notes_tests;
