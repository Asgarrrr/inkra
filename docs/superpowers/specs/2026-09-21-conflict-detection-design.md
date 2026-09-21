# Conflict detection: never clobber a note another writer changed

Status: **designed, not implemented.**

Every `file:line` under `vendor/zed-reference` below was read in that checkout,
at pinned commit `526c95d`. Every `file:line` without a prefix is this repo.

## Why now

Autosave shipped on 2026-09-21. A dirty note is written every
`AUTOSAVE_DELAY` — two seconds (`crates/app/src/vault.rs:27`) — with no prompt
and no check. Nothing compares the file we are about to replace against the
file we last read.

So a sync client, a `git checkout`, or a second editor can change a note, and
the next tick overwrites it. The edit is gone with no record and no message.

`⌘S`-only needed the user to press a key at the wrong moment. Autosave made the
window continuous. This is the largest hole `docs/roadmap.md:95-98` records, and
the one item in the vault design's Deferred table whose cost rose when autosave
landed.

## What the file already offers

`Loaded` and `Saved` both carry an `Mtime` (`crates/vault/src/fs.rs:43`, `:53`).
Neither is read by anything. They were added for this.

`RealFs::save` already stats the destination before writing, to decide whether
the note is a hard link or a symlink that must be written in place
(`crates/vault/src/real_fs.rs:109`). The stat this design needs is that stat.

## The decision, and the evidence

### Zed refuses; it does not prompt an unattended writer

`Buffer` keeps `saved_mtime` — "mtime of the file when this buffer was last
loaded from or saved to disk" (`vendor/zed-reference/crates/language/src/buffer.rs:108-113`).
`has_conflict()` is `mtime.bad_is_greater_than(saved_mtime) && has_unsaved_edits()`
(`:2531-2549`, the comparison at `:2542`).

The split that matters is at the workspace layer. An explicit save on a
conflicted buffer prompts `Overwrite / Discard Edits / Cancel`
(`vendor/zed-reference/crates/workspace/src/pane.rs:2253`, `:2357-2366`).
Autosave does not: `can_autosave` requires `!has_conflict`
(`vendor/zed-reference/crates/workspace/src/item.rs:586-589`), and
`Pane::autosave_item` is a silent no-op when it is false (`pane.rs:2539-2563`).

The prompt is reserved for the save a human asked for, because a human is there
to answer. We have no such save: every one of our writes is unattended.

### Three things Zed does that we will not copy

