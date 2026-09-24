mod index;
mod key;
mod notes;
mod source;
mod target;
#[cfg(test)]
mod test_support;

pub use index::{Backlink, LinkIndex};
pub use notes::Resolution;
pub use source::{Source, SourceLink};
pub use target::{Style, Target};
