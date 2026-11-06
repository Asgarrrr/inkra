//! The index cache file: a header, then sections (persisted index spec, P2, P3).

mod fingerprint;
mod header;

pub use header::{FORMAT_VERSION, MAX_BYTES, Rejected, decode, encode};

#[cfg(test)]
mod header_tests;
