# Conflict Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** No write may destroy bytes it did not put there — a note changed on
disk by another writer is preserved beside the note before the note is replaced.

**Architecture:** `Fs::save` takes a required `Stamp` — `(mtime, size)` — that
the caller believes the file carries. `RealFs` stats the destination
immediately before replacing it and, on a mismatch, copies the existing bytes to
a sibling `Note (conflict <mtime>Z).md` before proceeding. The expectation is
held by the per-note `Writer` under the lock that already orders that note's
writes, not by `OpenNote`, because a stamp captured before the lock is stale by
the time the write runs. A conflict rescans the vault so the new file reaches
the sidebar, and emits an event. That rescan is why Task 7 splits
`Vault::rescan` in two: it holds `&mut self` across an await, which an
`Entity<Vault>` cannot do.

**Tech Stack:** Rust, no new dependencies. `std::fs`, `async_trait`, `gpui`
entities and `futures::lock::Mutex`, all already in the tree.

**Spec:** `docs/superpowers/specs/2026-09-21-conflict-detection-design.md`.
Read it before Task 1; it records why each choice beat its alternatives.

**Branch:** `conflict-detection`, based on `vault-open-list-save` at `ff77210`.
`./scripts/check.sh` green at that commit.

---

## Verified constraints

Read in this repo, and in `vendor/zed-reference` at pinned commit `526c95d`.

| # | Claim | Source |
|---|---|---|
| C1 | `Loaded` and `Saved` both carry an `Mtime` and neither field has a reader | `crates/vault/src/fs.rs:40-55` |
| C2 | `RealFs::save` already stats the destination, into `existing`, and uses it for the hard-link and symlink decision and for the mode restore | `crates/vault/src/real_fs.rs:109`, `:124-132`, `:172-175` |
| C3 | `RealFs::save` stats again after the write to build `Saved` | `crates/vault/src/real_fs.rs:134-138` |
| C4 | `MemFs::now` returns a constant, so every `MemFs` write shares one mtime | `crates/vault/src/mem_fs.rs:75-77` |
| C5 | `MemFs::insert_file` keeps an existing path's identity and already models "an external writer rewriting a file in place" | `crates/vault/src/mem_fs.rs:79-101`, `:262-267` |
| C6 | `MemFs::save` mints a fresh identity, because `RealFs::save` renames and so changes the inode | `crates/vault/src/mem_fs.rs:262-299` |
| C7 | `Writer` holds `written: futures::lock::Mutex<u64>`, locked and unlocked entirely inside a background task | `crates/app/src/vault.rs:59-67` |
| C8 | `write_through` takes the lock, compares `seq`, writes, and advances `written` only on success | `crates/app/src/vault.rs:78-100` |
| C9 | `NoteStore::write` captures text and version under `&mut App`, spawns the write, then re-keys the vault and advances `saved_version` | `crates/app/src/vault.rs:442-501` |
| C10 | `flush_all` builds its writes from `note.writer`, the same `Arc` the chain uses | `crates/app/src/vault.rs:390-430` |
| C11 | `Vault::rescan` has no caller outside the test suite | `crates/app/src/vault.rs:987-991`; confirmed by `grep -rn rescan crates/` |
| C12 | `main.rs` loads the launch file and discards everything but `text` | `crates/app/src/main.rs:66-67` |
| C13 | `read_dir` yields a file only when its extension is `md`, so a conflict file must end in `.md` to be listed | `crates/vault/src/real_fs.rs:74` |
| C14 | Zed's autosave refuses on conflict and is a silent no-op; the prompt is reserved for an explicit save | `vendor/zed-reference/crates/workspace/src/item.rs:586-589`, `crates/workspace/src/pane.rs:2539-2563` |

---

## File structure

| File | Change | Responsibility after the change |
|---|---|---|
| `crates/vault/src/fs.rs` | Modify | Adds `Stamp`; `Loaded` and `Saved` carry it instead of a bare `Mtime`; `Saved` reports `preserved`; `Fs::save` takes the expectation |
| `crates/vault/src/civil.rs` | Create | One function: a Unix timestamp to `YYYY-MM-DD HH-MM-SS`. Nothing else |
| `crates/vault/src/conflict.rs` | Create | One function: the conflict file's path for a note and a preserved mtime, avoiding collisions |
| `crates/vault/src/real_fs.rs` | Modify | Compares the stamp, preserves the existing bytes, then replaces as before |
| `crates/vault/src/mem_fs.rs` | Modify | A clock that advances per mutation, so two writes differ; the same preserve behaviour |
| `crates/vault/src/lib.rs` | Modify | Exports `Stamp` |
| `crates/app/src/vault.rs` | Modify | `Writer` holds the expectation; `write_through` uses it; a conflict rescans and emits |
| `crates/app/src/main.rs` | Modify | Carries the launch file's stamp through to `adopt` |
| `crates/vault/tests/real_fs.rs` | Modify | Proves the real stat, copy and rename compose |

`civil.rs` and `conflict.rs` are separate from `real_fs.rs` because both are
pure functions over values, testable with no filesystem at all, and `real_fs.rs`
is the one file in `vault` that cannot be.

---

## Task 1: `Stamp`, and the API that carries it

**Files:**
- Modify: `crates/vault/src/fs.rs:19-20`, `:40-55`, `:67-88`
- Modify: `crates/vault/src/lib.rs:16`

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block at the end of `crates/vault/src/fs.rs`:

```rust
    #[test]
    fn a_stamp_differs_when_only_the_size_does() {
        use super::{Mtime, Stamp};
        use std::time::{Duration, SystemTime};

        let at = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
        assert_ne!(
            Stamp { mtime: at, size: 1 },
            Stamp { mtime: at, size: 2 }
        );
    }

    #[test]
    fn a_stamp_differs_when_the_mtime_moves_backwards() {
        use super::{Mtime, Stamp};
        use std::time::{Duration, SystemTime};

        // A sync client preserving an original modified date hands back a file
        // older than the one we wrote. An ordering comparison calls that
        // unchanged; this is why the check is inequality.
        let newer = Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(20)),
            size: 4,
        };
        let older = Stamp {
            mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10)),
            size: 4,
        };
        assert_ne!(newer, older);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p inkra-vault --all-features a_stamp`
