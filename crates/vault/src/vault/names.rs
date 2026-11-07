//! Whether a proposed name is already a child of its folder (create-rename-delete spec).

use super::{Entry, EntryId, Vault};
use crate::fs::Kind;
use crate::note_file_name;

impl Vault {
    /// Whether `name`, typed for a `kind`, collides with a child of `parent` (the root when `None` or gone).
    /// `except` is excluded so a same-name (or case-only) rename is a no-op, not a duplicate.
    #[must_use]
    pub fn name_taken(
        &self,
        parent: Option<EntryId>,
        kind: Kind,
        name: &str,
        except: Option<EntryId>,
    ) -> bool {
        let wanted = kind.file_name(name);
        let Some(parent_path) = parent.and_then(|id| self.entry(id)).map(|e| &e.path) else {
            return self.entries().any(|entry| {
                Some(entry.id) != except && entry.path.depth() == 0 && on_disk_name(entry) == wanted
            });
        };
        self.entries().any(|entry| {
            Some(entry.id) != except
                && entry.path.depth() == parent_path.depth() + 1
                && entry.path.starts_with(parent_path)
                && on_disk_name(entry) == wanted
        })
    }
}

/// Normalised as a typed note name is, so `plan`, `plan.md` and `plan.MD` collide as one name.
fn on_disk_name(entry: &Entry) -> String {
    let name = entry
        .path
        .as_path()
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    match entry.kind {
        Kind::Note => note_file_name(&name),
        Kind::Dir | Kind::Asset => name,
    }
}
