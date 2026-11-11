use std::io::ErrorKind;
use std::path::Path;

use super::MemFs;
use super::errors::{io, stamp_of};
use crate::fs::{Result, Saved, Stamp, VaultError};
use crate::temp::temp_path;

impl MemFs {
    /// Mints a fresh identity, unlike `insert_file`: `RealFs::save` renames a
    /// temp file over the destination, so the inode changes too.
    pub(super) async fn save_text(
        &self,
        path: &Path,
        text: &str,
        expected: Stamp,
    ) -> Result<Saved> {
        let blocked = self.state().blocked_saves.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        {
            let mut state = self.state();
            state.injected_save_failure(path)?;
            if state.unwritable.contains(path) {
                return Err(io(path, ErrorKind::PermissionDenied));
            }
            if state.missing_parent(path) {
                return Err(io(path, ErrorKind::NotFound));
            }
        }

        // Guard scoped to this block: `state()`'s mutex is not reentrant.
        let preserved = {
            let state = self.state();
            match state.files.get(path) {
                Some((existing, mtime, _)) if stamp_of(existing, *mtime) != expected => {
                    let destination = crate::conflict::conflict_path(path, *mtime, |candidate| {
                        state.exists(candidate)
                    });
                    Some((destination, existing.clone(), stamp_of(existing, *mtime)))
                }
                _ => None,
            }
        };
        if let Some((destination, bytes, stamp)) = preserved.clone() {
            self.insert_bytes(destination.clone(), bytes);
            if let Err(source) = self.state().injected_failure_after_preserve(path) {
                return Err(VaultError::Unwritten {
                    preserved: destination,
                    stamp,
                    source: Box::new(source),
                });
            }
        }

        let (stamp, identity, blocked) = {
            let mut state = self.state();
            let identity = Self::fresh_identity(&mut state);
            let bytes = text.as_bytes().to_vec();
            let mtime = Self::put_file(&mut state, path.to_path_buf(), bytes, Some(identity));
            let stamp = stamp_of(text.as_bytes(), mtime);
            // As `RealFs::save`: a temporary file, renamed onto the note.
            state.changed(&temp_path(path));
            state.changed(path);
            (stamp, identity, state.blocked_after_saves.remove(path))
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        Ok(Saved {
            stamp,
            identity,
            preserved: preserved.map(|(destination, _, _)| destination),
        })
    }
}