Expected: FAIL, `cannot find type 'Stamp' in this scope`.

- [ ] **Step 3: Add the type**

In `crates/vault/src/fs.rs`, immediately after the `Mtime` declaration at
`:19-20`:

```rust
/// What a note looked like on disk, as cheaply as a `stat` can tell us.
///
/// Compared with `!=`, never with an ordering. A sync client that preserves a
/// file's original modified date hands back an mtime *older* than the one we
/// wrote, and `>` calls that unchanged and overwrites it. Zed compares with
/// ordering through a method named `bad_is_greater_than`
/// (`vendor/zed-reference/crates/fs/src/fs.rs:422-425`), on a type that refuses
/// to derive `Ord` precisely because the comparison is unsound.
///
/// `size` is the second term because mtime granularity is one second on HFS+
/// and coarser on some network mounts, so two writes can share a timestamp.
/// It does not close the gap — same granule and same length still compares
/// equal — and the spec records why a content hash is not worth its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: Mtime,
    pub size: u64,
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p inkra-vault --all-features a_stamp`
Expected: PASS, 2 tests.

- [ ] **Step 5: Carry it through `Loaded` and `Saved`, and take it in `save`**

In `crates/vault/src/fs.rs`, replace the `Loaded` and `Saved` declarations at
`:37-55` with:

```rust
/// Only `text` and `stamp` have callers today. `identity` is what the vault
/// keys an `EntryId` by.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub text: String,
    pub stamp: Stamp,
    pub identity: FileIdentity,
}

/// What a write produced. Symmetric with `Loaded`, and for one reason:
/// `RealFs::save` renames a temp file over the destination, so the file's
/// `(device, inode)` after a save is not the one before it. A caller that keys
/// anything by identity — `Vault` keys every `EntryId` — has to be told.
#[derive(Debug, Clone)]
pub struct Saved {
    pub stamp: Stamp,
    pub identity: FileIdentity,
    /// Where the bytes this write replaced were preserved, when the file had
    /// moved out from under the caller's expectation. `None` is the ordinary
    /// case: the file was exactly what the caller thought it was.
    pub preserved: Option<PathBuf>,
}
```

`Saved` loses `Copy`: `PathBuf` is not `Copy`. Its one caller clones or moves.

Then replace the `save` signature at `:78-87`:

```rust
    /// Replaces the file rather than truncating it, so a crash mid-write cannot
    /// leave a half-written note. Zed does the opposite for buffer saves
    /// (`vendor/zed-reference/crates/fs/src/fs.rs:1019-1033` is `File::create`)
    /// and reserves `atomic_write` (`:974`) for its own config files; it can
    /// afford to, because its autosave defaults to off and the user picks the
    /// moment. Ours writes unprompted.
    ///
    /// `expected` is what the caller believes is on disk. When the file says
    /// otherwise, its current bytes are preserved beside it before this write
    /// lands, and the path they went to comes back in `Saved::preserved`.
    /// Required rather than optional: an `Option` would read as "write anyway",
    /// which is the clobber this parameter exists to remove.
    ///
    /// Returns the identity the file carries **afterwards**, which a rename
    /// changes. See `Saved`.
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved>;
```

- [ ] **Step 6: Export it**

In `crates/vault/src/lib.rs:16`, add `Stamp` to the `pub use`:

```rust
pub use fs::{DirEntry, FileIdentity, Fs, Kind, Loaded, Mtime, Result, Saved, Stamp, VaultError};
```

- [ ] **Step 7: Confirm the workspace is red for exactly the expected reasons**

Run: `cargo check --workspace --all-features 2>&1 | grep -c "^error"`
Expected: a non-zero count. Every error is a `.mtime` field access or a
two-argument `save` call, in `real_fs.rs`, `mem_fs.rs`, `vault.rs`,
`app/src/vault.rs`, `app/src/main.rs` and the test files. Tasks 2-7 close them.
Do not commit here; the tree does not compile.

---

## Task 2: A Unix timestamp to a civil date

**Files:**
- Create: `crates/vault/src/civil.rs`
- Modify: `crates/vault/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/vault/src/civil.rs` containing only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::format_utc;
    use std::time::{Duration, SystemTime};

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn the_epoch_itself() {
        assert_eq!(format_utc(at(0)), "1970-01-01 00-00-00");
    }

    #[test]
    fn a_leap_day() {
        // 2024-02-29T12:34:56Z
        assert_eq!(format_utc(at(1_709_209_496)), "2024-02-29 12-34-56");
    }

    #[test]
    fn a_century_boundary_that_is_a_leap_year() {
        // 2000-02-29T00:00:00Z. Divisible by 100 and by 400, so it *is* a leap
        // year — the case a three-line leap rule gets wrong by a day.
        assert_eq!(format_utc(at(951_782_400)), "2000-02-29 00-00-00");
    }

    #[test]
    fn a_time_before_the_epoch_clamps() {
        // `SystemTime` before the epoch cannot be expressed as a duration
        // since it. Nothing in a vault should carry one; clamping keeps the
        // name total rather than making the caller handle an error it cannot
        // act on.
        assert_eq!(
            format_utc(SystemTime::UNIX_EPOCH - Duration::from_secs(1)),
            "1970-01-01 00-00-00"
        );
    }

    #[test]
    fn a_recent_timestamp() {
        // 2026-09-21T14:32:05Z
        assert_eq!(format_utc(at(1_789_050_725)), "2026-09-21 14-32-05");
    }
}
```

Add to `crates/vault/src/lib.rs`, beside the other module declarations:

```rust
mod civil;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p inkra-vault --all-features civil`
Expected: FAIL, `cannot find function 'format_utc'`.

- [ ] **Step 3: Write the implementation**

At the top of `crates/vault/src/civil.rs`, above the test module:

```rust
//! A Unix timestamp to a civil date, in UTC.
//!
//! Twenty lines rather than a dependency, per `dependencies.md`: date
//! arithmetic is named there as the write-it-yourself case. Local time is not
//! offered, because rendering it needs the zone database — `libc::localtime_r`
//! and a new direct dependency — to save the reader an offset they can apply
//! themselves.

