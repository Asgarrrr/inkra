//! The index cache file: a header, then sections (persisted index spec, P2, P3).

mod bytes;
mod fingerprint;
mod header;
mod notes;
mod scrub;
mod words;
mod writes;

pub use header::{Decoded, FORMAT_VERSION, MAX_BYTES, Rejected, decode, encode};
pub use notes::CachedNote;
pub(crate) use scrub::scrub_sample;
pub use scrub::{SCRUB, ScrubJob};
pub use words::CachedWords;
pub use writes::{CHANGES, Due, MAX_BACKOFF, QUIET, Writes};

/// What the cache read gave the index: the turn input that ends `LiveIndex::await_cache` (P7).
#[derive(Debug)]
pub enum Outcome {
    Decoded(Decoded),
    Rejected(Rejected),
    /// No file, an I/O error, or no `HOME`.
    Missing,
}

impl Outcome {
    /// As the launch line names it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Decoded(_) => "decoded",
            Self::Rejected(_) => "rejected",
            Self::Missing => "missing",
        }
    }
}

#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod notes_tests;
#[cfg(test)]
mod scrub_tests;
#[cfg(test)]
mod words_tests;
#[cfg(test)]
mod writes_tests;
