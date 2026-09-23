//! A path relative to the vault root, with no `..`. Also the rules a *typed*
//! name must satisfy before it can become one.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// UTF-16 code units, the unit APFS and HFS+ count — not bytes, not `char`s.
const MAX_NAME_UNITS: usize = 255;

/// Why a typed name cannot become a note. Reported, never a `VaultError`:
/// nothing here touches the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    /// Nothing but whitespace.
    Empty,
    /// A path separator, or a colon — macOS renders `:` as `/` in Finder.
    Separator,
    /// A leading `.`, which `Fs::read_dir` filters out.
    LeadingDot,
    /// `.` or `..`.
    Reserved,
    /// Longer than the filesystem accepts.
    TooLong,
}

/// Whether a typed name can become a note or a folder. Whitespace is trimmed
/// rather than reported.
#[must_use]
pub fn check_name(name: &str) -> Option<NameProblem> {
    let name = name.trim();
    if name.is_empty() {
        return Some(NameProblem::Empty);
    }
    if name == "." || name == ".." {
        return Some(NameProblem::Reserved);
    }
    if name.starts_with('.') {
        return Some(NameProblem::LeadingDot);
    }
    if name.contains('/') || name.contains(':') {
        return Some(NameProblem::Separator);
    }
    // Measured on the file name, not the typed one: `note_file_name` is what
    // reaches the filesystem, and it is up to four units longer.
    if note_file_name(name).encode_utf16().count() > MAX_NAME_UNITS {
        return Some(NameProblem::TooLong);
    }
    None
}

/// The file name a note typed as `name` gets: always a lowercase `.md`,
/// replacing any existing extension whose case-insensitive form is `md`.
/// Any other extension is kept, so `notes.2026` becomes `notes.2026.md`.
#[must_use]
pub fn note_file_name(name: &str) -> String {
    let name = name.trim();
    let stem = name
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && extension.eq_ignore_ascii_case("md"))
        .map_or(name, |(stem, _)| stem);
    format!("{stem}.md")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(Arc<Path>);

impl RelPath {
    /// `None` when the path is absolute, contains `..`, or has a prefix or
    /// root component. A `.` component is dropped rather than refused, so
    /// `./a.md` and `a.md` normalize to the same `RelPath`.
    #[must_use]
    pub fn new(path: &Path) -> Option<Self> {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        Some(Self(Arc::from(normalized.as_path())))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.components().count().saturating_sub(1)
    }

    /// Component-wise, never `str::starts_with` — byte prefixes would make
    /// `Projects.md` look like a child of the directory `Projects`.
    #[must_use]
    pub fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }

    /// Lossy rather than fallible: a filename that is not valid UTF-8 should
    /// render with replacement characters, not vanish from the vault.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.0
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests;

#[cfg(test)]
mod name_tests;
