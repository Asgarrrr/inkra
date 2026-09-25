mod chunks;
mod index;
mod key;
mod names;
mod note_set;
mod notes;
mod source;
mod target;
#[cfg(test)]
mod test_support;

pub use index::{Backlink, NoteIndex};
pub use names::{NameMatch, NameQuery, NamesSnapshot, match_names, merge_names};
pub use note_set::NoteSet;
pub use notes::Resolution;
pub use source::{Source, SourceLink};
pub use target::{Style, Target};
