use std::path::PathBuf;
use std::sync::Arc;

use inkra_vault::{Fs, Saved, Stamp};

/// Serialises one note's writes, and drops one that a newer write overtook.
///
/// One note normally has exactly one writer: `schedule_save` chains every write
/// behind the previous one. The quit flush is the exception, and cannot be
/// chained — `App::shutdown` blocks the foreground session while it polls the
/// quit futures (`gpui/src/app.rs:1073`,
/// `scheduler/src/test_scheduler.rs:100`, `:362`), so a quit write that awaited
/// the `cx.spawn` chain would wait for a task that can never resume. It
/// therefore issues its own write, and for a moment a note has two.
///
/// Dropping `NoteStore` does not settle this. `windows.clear()` releases the
/// store before the quit futures are polled, which cancels the chain's `Task`
/// — but cancelling a `Task` only stops it being polled *again*
/// (`async-task/src/task.rs:440-445`), and `RealFs::save` has no `.await` in
/// it, so a write a background thread already began runs to its `rename()`
/// regardless. The loser of that race is whichever `rename()` lands last, and
/// without ordering that can be the *older* text.
///
/// The lock alone cannot order them: `futures::lock::Mutex` documents no
/// fairness at all (`futures-util-0.3.34/src/lock/mutex.rs:14-21`), so
/// acquisition order is not issue order. The sequence is what decides; the lock
/// only stops the two `rename()`s interleaving.
pub(super) struct Writer {
    /// The newest write that reached the filesystem, and what it left behind.
    ///
    /// Locked and unlocked entirely inside a background task. No foreground
    /// code may hold this guard: the quit future is polled while the foreground
    /// session is blocked, so a guard held by a `cx.spawn` chain would never be
    /// released and shutdown would spin until its timeout.
    state: futures::lock::Mutex<WriterState>,
}

impl Writer {
    /// The constructor, so `WriterState`'s fields can stay private to this
    /// file: `NoteStore::track` builds this from a sibling module and must not
    /// reach into them.
    pub(super) fn new(stamp: Stamp) -> Self {
        Self {
            state: futures::lock::Mutex::new(WriterState {
                // Zero is below every minted sequence, which starts at one, so
                // the first write of a note is never mistaken for a stale one.
                written: 0,
                stamp,
            }),
        }
    }
}

/// The expectation lives here, and not beside `saved_version` on `OpenNote`,
/// because a write captures its text under `&mut App` and only *then* waits for
/// the lock. A second write issued while the first is in flight would therefore
/// carry a stamp the first is about to invalidate, find a file it did not
/// recognise — one it had just written itself — and preserve our own new text
/// into a conflict file. Ordinary typing would mint junk beside every note.
///
/// Read after the lock, the expectation is always what our own last write
/// actually produced, or what the load returned before there was one.
struct WriterState {
    /// The sequence of the newest write that reached the filesystem.
    written: u64,
    /// What the file looked like after that write.
    stamp: Stamp,
}

/// Writes one note's text, unless a newer write already landed.
///
/// `None` means exactly that — overtaken, not failed. The caller must not
/// report it as a failure, and must not advance `saved_version`: the text on
/// disk is the *other* write's, not this one's.
///
/// Free rather than a method because both writers call it from a background
/// task, where no `App` is reachable (`gpui/src/app.rs:1004-1013` holds the
/// `RefCell` across the whole of `shutdown`).
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
    // Only a write that landed advances the mark, so the field keeps meaning
    // what it says. Not covered by a test, and deliberately so: sequences are
    // minted monotonically and used once, so a retry always carries a higher
    // number than the failure before it, and no reachable interleaving can tell
    // the two versions apart. The guard is here to keep the invariant true for
    // the next reader of `written`, not because a test demands it.
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
                // These tests drive `write_through` directly against a file
                // they wrote themselves, so the seed is never what the note
                // carries — which is the point: the first write of each is
                // expected to find a mismatch and preserve, and they assert on
                // the note's final text, which is unaffected either way.
                stamp: Stamp {
                    mtime: Mtime(std::time::SystemTime::UNIX_EPOCH),
                    size: 0,
                },
            }),
        })
    }

    /// The ordering guard, driven directly rather than raced.
    ///
    /// A `#[gpui::test]` cannot put this under test honestly. `TestScheduler`
    /// runs every runnable on the one thread driving the block
    /// (`vendor/zed-reference/crates/scheduler/src/test_scheduler.rs:308-417`),
    /// so it can model "queued and not yet polled" but never the production
    /// hazard, which is a background thread already inside `rename()`. And a
    /// test that let the scheduler pick the order would assert whatever seed 0
    /// happens to produce, for as long as gpui's scheduling weights stay put.
    ///
    /// Driving `write_through` by hand fixes both: it needs no window, no
    /// scheduler and no clock, so each interleaving is chosen rather than
    /// hoped for. The `scroll.rs` precedent — a plain `#[test]` in `app` for
    /// something that cannot move down a layer.
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

    /// The R2 revert, forced. Without the sequence the older write lands last
    /// and the file holds text the user had already replaced — the loss this
    /// whole feature exists to prevent, arriving through the quit flush that is
    /// supposed to be the safety net.
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

        // `None`, not `Some(Err(_))`: overtaken is not failed, and a caller
        // that reported it would raise `SaveFailed` for a write that was
        // simply too late to matter.
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