**Ordering, not equality.** `MTime` deliberately withholds `Ord`
(`vendor/zed-reference/crates/fs/src/fs.rs:329-337`), citing apenwarr's "mtime
comparison considered harmful"; the escape hatch the conflict check uses is
named `bad_is_greater_than` (`:422-425`). Zed's own issue tracker records the
switch from `>` to `!=` elsewhere for the same reason
([zed#21034](https://github.com/zed-industries/zed/issues/21034)). Dropbox's
desktop client preserves a file's original modified date
([help.dropbox.com](https://help.dropbox.com/organize/modified-date)), so an
edit synced down from another machine can carry an mtime *older* than ours.
`>` calls that unchanged and overwrites it.

**No size term.** `DiskState::Present` carries `size`
(`vendor/zed-reference/crates/language/src/buffer.rs:406-416`) and the conflict
check ignores it. Mtime granularity is 1 s on HFS+, 2 s on FAT, and coarser on
some network mounts; APFS is nanosecond-granular but nothing says a vault sits
on APFS. VS Code folds size into its etag for exactly this reason, with the
comment that "relying only on the mtime check has prooven to produce false
positives due to file system weirdness (especially around remote file systems)"
([fileService.ts](https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/files/common/fileService.ts)).

**No stat before the write.** `save_local_buffer` reads the buffer's cached
`File` and writes (`vendor/zed-reference/crates/project/src/buffer_store.rs:388-448`);
the only stat is *after* the bytes land, to collect the new mtime
(`vendor/zed-reference/crates/worktree/src/worktree.rs:1883-1894`). The
conflict flag is therefore only as fresh as the last filesystem watcher event,
and a missed event is a silent overwrite — filed as a data-loss bug, whose
proposed fix is the stat-before-write this design adopts
([zed#63174](https://github.com/zed-industries/zed/issues/63174)).

### The watcher is not a prerequisite

Our own Deferred table says conflict detection "lands with the watcher". That
describes Zed's implementation, not a constraint. Zed's stat-backed refresh path
is independent of the watcher and keeps working with a `NullWatcher`
(`vendor/zed-reference/crates/worktree/src/worktree.rs:1364-1371`, `:2119-2128`);
Zed simply does not invoke it before writing.

The reverse is the true statement: a watcher does not remove the need for the
save-time stat. Watchers drop events under queue overflow, on poll backends and
on remote filesystems, which is the whole content of zed#63174. Polling on save
is the load-bearing check, not a substitute for one.

### What the ecosystem converged on

| Option | Reversible | Field record |
|---|---|---|
| Write anyway | No | Logseq's effective behaviour, closed "not a bug", unrecoverable loss reported ([#8074](https://github.com/logseq/logseq/discussions/8074)) |
| Auto-merge into the dirty buffer | No — fabricates a version neither side wrote | Obsidian's default. The loudest data-loss reports found: notes emptied under the "merged automatically" toast ([forum 87814](https://forum.obsidian.md/t/notes-self-deleting-aka-modified-externally-merged-automatically-pop-up/87814)) |
| Refuse, go silent | Yes, until quit | Zed and VS Code's autosave branch. Both have an explicit save that eventually surfaces it. We do not |
| Preserve the other version beside the note | Yes | Dropbox, Syncthing, iCloud, and Obsidian 1.9.7's opt-in "Create conflict file" ([obsidian.md/help](https://obsidian.md/help/sync/troubleshoot)) |

We take the last one.

## The mechanism

One rule: **no write may destroy bytes it did not put there.**

`Fs::save` gains an expectation. The caller passes the `Stamp` it believes the
file carries — required, not optional. An `Option` would read as "no
expectation, write anyway", which is the clobber this design exists to remove,
and nothing needs it: the vault cannot create a note yet, so every write is a
write of a file we loaded. The implementation stats the destination immediately
before replacing it:

- The stamp matches, or the file does not exist — replace it, as today. A note
  deleted underneath us is recreated rather than resurrected into a conflict
  file: there are no bytes to preserve. Zed prompts separately for that case
  (`vendor/zed-reference/crates/workspace/src/pane.rs:2255`); we have no prompt,
  and recreating loses nothing.
- The stamp differs — copy the bytes currently on disk to a sibling conflict
  file, fsync it, *then* replace. The note keeps its path; the other writer's
  version keeps its content under a new name.
- The copy fails — replace nothing, return the error. The file on disk is left
  exactly as it was found.

### Why the buffer keeps the path

The alternative is to send the buffer's text to the conflict file and leave the
note holding the other writer's version. It preserves the same bytes, and it
was the first choice this design made. It is wrong for a structural reason.

After it, the note on screen is no longer connected to its file. Every later
keystroke lives only in memory, a latch is needed so the next tick does not mint
a second conflict file two seconds later — the "endless conflict files" failure
Syncthing documents ([docs.syncthing.net](https://docs.syncthing.net/users/syncing.html))
— and the quit flush faces the same conflict again with nowhere to put it. The
note stays in a degraded state until the user intervenes.

Writing the buffer to its own path ends the conflict in one step. The file and
the buffer agree again, the write returns a fresh stamp, and autosave resumes on
its normal cadence. No latch, no loop, no degraded state. This is Dropbox's
model: the conflicted copy is made, and the live document keeps its name.

### Ordering, which is the whole safety argument

Preserve first, replace second. Between the two steps the bytes exist twice, so
no crash point loses them. Reversing the order makes a crash between them
destroy the version we set out to protect.

### What is compared

`Stamp { mtime: Mtime, size: u64 }`, compared with `!=`.

Not `(device, inode)`: our own save renames a temp file over the destination
(`crates/vault/src/real_fs.rs:154-180`), so the identity moves on every write we
make ourselves. Comparing it would report every one of our own saves as a
conflict.

Not a content hash. It reads the whole file on every tick to catch the case
mtime and size both miss — same granule, same length — and that case is rare
enough not to pay for it on every write of every note.

## Where each piece lives

| Piece | Crate | Why |
|---|---|---|
| `Stamp`, the expectation parameter, the preserve-then-replace sequence, the conflict file's name | `vault` | It is file I/O. `app` opens no file itself |
| Remembering the stamp per open note, passing it, reacting to what came back | `app` | It is the binding between a file and a `Buffer`, which is `NoteStore` |

`app` never stats. It holds the stamp the last load or save returned and hands
it back on the next write. That keeps the whole comparison inside one `Fs` call,
which is also what keeps the window between the stat and the rename as narrow as
the implementation can make it.

### The stamp lives in the `Writer`, not in `OpenNote`

The obvious home is a field beside `saved_version` on `OpenNote`
(`crates/app/src/vault.rs:102-128`). It produces a false conflict on ordinary
typing, and the reason is the one `Writer` already exists for.

Two writes of one note can be in flight at once: the periodic chain's, and the
quit flush's, which cannot chain behind it. Write B captures its expectation
under `&mut App`, then waits on the writer's lock while write A lands. A changed
the file, so B's captured stamp is now stale — and B would preserve *our own*
newly written text to a conflict file and report a conflict to the user. Typing
would mint junk files.

So the expectation is not "what the note looked like when this write was
issued". It is **"what the file looked like after our own last write, or what we
loaded if we have not written it yet"**. That value already has exactly one home
with exactly the right lifetime: the `Writer`, which holds the sequence of the
newest write that reached the filesystem (`crates/app/src/vault.rs:59-67`). It
holds the stamp beside it, under the same lock, updated in the same place. A
write reads its expectation after acquiring the lock, never before.

`OpenNote` therefore gains no field. `track` seeds the `Writer` with the stamp
the load returned.

## What `app` does with the answer

`Saved` reports whether a conflict file was written. `NoteStore::write` already
re-keys the vault entry and advances `saved_version` on success
(`crates/app/src/vault.rs:483-500`); it additionally stores the new stamp, and
emits an event when a conflict file was created.

There is no status bar and no banner — "a home for user-visible errors" is
Deferred in `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`, and this
design does not build one. What the user sees instead is the conflict file
itself: it is a `.md` file in the vault, so a scan lists it and the sidebar
shows it as a row.

**That requires a rescan, and nothing in the running application performs one.**
`Vault::rescan` has no caller outside the test suite today — the point
`crates/app/src/vault.rs:987-991` already records. So the write that creates a
conflict file also asks for a rescan, and this becomes that method's first
production caller. Without it the file would sit on disk unmentioned until the
next launch, and the only signal this design has would not fire.

The rescan runs on conflict alone, never on an ordinary write. A conflict is
rare; a full directory walk every two seconds would not be.

`SaveFailed` already exists for the failure case (`crates/app/src/vault.rs:32-35`)
and is the precedent for emitting an event rather than printing. The conflict
event follows it, so that the deferred status line has one seam to subscribe to
rather than two that grew apart.

### The conflict file's name

`Note (conflict 2026-09-21 14-32-05Z).md`.

Human-readable, in Dropbox's and Obsidian's shape rather than Syncthing's
machine-parseable one. The `.md` extension is deliberate and load-bearing: a
name `read_dir` filters out (`crates/vault/src/real_fs.rs:74`) would hide the
only signal this design has.

**The timestamp is the preserved file's own mtime, not the current time.** It
names the version being set aside, which is the more useful of the two, and it
keeps CLAUDE.md's rule intact: `vault` takes every time value from the
filesystem and never from `SystemTime::now`. It also makes the name
deterministic, so a test can assert it rather than glob for it.

UTC, with the `Z` stated. Rendering local time needs the zone database, which
means `libc::localtime_r` and a new direct dependency, to save the reader an
offset they can apply themselves. The conversion from a Unix timestamp to a
civil date is the standard days-from-civil algorithm, about twenty lines, which
`dependencies.md` puts firmly on the write-it-yourself side.

A collision — two conflicts preserving versions that share one second — appends
` 2`, ` 3`, and so on.

## Testing

The stamp check is filesystem behaviour, so it is proven in `vault` with
`#[test]`, and its consequences for an open note are proven in `app` with
`#[gpui::test]` against `MemFs`.

`MemFs` blocks both today. `MemFs::now()` returns a constant
(`crates/vault/src/mem_fs.rs:75-76`), so every write shares one mtime and no
stamp can ever differ. It needs a clock that advances per write, and a way for a
test to write a file *behind* the store's back — the external writer this whole
design exists to detect. Both belong beside `block_load`, `block_save` and
`make_unwritable`, which are the existing seams for making a race reproducible.

`vault/tests/real_fs.rs` keeps its role: only the real filesystem can prove that
a real stat, a real copy and a real rename compose the way the fake says they do.

## What this deliberately leaves out

| Item | Trigger | Why not now |
|---|---|---|
| The TOCTOU window between the stat and the rename | Nothing closes it | No mainstream filesystem offers a compare-and-swap replace. macOS `renamex_np` gives `RENAME_EXCL`, which fails whenever the destination exists, and `RENAME_SWAP`, which swaps unconditionally ([rename(2)](https://keith.github.io/xcode-man-pages/rename.2.html)). `flock` is advisory and no sync client takes it. Neither VS Code nor Zed closes it. The conflict file bounds the cost of losing that race to one duplicate file instead of one lost edit |
| Same granule, same size | A user reports a missed conflict | Needs a content comparison on every write. Mtime plus size covers the cases with a field record |
| Reloading a clean note changed on disk | The watcher lands | A clean buffer never reaches a write, so it never reaches the stat. Detecting it needs the watcher, and acting on it needs diff-based reload, which needs anchored selections |
| Merging the two versions | A user asks | The option with the worst field record in the survey above. A conflict file is a thing the user can diff with any tool they already have |
| Telling the user in the window | The first user who is not us | Rides on "a home for user-visible errors", already Deferred |
| Cleaning up conflict files | A vault accumulates enough to annoy | They are ordinary notes. Nothing else in the vault is swept either |

## Rejected alternatives

**Check in `app`, before calling `save`.** Rejected twice over: `app` opening a
file breaks the layering that lets every `app` test run against `MemFs`, and a
stat in `app` followed by a write in `vault` widens the window between them by a
task hop rather than narrowing it.

**A `has_conflict` flag on the note, checked by the scheduler.** This is Zed's
shape, and it needs a watcher to keep the flag fresh. Without one the flag would
only ever be set by the write that already happened, which is the check itself.
The flag would be a cache with no invalidator.

**Refuse the write and latch the note, with no conflict file.** Preserves the
disk version and loses nothing immediately, and it is what Zed and VS Code do.
Rejected because both have an explicit, prompting save to escape the latch, and
we have neither a prompt nor any other surface. The latch would never lift.

**A separate, cheaper path for the quit flush.** Considered because
`SHUTDOWN_TIMEOUT` is 200 ms and the preserve-then-replace sequence costs a read
and a write more than a plain replace. Rejected: the budget is already
best-effort and already documented as unreliable
(`docs/superpowers/plans/2026-09-21-autosave-quit-flush.md`), and a second write
policy would double the cases every test has to cover to buy a saving nobody has
measured.