use std::time::SystemTime;

/// `YYYY-MM-DD HH-MM-SS`, UTC. Safe in a filename: no colon, no slash.
///
/// A time before the epoch clamps to the epoch. No file in a vault carries one,
/// and a total function keeps the caller from handling an error it could not
/// act on anyway.
pub(crate) fn format_utc(at: SystemTime) -> String {
    let seconds = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());

    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02} {:02}-{:02}-{:02}",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60,
    )
}

/// Howard Hinnant's `civil_from_days`, the inverse of the days-from-civil
/// algorithm used by `std::chrono`. Shifts the era so that March is month 1,
/// which is what removes the leap-day special case from the arithmetic: a leap
/// day then falls at the end of a year rather than inside it.
///
/// Correct for the Gregorian rule in full, including that 1900 is not a leap
/// year and 2000 is.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p inkra-vault --all-features civil`
Expected: PASS, 5 tests.

If `a_recent_timestamp` fails, the expected string is wrong, not the algorithm.
Recompute with `date -u -r 1789050725 "+%Y-%m-%d %H-%M-%S"` and correct the
test.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/src/civil.rs crates/vault/src/lib.rs
git commit -m "Render a civil date without a date crate"
```

The workspace still does not compile; this commit is a self-contained new
module and its own tests pass.

---

## Task 3: The conflict file's name

**Files:**
- Create: `crates/vault/src/conflict.rs`
- Modify: `crates/vault/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/vault/src/conflict.rs` containing only its tests:

```rust
#[cfg(test)]
mod tests {
    use super::conflict_path;
    use crate::fs::Mtime;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn at(seconds: u64) -> Mtime {
        Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn it_sits_beside_the_note_and_keeps_the_md_extension() {
        // `read_dir` yields a file only when its extension is `md`
        // (`real_fs.rs:74`). A conflict file the scan filters out would be the
        // only signal this feature has, invisible.
        let path = conflict_path(Path::new("/vault/sub/Note.md"), at(1_789_050_725), |_| false);
        assert_eq!(
            path,
            PathBuf::from("/vault/sub/Note (conflict 2026-09-21 14-32-05Z).md")
        );
    }

    #[test]
    fn a_taken_name_gets_a_counter() {
        let taken = |candidate: &Path| {
            candidate == Path::new("/vault/Note (conflict 1970-01-01 00-00-10Z).md")
        };
        assert_eq!(
            conflict_path(Path::new("/vault/Note.md"), at(10), taken),
            PathBuf::from("/vault/Note (conflict 1970-01-01 00-00-10Z) 2.md")
        );
    }

    #[test]
    fn a_note_without_an_extension_still_gets_one() {
        // `EntryId::DETACHED` — a file named on the command line — never passed
        // the `*.md` filter and may have any name at all.
        let path = conflict_path(Path::new("/tmp/notes"), at(10), |_| false);
        assert_eq!(
            path,
            PathBuf::from("/tmp/notes (conflict 1970-01-01 00-00-10Z).md")
        );
    }
}
```

Add to `crates/vault/src/lib.rs`:

```rust
mod conflict;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p inkra-vault --all-features conflict_path`
Expected: FAIL, `cannot find function 'conflict_path'`.

- [ ] **Step 3: Write the implementation**

At the top of `crates/vault/src/conflict.rs`:

```rust
//! Where the bytes a write would have destroyed go instead.

use std::path::{Path, PathBuf};

use crate::civil::format_utc;
use crate::fs::Mtime;

/// `Note.md` and the mtime of the version being set aside become
/// `Note (conflict 2026-09-21 14-32-05Z).md`, beside the note.
///
/// Named for the *preserved* file's mtime rather than the current time: it
/// names the version in the file, it keeps `vault` free of a clock read, and it
/// makes the name deterministic enough for a test to assert.
///
/// Human-readable, in Dropbox's and Obsidian's shape rather than Syncthing's
/// machine-parseable one. Always `.md`, because `read_dir` lists nothing else
/// (`real_fs.rs:74`) and an unlisted conflict file is an invisible one.
///
/// `taken` decides whether a candidate already exists. A function rather than a
/// direct filesystem call so this stays testable without one; `RealFs` passes
/// `Path::exists`.
pub(crate) fn conflict_path(
    note: &Path,
    preserved: Mtime,
    taken: impl Fn(&Path) -> bool,
) -> PathBuf {
    let stem = note
        .file_stem()
        .unwrap_or_else(|| note.as_os_str())
        .to_string_lossy()
        .into_owned();
    let when = format_utc(preserved.0);

    let mut counter = 1;
    loop {
        let suffix = if counter == 1 {
            String::new()
        } else {
            format!(" {counter}")
        };
        let candidate = note.with_file_name(format!("{stem} (conflict {when}Z){suffix}.md"));
        if !taken(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p inkra-vault --all-features conflict_path`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/src/conflict.rs crates/vault/src/lib.rs
git commit -m "Name the file a conflicted write preserves"
```

---

## Task 4: `RealFs` preserves before it replaces

**Files:**
- Modify: `crates/vault/src/real_fs.rs:39-44`, `:90-140`, `:154-180`
- Test: `crates/vault/tests/real_fs.rs`

- [ ] **Step 1: Write the failing tests**

Append to `crates/vault/tests/real_fs.rs`. It already has helpers; follow the
shape of `save_replaces_the_file_and_reports_the_new_identity` at `:62`.

```rust
#[test]
fn save_with_a_matching_stamp_preserves_nothing() {
    let dir = tempdir().expect("tempdir");
    let note = dir.path().join("Note.md");
    fs::write(&note, "before").expect("write");

    let loaded = block_on(RealFs.load(&note)).expect("load");
    let saved = block_on(RealFs.save(&note, "after", loaded.stamp)).expect("save");

    assert_eq!(saved.preserved, None);
    assert_eq!(fs::read_to_string(&note).expect("read"), "after");
    assert_eq!(fs::read_dir(dir.path()).expect("read_dir").count(), 1);
}

#[test]
fn save_with_a_stale_stamp_preserves_the_bytes_it_would_have_destroyed() {
    let dir = tempdir().expect("tempdir");
    let note = dir.path().join("Note.md");
    fs::write(&note, "before").expect("write");
    let loaded = block_on(RealFs.load(&note)).expect("load");

    // The other writer. Sleeping is what makes the mtime differ on a
    // filesystem whose granularity is coarse; the differing length would catch
    // it anyway, and both terms are the point.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "someone else's much longer text").expect("write");

    let saved = block_on(RealFs.save(&note, "ours", loaded.stamp)).expect("save");

    let preserved = saved.preserved.expect("a conflict file");
    assert_eq!(
        fs::read_to_string(&preserved).expect("read"),
        "someone else's much longer text"
    );
    assert_eq!(fs::read_to_string(&note).expect("read"), "ours");
    assert_eq!(preserved.extension().and_then(|e| e.to_str()), Some("md"));
}

#[test]
fn save_reports_a_stamp_the_next_save_accepts() {
    let dir = tempdir().expect("tempdir");
    let note = dir.path().join("Note.md");
    fs::write(&note, "before").expect("write");

    let loaded = block_on(RealFs.load(&note)).expect("load");
    let first = block_on(RealFs.save(&note, "one", loaded.stamp)).expect("save");
    // The whole self-healing property: after a write, the stamp it returned is
    // the expectation the next write of the same note must pass with.
    let second = block_on(RealFs.save(&note, "two", first.stamp)).expect("save");

    assert_eq!(second.preserved, None);
    assert_eq!(fs::read_to_string(&note).expect("read"), "two");
}

#[test]
fn save_of_a_missing_note_recreates_it_and_preserves_nothing() {
    let dir = tempdir().expect("tempdir");
    let note = dir.path().join("Note.md");
    fs::write(&note, "before").expect("write");
    let loaded = block_on(RealFs.load(&note)).expect("load");
    fs::remove_file(&note).expect("remove");

    let saved = block_on(RealFs.save(&note, "ours", loaded.stamp)).expect("save");

    assert_eq!(saved.preserved, None);
    assert_eq!(fs::read_to_string(&note).expect("read"), "ours");
}

#[test]
fn a_failed_preserve_leaves_the_file_untouched() {
    let dir = tempdir().expect("tempdir");
    let note = dir.path().join("Note.md");
    fs::write(&note, "before").expect("write");
    let loaded = block_on(RealFs.load(&note)).expect("load");

    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&note, "theirs").expect("write");
    // The conflict file cannot be created in a directory with no write
    // permission. The note itself is still writable in place, so only the
    // preserve step can fail — which is the ordering this asserts.
    let mut mode = fs::metadata(dir.path()).expect("metadata").permissions();
    mode.set_mode(0o500);
    fs::set_permissions(dir.path(), mode).expect("chmod");

    let result = block_on(RealFs.save(&note, "ours", loaded.stamp));

    let mut mode = fs::metadata(dir.path()).expect("metadata").permissions();
    mode.set_mode(0o700);
    fs::set_permissions(dir.path(), mode).expect("chmod");

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&note).expect("read"), "theirs");
}
```

`a_failed_preserve_leaves_the_file_untouched` needs
`use std::os::unix::fs::PermissionsExt;` at the top of the file if it is not
already there.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p inkra-vault --all-features --test real_fs`
Expected: FAIL to compile — `save` takes 3 arguments, `loaded.stamp` does not
exist. Task 1 changed the signature; this is the expected shape of red.

- [ ] **Step 3: Update the existing `real_fs` tests to the new signature**

Every existing call in `crates/vault/tests/real_fs.rs` — at `:53`, `:68`,
`:89`, `:103`, `:124`, `:145` — loads first and passes the stamp. For a file
created by the test immediately before, that is:

```rust
    let stamp = block_on(RealFs.load(&note)).expect("load").stamp;
    block_on(RealFs.save(&note, "after", stamp)).expect("save");
```

`save_replaces_the_file_and_reports_the_new_identity` at `:68` also reads
`saved.mtime` if it does; replace with `saved.stamp.mtime`.

- [ ] **Step 4: Implement the stamp and the preserve**

In `crates/vault/src/real_fs.rs`, replace `mtime_of` at `:39-44` with:

```rust
fn stamp_of(path: &Path, metadata: &fs::Metadata) -> Result<Stamp> {
    let mtime = metadata
        .modified()
        .map(Mtime)
        .map_err(|source| io_error(path, source))?;
    Ok(Stamp {
        mtime,
        size: metadata.len(),
    })
}
```

`read_dir` at `:81` still wants a bare `Mtime` for `DirEntry`; give it
`stamp_of(&entry_path, &metadata)?.mtime`.

Replace `load`'s return at `:101-105` with `stamp: stamp_of(path, &metadata)?`.

Replace `save` at `:108-139` with:

```rust
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        let existing = fs::metadata(path).ok();

        // Preserved *before* the replace, never after: between the two steps
        // the bytes exist twice, so no crash point loses them. The reverse
        // order makes a crash destroy the version this exists to protect.
        //
        // A missing file is not a conflict. There are no bytes to preserve, and
        // recreating the note loses nothing.
        let preserved = match existing.as_ref() {
            Some(metadata) if stamp_of(path, metadata)? != expected => {
                Some(preserve(path, metadata)?)
            }
            _ => None,
        };

        // A note with a second hard link is written in place. Replacing it
        // would give this path a new inode and leave the other link on the old
        // one: `Vault::rescan` collapses two names sharing an identity into one
        // entry (`vault.rs:108-110`), so the pair would stop sharing and the
        // second name would surface as a new row carrying stale text — a
        // duplicate appearing because of a write nobody asked for.
        //
        // A symlink is written through for the same reason: the rename would
        // replace the link with a regular file and strand its target holding
        // stale text. `read_dir` keeps symlinks out of a vault, but a file
        // named on the command line — `EntryId::DETACHED` — never passed that
        // filter. `fs::metadata` follows the link, so the `nlink` above is the
        // target's and cannot stand in for this check.
        let in_place = existing
            .as_ref()
            .is_some_and(|metadata| metadata.nlink() > 1)
            || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
        if in_place {
            fs::write(path, text).map_err(|source| io_error(path, source))?;
        } else {
            replace(path, text, existing.as_ref())?;
        }

        let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
        Ok(Saved {
            stamp: stamp_of(path, &metadata)?,
            identity: identity_of(&metadata),
            preserved,
        })
    }
```

And add, beside `replace`:

```rust
/// Copies the bytes currently at `path` to a sibling conflict file, and returns
/// where they went.
///
/// Read and write rather than `fs::copy`, for the `sync_all`: the whole point
/// is that these bytes survive a crash that happens a moment later, during the
/// replace.
fn preserve(path: &Path, metadata: &fs::Metadata) -> Result<PathBuf> {
    let mtime = metadata
        .modified()
        .map(Mtime)
        .map_err(|source| io_error(path, source))?;
    let destination = conflict_path(path, mtime, |candidate| candidate.exists());

    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    let mut file =
        fs::File::create(&destination).map_err(|source| io_error(&destination, source))?;
    file.write_all(&bytes)
        .map_err(|source| io_error(&destination, source))?;
    file.sync_all()
        .map_err(|source| io_error(&destination, source))?;

    Ok(destination)
}
```

Update the imports at `:18-21` to add `Stamp` and `crate::conflict::conflict_path`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p inkra-vault --all-features --test real_fs`
Expected: PASS. The two sleeping tests make this target take about 3 seconds.

- [ ] **Step 6: Commit**

```bash
git add crates/vault/src/real_fs.rs crates/vault/src/fs.rs crates/vault/src/lib.rs crates/vault/tests/real_fs.rs
git commit -m "Preserve the bytes a stale write would have destroyed"
```

---

## Task 5: `MemFs` gets a clock and the same behaviour

**Files:**
- Modify: `crates/vault/src/mem_fs.rs:19-36`, `:69-101`, `:238-299`
- Modify: `crates/vault/src/vault.rs:307`
- Modify: `crates/vault/tests/identity_proptest.rs:122`

- [ ] **Step 1: Write the failing tests**

Append to the `mod tests` block in `crates/vault/src/mem_fs.rs`:

```rust
    #[test]
    fn two_writes_of_one_path_get_different_stamps() {
        // `now` used to be a constant, which made every stamp equal and every
        // conflict undetectable. This is the test that pins the clock.
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "one");
        let first = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;
        fs.insert_file("/vault/a.md", "two");
        let second = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;

        assert_ne!(first, second);
    }

    #[test]
    fn a_stale_stamp_preserves_the_external_text() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let stamp = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;

        // `insert_file` is the external writer: it keeps the path's identity,
        // exactly as a second process rewriting a file in place would.
        fs.insert_file("/vault/a.md", "theirs");

        let saved =
            block_on(fs.save(Path::new("/vault/a.md"), "ours", stamp)).expect("save");

        let preserved = saved.preserved.expect("a conflict file");
        assert_eq!(fs.read_to_string(&preserved).as_deref(), Some("theirs"));
        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("ours")
        );
    }

    #[test]
    fn the_stamp_a_save_returns_is_accepted_by_the_next_save() {
        let fs = MemFs::new();
        fs.insert_file("/vault/a.md", "before");
        let stamp = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;

        let first = block_on(fs.save(Path::new("/vault/a.md"), "one", stamp)).expect("save");
        let second =
            block_on(fs.save(Path::new("/vault/a.md"), "two", first.stamp)).expect("save");

        assert_eq!(second.preserved, None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p inkra-vault --all-features mem_fs`
Expected: FAIL to compile — `save` takes 3 arguments, `.stamp` does not exist.

- [ ] **Step 3: Add the clock**

In `crates/vault/src/mem_fs.rs`, add to `State` at `:19-36`:

```rust
    /// Seconds past the base, advanced by every mutation. A constant here made
    /// every write share one mtime, which made a stale stamp unreachable and
    /// conflict detection untestable.
    clock: u64,
```

Replace `now` at `:75-77` with:

```rust
    /// Advances one second and returns the new time. Takes `&mut State` rather
    /// than nothing, because a clock that does not move is not a clock.
    fn tick(state: &mut State) -> Mtime {
        state.clock += 1;
        Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + state.clock))
    }
```

Every `Self::now()` call site — `:93`, `:100`, `:107`, `:293` — becomes
`Self::tick(&mut state)`. In `insert_file` the borrow checker needs the tick
computed before the `state.files.insert` call that also borrows `state`:

```rust
        let existing = state.files.get(&path).map(|(_, _, id)| *id);
        let identity = existing.unwrap_or_else(|| Self::fresh_identity(&mut state));
        let mtime = Self::tick(&mut state);
        state.files.insert(path, (text.into(), mtime, identity));
```

- [ ] **Step 4: Make `MemFs::save` behave like `RealFs::save`**

Replace `save` at `:271-299`:

```rust
    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        // Parked before the insert, and the guard dropped before the await for
        // the same `Send` reason as `load`. A gated write must not have landed
        // while it is still held: a test that releases the gate is asserting
        // that *this* write is the one reaching the file.
        let blocked = self.state().blocked_saves.remove(path);
        if let Some(wait) = blocked {
            let _ = wait.await;
        }
        // Checked after the gate, so a blocked write still fails where a real
        // one would — at the end, having consumed the time.
        if self.state().unwritable.contains(path) {
            return Err(VaultError::Io {
                path: path.to_path_buf(),
                source: io::Error::from(io::ErrorKind::PermissionDenied),
            });
        }

        // Preserved before the replace, in the order `RealFs` uses and for the
        // same reason. A fake that reversed it would pass while the real one
        // lost data.
        let preserved = {
            let state = self.state();
            match state.files.get(path) {
                Some((existing, mtime, _)) if stamp_of(existing, *mtime) != expected => {
                    let taken = |candidate: &Path| state.files.contains_key(candidate);
                    let destination = crate::conflict::conflict_path(path, *mtime, taken);
                    Some((destination, existing.clone()))
                }
                _ => None,
            }
        };
        if let Some((destination, bytes)) = preserved.clone() {
            self.insert_file(destination, bytes);
        }

        self.insert_file(path.to_path_buf(), text);

        let mut state = self.state();
        let identity = Self::fresh_identity(&mut state);
        let mtime = Self::tick(&mut state);
        if let Some((stored_text, stored_mtime, stored_identity)) = state.files.get_mut(path) {
            *stored_mtime = mtime;
            *stored_identity = identity;
            let stamp = stamp_of(stored_text, mtime);
            return Ok(Saved {
                stamp,
                identity,
                preserved: preserved.map(|(destination, _)| destination),
            });
        }
        Err(VaultError::Io {
            path: path.to_path_buf(),
            source: io::Error::from(io::ErrorKind::NotFound),
        })
    }
```

And a free helper beside `Gate`:

```rust
/// The stamp a stored file would report. `size` is the byte length of the text
/// as stored, which is what `RealFs` reports for the same content.
fn stamp_of(text: &str, mtime: Mtime) -> Stamp {
    Stamp {
        mtime,
        size: text.len() as u64,
    }
}
```

`load` at `:255-259` returns `stamp: stamp_of(text, *mtime)`.

- [ ] **Step 5: Fix the two remaining `vault` callers**

`crates/vault/src/vault.rs:307` and `crates/vault/tests/identity_proptest.rs:122`
both call `fs.save(path, "…")`. Each loads first:

```rust
        let stamp = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;
        let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
```

In the proptest at `:122` the path is `absolute`; use it in both calls.

- [ ] **Step 6: Run the crate's whole suite**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS. `inkra-vault` is now fully green; `inkra-app` still is not.

- [ ] **Step 7: Commit**

```bash
git add crates/vault
git commit -m "Give MemFs a clock, so a stale stamp is reachable"
```

---

## Task 6: The `Writer` holds the expectation

**Files:**
- Modify: `crates/app/src/vault.rs:59-100`, `:272-300`, `:203-239`, `:442-501`
- Modify: `crates/app/src/main.rs:60-70`, and `adopt` at `crates/app/src/vault.rs:262-265`

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block in `crates/app/src/vault.rs`, following
`typing_writes_the_note_without_cmd_s` at `:696-711`, which is the nearest
existing test in shape: `open_shell` for the window, `simulate_input` for the
keystroke, `advance_clock(AUTOSAVE_DELAY)` for the cadence.

```rust
    /// The defect the expectation's placement exists to avoid, in its simplest
    /// reachable form. The second write's expectation cannot be the stamp the
    /// *load* returned: our own first write already invalidated it. A note that
    /// took its expectation from `OpenNote` would mint a conflict file holding
    /// its own text on the second keystroke of an ordinary session.
    #[gpui::test]
    fn a_second_write_does_not_conflict_with_the_first(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();
        visual.simulate_input("Y");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeXY")
        );
        let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault")))
            .expect("read_dir");
        assert_eq!(
            entries.len(),
            1,
            "a write must not conflict with our own previous one"
        );
    }

    /// The same, with the two writes genuinely overlapping rather than
    /// sequential: `block_save` holds the first open past the turn that issued
    /// it, which is the only way a second write is issued while the first is
    /// still in flight.
    #[gpui::test]
    fn an_overlapping_write_does_not_conflict_with_the_one_in_flight(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");

        let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);
        let gate = fs.block_save("/vault/a.md");
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        visual.simulate_input("Y");
        notes.update(cx, |notes, cx| notes.save_current(cx));
        gate.release();
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault")))
            .expect("read_dir");
        assert_eq!(entries.len(), 1, "neither write may conflict with the other");
    }

    /// An external writer changes the note between the load and the write. Its
    /// text survives, under a new name, and the note holds what is on screen.
    #[gpui::test]
    fn an_external_change_is_preserved_beside_the_note(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        // `insert_file` keeps the path's identity, exactly as a second process
        // rewriting a file in place would.
        fs.insert_file("/vault/a.md", "theirs");

        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX")
        );
        let entries = futures::executor::block_on(fs.read_dir(Path::new("/vault")))
            .expect("read_dir");
        let preserved = entries
            .iter()
            .find(|entry| entry.path.to_string_lossy().contains("conflict"))
            .expect("a conflict file");
        assert_eq!(fs.read_to_string(&preserved.path).as_deref(), Some("theirs"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p inkra-app --all-features a_second_write_does_not_conflict`
Expected: FAIL to compile — `inkra-app` does not build yet, because `save` takes
three arguments.

- [ ] **Step 3: Move the expectation into the `Writer`**

In `crates/app/src/vault.rs`, replace `Writer` at `:59-67`. Keep the whole
existing doc comment and append the new paragraph:

```rust
struct Writer {
    /// The newest write that reached the filesystem, and what it left behind.
    ///
    /// The stamp lives here and not on `OpenNote` because a write captures its
    /// text under `&mut App` and only then waits for this lock. A second write
    /// issued while the first is in flight would therefore capture a stamp the
    /// first write is about to invalidate, and preserve *our own* newly written
    /// text into a conflict file. Read after the lock, the expectation is
    /// always what our own last write actually produced.
    state: futures::lock::Mutex<WriterState>,
}

struct WriterState {
    /// The sequence of the newest write that reached the filesystem.
    written: u64,
    /// What the file looked like after that write — or what the load returned,
    /// before there was one.
    stamp: Stamp,
}
```

- [ ] **Step 4: Use it in `write_through`**

Replace `write_through` at `:78-100`:

```rust
async fn write_through(
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
```

- [ ] **Step 5: Seed it at `track`**

`track` at `:272-300` takes a `stamp: Stamp` parameter and builds:

```rust
                writer: Arc::new(Writer {
                    state: futures::lock::Mutex::new(WriterState {
                        // Zero is below every minted sequence, which starts at
                        // one, so the first write of a note is never mistaken
                        // for a stale one.
                        written: 0,
                        stamp,
                    }),
                }),
```

`open_note` at `:233` passes `loaded.stamp`. It currently moves `loaded.text`
into `Buffer::new`; capture the stamp before that move.

`adopt` at `:262-265` takes a `stamp: Stamp` and forwards it.

- [ ] **Step 6: Carry the launch file's stamp**

In `crates/app/src/main.rs:60-70`, `Launch::Note` carries the stamp:

```rust
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| Launch::Note(path, loaded.text, loaded.stamp))
```

and the `adopt` call at `:102` forwards it. Update the `Launch` enum and its
match arms.

- [ ] **Step 7: Run the workspace build**

Run: `cargo check --workspace --all-features`
Expected: clean. If `Saved` is moved twice in `NoteStore::write`, clone the
`preserved` path before reading `saved.identity`.

- [ ] **Step 8: Run the two new tests**

Run: `cargo test -p inkra-app --all-features conflict`
Expected: PASS, both.

- [ ] **Step 9: Commit**

```bash
git add crates/app crates/vault
git commit -m "Take the write's expectation from the writer, not the note"
```

---

## Task 7: Split the scan so an entity can drive it

`Vault::rescan` is `pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()>`
(`crates/vault/src/vault.rs:105`). It holds `&mut self` across an `await`, so an
`Entity<Vault>` cannot call it: `Entity::update` hands out `&mut Vault` only for
the body of a synchronous closure. Task 8 needs to rescan from `app`, so the
walk and the identity bookkeeping separate first.

This is a refactor with no behaviour change. Its proof is that the existing
`vault` suite stays green without being edited.

**Files:**
- Modify: `crates/vault/src/vault.rs:88-96`, `:105-140`

- [ ] **Step 1: Confirm the current suite is green, so the refactor has a baseline**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS. Note the test count; Step 4 must match it.

- [ ] **Step 2: Split the method**

Replace `rescan` with a pair, and keep `rescan` as their composition so no
existing caller changes:

```rust
    /// Everything on disk under the root, flattened, depth-limited.
    ///
    /// Separate from `apply` because it awaits, and `apply` needs `&mut self`.
    /// An `Entity<Vault>` can hold `&mut Vault` only inside a synchronous
    /// closure, so a caller that owns the entity runs this half first and hands
    /// the result to the other.
    ///
    /// # Errors
    ///
    /// Never, today: an unreadable directory is skipped rather than reported,
    /// so one folder cannot lose the vault. The `Result` is the seam a second
    /// `Fs` backend will need.
    pub async fn collect(fs: &dyn Fs, root: &Path) -> Result<Vec<DirEntry>> {
        let mut found = Vec::new();
        let mut worklist = vec![(root.to_path_buf(), 0usize)];

        while let Some((dir, depth)) = worklist.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Ok(children) = fs.read_dir(&dir).await else {
                continue;
            };
            for child in children {
                if child.kind == Kind::Dir {
                    worklist.push((child.path.clone(), depth + 1));
                }
                found.push(child);
            }
        }
        Ok(found)
    }

    /// Mints and re-uses ids for what `collect` found. Ids are keyed by
    /// `(device, inode)`, so a rename keeps its id and a recreated file takes a
    /// new one.
    pub fn apply(&mut self, children: Vec<DirEntry>) {
        // The body of the old `rescan`, from `let mut found` onwards, with the
        // walk removed: the loop over `children` replaces the inner `for child
        // in children` and the worklist is gone.
    }

    /// # Errors
    ///
    /// Never, today. See `collect`.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let children = Self::collect(fs, &self.root).await?;
        self.apply(children);
        Ok(())
    }
```

Move the existing body of `rescan` — the `next_by_identity` map, the
`strip_prefix`, the `RelPath`, the hard-link collapse, the id minting and the
sort — into `apply` unchanged. The only edit to that body is that its outer
`while let Some((dir, depth))` walk is gone and it iterates `children` directly.
Read `crates/vault/src/vault.rs:105-140` and the lines after it before writing:
the whole point of this task is that nothing in that logic changes.

`Kind` must be in scope in `vault.rs`; it is already imported for `Entry`.

- [ ] **Step 3: Expose the root**

`app` needs it to call `collect`. Beside `absolute`:

```rust
    /// Where the vault starts. `app` needs it to drive `collect` itself.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
```

- [ ] **Step 4: Run the suite, unedited**

Run: `cargo test -p inkra-vault --all-features`
Expected: PASS, the same count as Step 1. If a test needed editing, the refactor
changed behaviour and is wrong.

- [ ] **Step 5: Commit**

```bash
git add crates/vault/src/vault.rs
git commit -m "Split the vault scan from the identity bookkeeping"
```

---

## Task 8: A conflict rescans and is announced

**Files:**
- Modify: `crates/app/src/vault.rs:30-35`, `:483-501`

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block in `crates/app/src/vault.rs`:

```rust
    /// The conflict file's only route to the user. Nothing else in the running
    /// application rescans (`vault.rs:987-991`), so without this the file sits
    /// on disk unmentioned until the next launch.
    #[gpui::test]
    fn a_conflict_reaches_the_vault_tree(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let (mut visual, notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        let before = notes.read_with(cx, |notes, cx| {
            notes.vault().expect("a vault").read(cx).entries().count()
        });

        fs.insert_file("/vault/a.md", "theirs");
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        let after = notes.read_with(cx, |notes, cx| {
            notes.vault().expect("a vault").read(cx).entries().count()
        });
        assert_eq!(after, before + 1, "the preserved file is listed");
    }

    /// An ordinary write must not walk the directory. A conflict is rare;
    /// `AUTOSAVE_DELAY` is not.
    #[gpui::test]
    fn an_ordinary_write_does_not_rescan(cx: &mut TestAppContext) {
        let fs = Arc::new(MemFs::new());
        fs.insert_file("/vault/a.md", "before");
        let (mut visual, _notes) = open_shell(cx, Arc::clone(&fs) as Arc<dyn Fs>);

        // A rescan of an unreadable directory yields nothing and would empty
        // the tree. The write must not trigger one.
        fs.make_unreadable("/vault");
        visual.simulate_input("X");
        cx.executor().advance_clock(AUTOSAVE_DELAY);
        cx.run_until_parked();

        assert_eq!(
            fs.read_to_string(Path::new("/vault/a.md")).as_deref(),
            Some("beforeX")
        );
    }
```

`Vault::entries` returns an iterator, so it is `.count()`, not `.len()` — see
its use in `open_two_notes` at `crates/app/src/vault.rs:530-536`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p inkra-app --all-features a_conflict_reaches_the_sidebar`
Expected: FAIL — `after` equals `before`, because nothing rescanned.

- [ ] **Step 3: Add the event**

In `crates/app/src/vault.rs`, beside `SaveFailed` at `:30-35`:

```rust
/// A write found the file changed underneath it and preserved the other
/// version beside the note. Emitted for the same reason as `SaveFailed`: the
/// deferred status line gets one seam to subscribe to rather than two that grew
/// apart.
#[derive(Debug, Clone)]
pub(crate) struct ConflictPreserved {
    pub(crate) id: EntryId,
    pub(crate) path: PathBuf,
}

impl EventEmitter<ConflictPreserved> for NoteStore {}
```

- [ ] **Step 4: Rescan and emit on conflict**

In `NoteStore::write` at `:483-501`, inside the success arm, after the existing
`reidentify` call:

```rust
            if let Some(preserved) = saved.preserved.clone() {
                cx.emit(ConflictPreserved { id, path: preserved });
                // The only production caller of the scan. A conflict file is a
                // `.md` file in the vault and the tree is the one surface this
                // feature has; nothing else in the running application would
                // ever list it (`vault.rs:987-991`). Kept off the ordinary path
                // deliberately: a full directory walk every `AUTOSAVE_DELAY`
                // would not be free, and a conflict is rare.
                //
                // Two halves, because `Vault::apply` needs `&mut Vault` and the
                // walk awaits: an entity hands out `&mut` only inside a
                // synchronous closure. Task 7 split them for this.
                if let Some(vault) = this.vault.clone() {
                    let fs = Arc::clone(&this.fs);
                    cx.spawn(async move |_, cx| {
                        let root = vault.read_with(cx, |vault, _| vault.root().to_path_buf());
                        let Ok(root) = root else { return };
                        let Ok(children) = Vault::collect(&*fs, &root).await else {
                            return;
                        };
                        let _ = vault.update(cx, |vault, cx| {
                            vault.apply(children);
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
```

`Vault::collect` and `Vault::apply` are the pair Task 7 created. The `cx.notify`
is what repaints the sidebar; without it the entry exists and no frame shows it.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p inkra-app --all-features`
Expected: PASS, the whole crate.

- [ ] **Step 6: Commit**

```bash
git add crates/app
git commit -m "Show a preserved conflict in the vault tree"
```

---

## Task 9: Documentation

**Files:**
- Modify: `CLAUDE.md`
- Modify: `docs/roadmap.md:88-98`
- Modify: `docs/superpowers/specs/2026-09-20-inkra-vault-design.md:650-651`

- [ ] **Step 1: Close the Deferred rows this plan closes**

In `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`, the "Conflict
detection" row at `:651` is closed. Its note said conflict detection "lands with
the watcher"; that was wrong, and the spec written for this plan says why.
"File watcher + diff reload" at `:650` stays open, and its note gains that
detection no longer waits on it.

- [ ] **Step 2: Update the roadmap**

`docs/roadmap.md:95-98` calls conflict detection "the largest honest hole". It
is no longer a hole; what remains is reloading a *clean* note changed on disk,
which needs the watcher. Rewrite the paragraph to say that, and do not repeat
the Deferred table.

- [ ] **Step 3: Add to the don't-recreate table**

In `CLAUDE.md`, add rows for `Stamp`, `conflict_path`, `format_utc`,
`Saved::preserved`, `ConflictPreserved`, and `MemFs`'s clock. Add a Code rule:
a stamp is compared with `!=`, never with an ordering, and never includes the
identity.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs
git commit -m "Record what conflict detection closed and what it did not"
```

---

## Task 10: Verify

- [ ] **Step 1: Run the full check**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`, exit 0.

- [ ] **Step 2: Run the application**

`check.sh` renders nothing, and this plan touches the sidebar's contents.

```bash
mkdir -p /tmp/inkra-conflict && printf 'before\n' > /tmp/inkra-conflict/Note.md
cargo run -p inkra-app -- /tmp/inkra-conflict/Note.md
```

Type a character. While the window is open, in another terminal:
`printf 'theirs\n' > /tmp/inkra-conflict/Note.md`. Type another character and
wait two seconds. Expected: `Note.md` holds what is on screen, and
`Note (conflict …Z).md` holds `theirs`. Confirm with `ls /tmp/inkra-conflict`.

Note this exercises the `EntryId::DETACHED` path, which has no vault and so no
rescan — the file appears on disk but in no sidebar. Run it a second time with
`cargo run -p inkra-app -- /tmp/inkra-conflict` to see the sidebar row appear.

- [ ] **Step 3: Paste the evidence**

`./scripts/check.sh` tail, its exit code, and `git diff --stat main...HEAD`.

---

## Known gaps this plan does not close

- **The TOCTOU window between the stat and the rename.** No mainstream
  filesystem offers a compare-and-swap replace. Recorded in the spec with the
  primitives that were considered and why each fails.
- **Same mtime granule, same size.** Two writes inside one timestamp granule
  that happen to produce equal lengths compare equal and the conflict is missed.
- **A clean note changed on disk is never reloaded.** It never reaches a write,
  so it never reaches the stat. Needs the watcher, and acting on it needs
  anchored selections.
- **`EntryId::DETACHED` conflicts are invisible.** A file opened by path has no
  vault, so the rescan in Task 7 does nothing and the conflict file appears in
  no sidebar. The event still fires; nothing consumes it yet.
- **Nothing consumes `ConflictPreserved`.** It exists for the deferred status
  line. An emitted event with no subscriber is dead weight until then, and is
  justified only by `SaveFailed` having set that precedent already.
