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
mod tests {
    use super::*;

    #[test]
    fn a_plain_relative_path_is_accepted() {
        let path = RelPath::new(Path::new("notes/today.md")).unwrap();
        assert_eq!(path.as_path(), Path::new("notes/today.md"));
    }

    #[test]
    fn an_absolute_path_is_rejected() {
        assert!(RelPath::new(Path::new("/etc/passwd")).is_none());
    }

    #[test]
    fn a_parent_component_is_rejected_anywhere_in_the_path() {
        assert!(RelPath::new(Path::new("../secrets.md")).is_none());
        assert!(RelPath::new(Path::new("notes/../../secrets.md")).is_none());
    }

    #[test]
    fn a_current_dir_component_is_normalized_away() {
        let dotted = RelPath::new(Path::new("./a.md")).unwrap();
        assert_eq!(dotted, RelPath::new(Path::new("a.md")).unwrap());
        assert_eq!(dotted.depth(), 0);
        assert_eq!(
            RelPath::new(Path::new("notes/./today.md")).unwrap(),
            RelPath::new(Path::new("notes/today.md")).unwrap()
        );
    }

    #[test]
    fn the_depth_is_the_component_count() {
        let path = RelPath::new(Path::new("a/b/c.md")).unwrap();
        assert_eq!(path.depth(), 2);
    }

    #[test]
    fn starts_with_compares_components_not_bytes() {
        let dir = RelPath::new(Path::new("Projects")).unwrap();
        let sibling = RelPath::new(Path::new("Projects.md")).unwrap();
        let child = RelPath::new(Path::new("Projects/plan.md")).unwrap();
        assert!(!sibling.starts_with(&dir));
        assert!(child.starts_with(&dir));
    }
}

#[cfg(test)]
mod name_tests {
    use super::{NameProblem, RelPath, check_name, note_file_name};
    use std::path::Path;

    #[test]
    fn an_ordinary_name_is_accepted() {
        assert_eq!(check_name("Meeting notes"), None);
    }

    #[test]
    fn an_empty_or_blank_name_is_refused() {
        assert_eq!(check_name(""), Some(NameProblem::Empty));
        assert_eq!(check_name("   "), Some(NameProblem::Empty));
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_not_refused() {
        assert_eq!(check_name("  plan  "), None);
        assert_eq!(note_file_name("  plan  "), "plan.md");
    }

    #[test]
    fn a_separator_is_refused_rather_than_read_as_nesting() {
        assert_eq!(check_name("a/b"), Some(NameProblem::Separator));
    }

    #[test]
    fn a_colon_is_refused_because_the_finder_shows_it_as_a_separator() {
        assert_eq!(check_name("plan:notes"), Some(NameProblem::Separator));
    }

    #[test]
    fn a_leading_dot_is_refused() {
        assert_eq!(check_name(".plan"), Some(NameProblem::LeadingDot));
    }

    #[test]
    fn the_reserved_names_are_refused() {
        assert_eq!(check_name("."), Some(NameProblem::Reserved));
        assert_eq!(check_name(".."), Some(NameProblem::Reserved));
    }

    #[test]
    fn a_name_longer_than_the_filesystem_accepts_is_refused() {
        assert_eq!(check_name(&"a".repeat(252)), None);
        assert_eq!(check_name(&"a".repeat(253)), Some(NameProblem::TooLong));
    }

    #[test]
    fn the_length_limit_counts_utf16_units_not_chars() {
        // Each emoji is 2 UTF-16 units; 126 of them plus `.md` is 255 exactly.
        assert_eq!(check_name(&"😀".repeat(126)), None);
        assert_eq!(check_name(&"😀".repeat(127)), Some(NameProblem::TooLong));
    }

    #[test]
    fn a_bare_name_gains_the_extension() {
        assert_eq!(note_file_name("plan"), "plan.md");
    }

    #[test]
    fn an_existing_extension_is_not_doubled() {
        assert_eq!(note_file_name("plan.md"), "plan.md");
    }

    /// `read_dir` matches `md` case-sensitively; `plan.MD` must not survive.
    #[test]
    fn an_uppercase_extension_is_normalized_rather_than_kept() {
        assert_eq!(note_file_name("plan.MD"), "plan.md");
        assert_eq!(note_file_name("plan.Md"), "plan.md");
    }

    #[test]
    fn an_unrelated_extension_is_kept_and_the_note_one_added() {
        assert_eq!(note_file_name("notes.2026"), "notes.2026.md");
    }

    /// An empty stem would leave `.md` — a dotfile `read_dir` would hide.
    #[test]
    fn the_extension_alone_stays_a_name_rather_than_becoming_a_dotfile() {
        assert_eq!(note_file_name(".md"), ".md.md");
    }

    #[test]
    fn every_accepted_name_is_one_relpath_accepts() {
        for name in [
            "plan",
            "plan.md",
            "notes.2026",
            "Meeting notes",
            "a",
            "élan",
            "日本語",
        ] {
            assert_eq!(check_name(name), None, "{name} should be accepted");
            assert!(
                RelPath::new(Path::new(&note_file_name(name))).is_some(),
                "{name} passed check_name but RelPath refused it"
            );
        }
    }
}
