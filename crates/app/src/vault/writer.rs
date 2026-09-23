use std::path::PathBuf;
use std::sync::Arc;

use inkra_vault::{Fs, Saved, Stamp};

/// Two writers exist momentarily (the periodic chain and the quit flush);
/// the sequence orders them, since the lock has no fairness guarantee.
pub(super) struct Writer {
    /// No foreground code may hold this guard, or shutdown spins until its timeout.
    state: futures::lock::Mutex<WriterState>,
}

impl Writer {
    pub(super) fn new(stamp: Stamp) -> Self {
        Self {
            state: futures::lock::Mutex::new(WriterState {
                // Below every minted sequence, so the first write is never
                // mistaken for a stale one.
                written: 0,
                stamp,
            }),
        }
    }
}

/// Lives here, not beside `saved_version` on `OpenNote`: a second write
/// issued before the first reaches the lock would otherwise preserve its
/// own new text into a conflict file.
struct WriterState {
    written: u64,
    stamp: Stamp,
}

/// `None` means overtaken, not failed — the caller must not report it and
/// must not advance `saved_version`.
pub(super) async fn write_through(
    writer: Arc<Writer>,
    fs: Arc<dyn Fs>,
    path: PathBuf,
    text: String,
    seq: u64,
) -> Option<inkra_vault::Result<Saved>> {
    let mut state = writer.state.lock().await;
    if seq < state.written {
        return None;
    }
    let result = fs.save(&path, &text, state.stamp).await;
    // Only a landed write advances the mark, or a failure corrupts the
    // next conflict check.
    if let Ok(saved) = &result {
        state.written = seq;
        state.stamp = saved.stamp;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use inkra_vault::mem_fs::MemFs;
    use inkra_vault::{Fs, Mtime, Stamp};

    fn writer() -> Arc<super::Writer> {
        Arc::new(super::Writer {
            state: futures::lock::Mutex::new(super::WriterState {
                written: 0,
                // Deliberately mismatched: the first write is expected to
                // find a conflict and preserve.
                stamp: Stamp {
                    mtime: Mtime(std::time::SystemTime::UNIX_EPOCH),
                    size: 0,
                },
            }),
        })
    }

    /// Driven directly rather than raced: a `#[gpui::test]` can't model a
    /// background thread already inside `rename()`.
    #[test]
    fn a_later_write_wins_when_it_lands_second() {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let writer = writer();
        let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

        let older = super::write_through(
            Arc::clone(&writer),
            Arc::clone(&dyn_fs),
            PathBuf::from("/vault/a.md"),
            "older".to_owned(),
            1,
        );
        let newer = super::write_through(
            writer,
            dyn_fs,
            PathBuf::from("/vault/a.md"),
            "newer".to_owned(),
            2,
        );
        assert!(futures::executor::block_on(older).is_some_and(|r| r.is_ok()));
        assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("newer")
        );
    }

    #[test]
    fn a_later_write_wins_when_it_lands_first() {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let writer = writer();
        let dyn_fs = Arc::clone(&fs) as Arc<dyn Fs>;

        let newer = super::write_through(
            Arc::clone(&writer),
            Arc::clone(&dyn_fs),
            PathBuf::from("/vault/a.md"),
            "newer".to_owned(),
            2,
        );
        let older = super::write_through(
            writer,
            dyn_fs,
            PathBuf::from("/vault/a.md"),
            "older".to_owned(),
            1,
        );
        assert!(futures::executor::block_on(newer).is_some_and(|r| r.is_ok()));

        assert!(
            futures::executor::block_on(older).is_none(),
            "the stale write must be skipped, not merely reported"
        );
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("newer"),
            "the older write must not have reached the file at all"
        );
    }
}
