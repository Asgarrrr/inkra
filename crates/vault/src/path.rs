//! A path known to be relative to the vault root, with no `..`.
//!
//! Same discipline as `ByteOffset` and `Utf16Offset` in `inkra-text`: the
//! invariant lives in the type, so no caller has to remember it.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(Arc<Path>);

impl RelPath {
    /// `None` when the path is absolute, contains `..`, or has a prefix or
    /// root component. Escaping the vault root is not an error to report to
    /// the user, it is a path the vault refuses to name.
    ///
    /// A `.` component is dropped rather than refused: `Path::new("./a.md")`
    /// and `Path::new("a.md")` are not equal and do not have the same component
    /// count, yet they name one file, and a join can legitimately produce
    /// either. Refusing the first would lose a real note; keeping both
    /// spellings would indent it one level too deep and let one file occupy two
    /// keys in a `HashMap<RelPath, _>`.
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

    /// Indentation depth in the tree: zero for a top-level entry.
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

    /// The name shown in the sidebar. Lossy rather than fallible: a filename
    /// that is not valid UTF-8 should render with replacement characters, not
    /// disappear from the vault.
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
        // The Obsidian folder-note trap: `Projects.md` must NOT be treated as
        // living inside the directory `Projects`.
        let dir = RelPath::new(Path::new("Projects")).unwrap();
        let sibling = RelPath::new(Path::new("Projects.md")).unwrap();
        let child = RelPath::new(Path::new("Projects/plan.md")).unwrap();
        assert!(!sibling.starts_with(&dir));
        assert!(child.starts_with(&dir));
    }
}
