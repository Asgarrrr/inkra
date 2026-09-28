# File watcher: the vault follows the disk while the app runs

Status: **done.** W1 (W1a, W1b) and W2 (W2a, W2b, W2c); W2c and its final review on 2026-09-28.

The user's priority (roadmap, "Index and search: the 2026-09-27 audit", item
2): a note changed outside the app while it runs shows in the app. Today the
tree, the index and an open note only learn of it at the next create, rename
or trash, and a clean open note keeps stale text until it is reopened
(`docs/roadmap.md`, Vault).

Prerequisite, shipped: `2026-09-27-coalesced-reconcile-design.md`. A stream of
scan swaps no longer starves the index, and a swap that moves only mtimes
reloads only those notes.

## What the user decided (2026-09-27)

1. **A clean open note reloads silently**, keeping the caret and the scroll.
   A dirty one keeps the user's edits; the external version is preserved
   beside the note by the existing conflict path.
2. **A note deleted or moved out of the vault stays open**, marked as gone
   from disk. Typing in it lets autosave recreate the file at its path.
   Closing it untouched lets it go.
3. **A reload shows nothing.** No toast, no marker.

## What Zed does

Paths are under `vendor/zed-reference/crates/`.

- **Watch.** `Fs::watch(path, latency)` returns a stream of `Vec<PathEvent>`
  and a `Watcher` handle (`fs/src/fs.rs:171-178`). `PathEventKind` is
  `Removed | Created | Changed | Rescan` (`fs.rs:80-91`). `RealFs` wraps
  `notify`'s `RecommendedWatcher`, FSEvents on macOS, recursive
  (`fs/src/fs_watcher.rs:28-33`, `:1143-1166`). Events are merged by path and
  handed over every `latency`, 100 ms for a worktree
  (`fs_watcher.rs:86-101`, `worktree/src/worktree.rs:83`); the fake uses zero.
- **Dropped events.** `notify`'s `Flag::Rescan` (FSEvents'
  `MustScanSubDirs`) becomes a `Rescan` event on the named path, or the root;
  a rescan swallows the events under it (`fs_watcher.rs:679-760`).
- **Applying a batch.** `process_events` sorts the paths, drops a child under
  a parent, stats each path, removes what is gone first so renames pair in
  any order, and scans the folders that appeared
  (`worktree.rs:4824-5207`, `:5721-5765`). Renames reuse ids by inode
  (`:3302-3325`).
- **Its own writes** are not filtered. The save records the new mtime; the
  echo event builds an identical `File` and changes nothing
  (`project/src/buffer_store.rs:564-592`).
- **Reload.** A buffer whose disk state changed and has no unsaved edits emits
  `ReloadNeeded` (`language/src/buffer.rs:1778-1810`). The reload diffs old
  and new text on a background task and applies the hunks as one ordinary
  transaction, so anchors and undo survive; if the buffer changed meanwhile,
  it is marked conflicted instead (`buffer.rs:1668-1757`, `:2353-2491`).
- **Deleted.** The buffer keeps its text under `DiskState::Deleted` and its
  old path (`buffer_store.rs:579-588`).

What we take and where we deviate is stated in each decision.

## Slices

| Slice | Delivers | The user sees |
|---|---|---|
| W1 | `Fs::watch`, the `notify` backend, a `MemFs` event feed, 100 ms batches, scoped rescans, `Rescan` falling back to a full scan | The tree, quick open, search and backlinks follow edits made outside the app |
| W2a | `inkra_text::diff` and `Buffer::apply_diff` (F8, the diff) | Nothing yet |
| W2b | The reload: F7's detection, F8's reload, `Diff::map`, the caret and the scroll kept | A clean open note follows the disk |
| W2c | The gone-from-disk state (F9) | A note deleted on disk stays open |

W1 comes first: without scoped rescans every autosave's echo would walk the
whole vault.

## Decisions

### F1. `Fs::watch`, raw events, no clock in `vault`

`Fs` gains:

```rust
fn watch(&self, root: &Path) -> Result<Watch>;

pub struct Watch {
    pub events: UnboundedReceiver<PathEvent>,
    pub root: PathBuf,     // as configured
    pub resolved: PathBuf, // canonical (F3)
    pub watcher: Watcher,
}

pub struct PathEvent { pub path: PathBuf, pub rescan: bool }
pub struct Watcher { /* the platform watch */ } // dropping it stops the watch
```

- **No event kinds.** Every consumer stats the path again (F3), as Zed's
  scanner does. The kind is not trusted: FSEvents reports a rename as one
  `Modify(Name(Any))` per side with no pairing (notify's `fsevent.rs`).
  `rescan: true` carries `Flag::Rescan`.
- **No batching in `vault`.** Batching needs a clock, and only `app` reads one
  (CLAUDE.md). `vault` forwards events as they come; `app` batches (F2).
- **`RealFs`** wraps `notify::recommended_watcher` on `root`, recursive. It
  drops nothing itself: filtering is F3's.
- **`MemFs`** keeps the senders of its watches. Every mutation it performs —
  `insert_file`, `remove`, `save`, `create_parents`, `create`, `rename`,
  `trash`, and the test-only injectors — sends an event for each path it
  touched, the way FSEvents would, including the temporary file of a save. A
  test helper sends a `rescan` event. So tests see our own writes echo, as the
  real disk does.

`notify` **8.2.0**, the stable line (2025-08-03). 9.0 is a release candidate
(rc.5, 2026-08-30); Zed runs a patched fork of it. 8.2 lacks
`EventKindMask`; F3's filter covers it. Its licence, CC0-1.0, joins
`deny.toml`'s allow list in the same change. Upgrade trigger: 9.0 stable.

### F2. Batches of 100 ms in `NoteStore`

`NoteStore` starts the watch when the vault's root is known, before the
initial scan, so no event falls between the scan's read and the watch.

A task drains the receiver: it waits for one event, then for 100 ms
(`WATCH_LATENCY`, Zed's `FS_WATCH_LATENCY`), then takes everything queued. It
passes each event through F3's filter into a `Pending` (F4) and hands the batch
to `Scans` (`NoteStore::rescan_changed`, then `inkra_vault::Scans::batch`).
Under `cfg(test)` the latency is zero, as Zed's fake: a test's events form one
batch once it parks. The code is `app/src/vault/watch.rs`.

Deviation from Zed: the latency lives in the consumer, not in `Fs::watch` as in
Zed (`fs.rs:171-178`). Only `app` reads a clock.

The task holds a weak handle and owns the `Watch`, so `Watch::changed` reads
its roots. `NoteStore` holds the task, not a detached one: dropping the store
drops the task, which drops the `Watcher` and ends the stream. A detached task
owning the `Watch` would keep the watcher alive after the store.

A watch that fails to start (no permission, too many watches) is logged once;
the app keeps working as today.

### F3. Which events count

`Watch::changed(&PathEvent) -> Option<Changed>` in `inkra-vault` decides; it
is pure, so it is tested there and W1a's scoped-walk proptest calls it.
`Changed` is `Path(RelPath)` or `Rescan`. An event counts unless its path,
relative to the root:

- is not under the root, or
- has a component starting with `.` (`read_dir` hides dotfiles and dot
  folders), or
- ends with `.tmp` and its name holds `.inkra-` (our own save's temporary
  file). `temp::is_temp` answers from the same constants `temp_path` names
  the file with.

A `rescan` event that counts is `Changed::Rescan`; one under a dot folder is
dropped with the rest, since `read_dir` would hide what it lost.

An event's path is stripped against the root as configured and against its
canonical form (`std::fs::canonicalize` once, when the watch starts): FSEvents
reports resolved paths (`/private/var`) even when the root was given
unresolved (`/var`), and a symlinked home or iCloud folder hits this. notify
8.2 registers the path as given (`fsevent.rs:376-408`); Zed strips against
both (`worktree.rs:5063-5075`). `RealFs::watch` returns both roots with the
receiver. A test on the real disk watches a root reached through a symlink.

Any other path counts, `.md` or not: a folder has no extension, and a folder
removed cannot be stat'ed to learn it was one. A non-note file only costs a
scoped read of its parent folder.

Our own save's final rename onto `note.md` does count. Its scoped rescan finds
the mtime `reidentify` already wrote into the vault's entry. The index's diff
still lists the note in `Mtimes`: the re-key does not notify, so the index's
last snapshot holds the old mtime. The note is not reloaded: `NoteSaved`
already recorded the saved mtime, so `queue_changed` skips it (measured in W1).
No stat is spent on filtering the echo out: this is Zed's "identical `File`"
answer.

### F4. Scoped rescans

`Pending` in `inkra-vault` is `Folders(BTreeSet<RelPath>)` or `Everything`: a
type in which "folders plus everything" cannot be written. `Scans` holds the
next scan beside the running one, in `Running`, so no pending set exists
without a scan. The next scan is `Next::Batch(Pending)`, the watch's batches
merged, or `Next::Rescan(waiters)`, the `rescan()` requests. Only a request
waits, and a request reads everything: "a waiter on a scoped scan" cannot be
written.

- `Pending::add` on path `p` adds `parent(p)`, never `p`: the set holds
  folders only. Reading `parent(p)` finds a folder created, moved in or renamed
  at `p`, and F5 walks it whole. A change inside a folder the vault holds names
  a path inside it (notify runs FSEvents with `FileEvents`, one event per
  file), so that folder is pending itself. A `rescan` event, or a path equal
  to the root, sets `Everything`.
- Past 256 pending folders the set collapses to `Everything`. Files do not
  count: a batch touching any number of notes in one folder reads that folder
  once. The number is a guess; Zed has no cap.
- A folder under another pending folder is dropped only when the ancestor is
  read recursively; with F5's rule it is not, so both stay.
- A scan starts when none runs; a batch arriving during a scan merges into the
  pending set (`Pending::merge`), which the next scan takes whole (R1's
  coalescing, one level up). `Everything` runs today's full scan; `Folders`
  runs `Vault::walk_changed` and swaps as today, the re-key replay unchanged.
- A vault is read whole, whatever is pending, until a whole walk has applied
  (`Vault::has_read_whole`). The rule keys on evidence, not on `is_scanned`:
  open-first O8 marks a failed initial scan scanned with no entries, and a
  scoped read on that empty base would find nothing. `is_scanned` keeps its O8
  meaning.
- `rescan()` callers (create, rename, trash) keep today's full scan: during a
  scan, a request makes the next scan a `Next::Rescan`, which absorbs any
  pending batch, and waits for it. Their own echo arrives after the full scan
  has started, so each still costs one more scoped scan and one more
  reconcile.

### F5. What a scoped scan reads

`Vault` gains:

```rust
pub async fn walk_changed(&self, fs: &dyn Fs, folders: &BTreeSet<RelPath>, jobs: &dyn Jobs) -> Walk;
```

A method, not an associated function: it must know which folders the vault
holds. For each pending folder that still exists, it reads its listing, **not
recursively**. Then it walks, recursively, every child folder the vault does
not hold at that path: a folder created, moved in, or renamed (a rename is a
new path). A folder that no longer exists, or is now a file (`NotADirectory`),
is skipped; its parent is pending too, and reading the parent drops it.
`MemFs` answers `NotADirectory` for a file, as `RealFs` does.

`Vault::applied` already replaces the children of every folder a walk read and
drops the descendants of a folder that did not survive.
Ids follow inodes (`by_identity`), so a rename inside the vault keeps its id,
and two halves of a rename arriving in two batches pair once both folders are
read.

A pending folder that fails to read keeps its entries (open-first spec, "A
folder that fails keeps its entries"). Unless the error is "not found" or "not
a directory", the walk reports it (`Reach::Incomplete`), and `Scans::swap`
(`crates/vault/src/scans.rs`) makes the next scan read `Everything`: a full
scan, which retries the folder. This is bounded by construction. Only `walk_changed` sets
`Incomplete`, and a full scan applies the open-first rule without asking again,
so a folder that keeps failing costs one extra full scan.

A folder new to the vault, walked below a pending one, that fails to read is
not retried until the next full scan. A full scan applies the same rule to any
folder below the root.

`RealFs::read_dir` stats each child after listing the folder. A child deleted
in between is skipped: a race, not a failed folder. Full scans start exactly
during churn (a `rescan` event, the 256-folder cap, a `git checkout`), and a
root that fails to read is not retried. Other stat errors still fail the read.
`MemFs` lists under one lock and cannot race.

**An external atomic save keeps the note's id.** Vim, VS Code and `git
checkout` save by renaming a temporary file over the note, so the inode
changes. Today `applied` minted ids by identity only (`vault.rs:211-227` before W1a): the
note would get a new id, the index would see a `Layout`, and an open note would
look gone (F9) instead of changed (F8). The rule becomes: a found row whose
identity is new, at a path whose previous entry's identity is found nowhere in
this walk, takes that entry's id. A path whose previous entry reappears
elsewhere in the same walk (renamed away) mints a fresh id.

- This narrows the vault design's rule "a file recreated at the same path gets
  a new id" (`2026-09-20-inkra-vault-design.md`, Identity;
  `.claude/rules/vault.md`). The case it rejected, a note deleted and an
  unrelated one created at its path, now keeps the id when both land between
  two reads of the folder. No signal tells the two apart: F1 discards event
  kinds. Zed accepts the same ambiguity: a path updated with no observed gap
  keeps its entry id (`worktree.rs:3302-3325`, the `entry_for_path` fallback).
  W1 updates both documents in the same change.
- Our own save reaches it only in the open-first O2 race: a scan reads the
  saved file before the save's re-key lands, and the rule gives the new inode
  the note's id directly. The re-key is then replayed at the swap. A replayed
  re-key whose inode the walk already maps to the note's id changes nothing:
  the walk read the saved file at or after the save, so its mtime is at least
  as fresh (`Vault::replay_reidentify`). Otherwise it is an ordinary
  `reidentify`. The rule only holds for a save that moved the inode: an
  in-place save (a hard-linked or symlinked note, `real_fs.rs:139`) keeps it,
  so the walk may have read it before the save and still map it to the note.
  A re-key therefore records, when it is queued, whether the live vault
  already mapped its inode to the note; such an in-place re-key replays as an
  ordinary `reidentify`, the save's mtime winning as it does outside a scan.
- A walk says how much it answers for in one field, `Walk::reach`: `Partial`,
  `Whole` or `Incomplete`, so "whole and failed" cannot be written. `Whole` is
  set only by a walk from the root. `applied` forgets the identities of
  entries it did not find only after a whole walk. `walk_changed` reads the
  root's listing when the root is pending, without its held subfolders:
  inferring "whole" from "read the root" would drop the inode of a note moved
  from the root into a subfolder whose half arrives in the next batch, and the
  move would mint a fresh id. A direct `reidentify`, with no scan running, keeps today's
  rule: the save's mtime wins, since the save came after every applied scan.
- A rename away whose destination folder is read in a later batch than the
  source loses the pairing: the source path's new file took the id first. Rare
  (a move plus a new file at the old path, across two batches); recorded, not
  handled.
- A known window, recorded, not handled: an external `mv a.md b.md` of an open
  note, and our autosave writing `a.md` after the scan took its base but before
  its walk read the folder. The walk gives the note's id to `b.md` (its inode
  moved there) and a fresh id to our saved `a.md`; the replayed re-key maps
  the saved inode to the note's id. For one scan, the saved file has two ids:
  its row's and the note's. Its echo repairs it: the note's id returns to
  `a.md`, and `b.md` takes a fresh id. The save must land inside the walk:
  2.0–3.3 ms for a scoped scan (W1b), 62–124 ms for a full walk at 50,000
  notes (criterion, parallel, nested and flat). The wrong ids last until the
  echo swaps, about 140 ms (the batch and one scoped scan).

### F6. `IndexSync` and the tree need nothing new

A scoped swap notifies the vault entity like any swap. The coalesced reconcile
turns an edit into `Mtimes` and a create, rename or delete into `Layout`. The
sidebar already renders the vault entity.

A swap that changes nothing notifies nobody: our own save's echo, once the
save's re-key has landed. The scan task compares the walk's vault with its
base (`Vault: Eq`, off the foreground); the swap still notifies when a reclaim
or a replayed re-key moved an id. Without it, each echo cost the sidebar's O(n)
rebuild on the foreground (Performance). An echo whose scan took its base
before the re-key landed differs by the save's mtime and inode, and notifies:
the 100 ms batch (F2) makes that rare.

A conflict-preserved file is a new `.md` note, so each conflict costs one
`Layout`. The save that preserved it already calls `rescan()`
(`store.rs:333-338`); the watcher's event for the same file coalesces with it.

### F7. Detecting that an open note changed (W2b)

Two terms, used verbatim below:

- The **expected stamp** is `WriterState.stamp` (`app/src/vault/writer.rs:29-32`):
  the stamp the next save hands `Fs::save`, so the stamp of the file the buffer
  last matched. It is set at open (`Writer::new`, `store.rs:190`), on each
  landed write (`writer.rs:50-53`), and on each applied reload (F8). Nothing
  else moves it.
- A **reload check** is the task F8 describes, queued on the note's `Ops`
  chain.

**The swap detects, not an observer.** The swap (`Scans::swap` and
`Scans::swap_root` in `crates/vault/src/scans.rs`, wired by `NoteStore::swap`
in `crates/app/src/vault/scans.rs`) is the only place an entry takes an mtime
read from disk. The vault entity's other writers are `Scans::reidentify` with
no scan running, which writes our own save's mtime, and the failed-walk path
(`fail` in `crates/vault/src/scans.rs`), which replays those re-keys only. An
observer of the vault entity would fire for all three and would see no
previous state to compare against. So the store, once the swap's
`mem::replace` has returned the previous state (`Swapped::replaced`) and
before that state goes to the background, walks `self.open` and, for each
note:

- `EntryId::DETACHED`: skipped. It has no entry; a note launched by path has no
  vault and no watch (`bootstrap.rs:30-50`, `store.rs:51-55`).
- The new state has no entry for the id, after F9's reclaim: gone from disk
  (F9, W2c). No reload check.
- The new entry's mtime differs from the previous state's entry for the id, or
  the previous state has none: queue a reload check.
- Otherwise nothing.

This is O(open notes): two `Vault::entry` lookups each (`vault.rs:101-107`), no
task and no allocation for an unchanged note. Both swaps of an initial scan run
it. It compares two vault states, not the expected stamp: that stamp sits
behind the `Writer`'s async lock, which no foreground code may hold
(`writer.rs:9`). The exact comparison happens on the chain.

**The check is ordered on the chain.** `swap` queues it with `after_chain`
(`tree_ops.rs:180-197`), behind every operation already queued on the note. A
write runs whole inside its chain task, through its final update
(`store.rs:285-349`). So the check sees the expected stamp and `saved_version`
of every write queued before it. It proceeds in this order:

1. Foreground, one update. Stop if the note is closed, `ops.trashing` is set
   (`tree_ops.rs:26-33`), the note is dirty (`is_dirty`, `store.rs:101-105`), or
   `vault.absolute(id)` is `None`. Else take the path and the entry's mtime.
2. Background. Read the expected stamp (`Writer::expected`). Stop if the entry's
   mtime (taken in step 1) equals the expected stamp's mtime. Else `fs.load(path)`. Stop if the load fails, or if
   `loaded.stamp == expected`.
3. On to F8 with the loaded file.

A failed load is logged, as a failed scan is (`NoteStore::swapped`), and the
note keeps its text. A file deleted before the load shows as gone at the
next swap (F9).

**Our own save's echo loads nothing.**

- No scan runs during the save. `write` re-keys the live vault to the saved
  mtime (`save.rs`, `Scans::reidentify`) before the echo's scan takes its
  base. The echo's swap finds the same mtime in both states and queues nothing.
  This is Zed's "identical `File`" (`buffer_store.rs:564-592`).
- A scan runs during the save. The re-key is recorded and replayed onto the new
  state only (`Scans::reidentify`, then `replace` in
  `crates/vault/src/scans.rs`), so the live state still holds the old mtime
  and the swap queues a check. Step 2 finds the entry's mtime
  equal to the expected stamp's, which `write_through` set from the same
  `saved.stamp` (`writer.rs:50-53`), and stops before loading.

- A check queued earlier, for an external change, can run while a scan holds
  the save's re-key: a scan started by the save's own echo, or any other. Step
  1 then reads the live entry's old mtime, and step 2 loads once and stops at
  `loaded.stamp == expected`. One spare load, never a reload
  (`a_check_queued_before_our_save_loads_once_while_a_scan_holds_the_rekey`,
  `vault/tests/reload.rs`).

Two checks queued on one note need no flag. The second finds the expected
stamp already advanced by the first and stops, or loads a newer file.

Deviation from Zed: `file_updated` decides from `is_dirty` when the worktree
event arrives (`language/src/buffer.rs:1778-1810`). We decide at the swap and
check again at the chain's start, at the snapshot and at the apply (F8). Our
saves run on a per-note chain, so the reload joins it.

### F8. The note changed on disk

**Dirty** (unsaved edits) at step 1: nothing now. The next autosave hands
`Fs::save` the old expected stamp. `Fs::save` finds the disk changed, preserves
the external bytes beside the note, writes ours, and the store emits
`ConflictPreserved` (`store.rs:340-346`; conflict-detection spec). This is
decision 1.

A known gap, shared with Zed (`buffer.rs:1791-1793` reloads only a clean
buffer): a note dirty at the swap and made clean again by `⌘Z` keeps its old
text until the next change on disk. `open_note` returns the open buffer as it
is (`store.rs:113-115`). The next edit makes it dirty, and that autosave
preserves the external version. No bytes are lost.

**Clean.** After F7's step 2 loaded a file whose stamp is not the expected
one, `app/src/vault/reload.rs` runs:

1. Foreground, one update. Stop if the note is dirty. Snapshot the buffer's
   text (`Arc<str>`) and `version()` through `buffer.read`, as `write` copies
   its text (`store.rs:290-293`).
2. Background. `diff(version, &text, &loaded.text)`, wrapped in an `Arc`.
3. Foreground, one update:

   ```rust
   let applied = note.buffer.update(cx, |buffer, cx| {
       let applied = buffer.apply_diff(Instant::now(), &diff);
       if applied && !diff.hunks().is_empty() {
           cx.emit(Reloaded(Arc::clone(&diff)));
           cx.notify();
       }
       applied
   });
   ```

   `false`: the buffer moved since the snapshot, so it is dirty, and the dirty
   rule applies. Stop. `true`: set `note.saved_version` to the buffer's
   version in the same update.
4. Background. `Writer::adopt(loaded.stamp)` sets the expected stamp.

`Writer::expected` and `Writer::adopt` are async and run only inside
`background_spawn` (`writer.rs:9`).

Invariants:

- No step assumes the note stays open. Every foreground update (F7 step 1, F8
  steps 1 and 3) reads `self.open.get(&id)` afresh and stops on `None`. A
  background step holds only the snapshot and the `Arc<Writer>` cloned where
  that lookup last succeeded. There is no `closed` flag, and nothing relies on
  trash being the only way a note leaves `open`.

- Clean at the snapshot and the same version at the apply means clean at the
  apply. Step 1 checks the first; `apply_diff` checks the second
  (`diff.rs:156-159`). `apply_diff` alone would accept a dirty buffer whose
  version did not move. An undo back to exactly the snapshot's version restores
  the snapshot's text, so the diff still lands on the text it was made from.
- The expected stamp advances only after a successful apply. Adopted before, a
  refused apply would leave a dirty buffer whose next save overwrites the
  external bytes without preserving them.
- Between steps 3 and 4 the expected stamp is still the old one. Every write
  but the quit flush is chained behind the reload. The quit flush skips a clean
  note (`store.rs:254-256`); a note typed into meanwhile is saved with the
  external bytes preserved. That costs a spare conflict file, never data.
- `saved_version` moves in the update that applies. The store's buffer
  observer (`store.rs:185-190`) schedules an autosave only for a dirty note, so
  the reload's notify schedules none. A second change on disk right after a
  reload reloads at its own swap. An autosave scheduled for every notify would
  hold the note's chain for `AUTOSAVE_DELAY` and delay that reload by up to
  2 s (`a_reload_schedules_no_autosave`, test 17b).
- `Reloaded` is emitted before `notify`. gpui runs effects in order
  (`gpui/src/app.rs:1760-1785`), so the view maps its offsets (below) before
  `on_buffer_changed` reparses (`editor/mod.rs`).
- A diff with no hunks (a touch, or the same bytes rewritten) edits nothing,
  emits nothing and notifies nothing. Step 4 still adopts the loaded stamp.
  Today such a touch makes the next save preserve the unchanged bytes as a
  conflict file.

**Line endings need no second pass.** `Fs::load` normalizes them
(`vault/src/fs.rs:84-85`; `real_fs.rs:125`, `mem_fs.rs:193`), so `loaded.text`
is LF already. This corrects the sentence of commit `378b2de`: W2b adds no
`normalize_line_endings` call.

**Undo.** The reload is one transaction, sealed on both sides
(`diff.rs:163-170`). `⌘Z` restores the text before it. The version returns to
its pre-reload value, which differs from `saved_version`, so the note is dirty
and autosave writes the user's text. The expected stamp is the reloaded
file's, which matches the disk, so `Fs::save` preserves nothing: the external
version is overwritten. `⌘⇧Z` restores it until the next edit clears the redo
stack (`buffer.rs:125`).

- This corrects F8's earlier sentence ("after preserving that version beside
  it"). `Fs::save` preserves only bytes whose stamp it did not expect
  (`fs.rs:87-89`); after a reload it expects them. Preserving here would need a
  per-note flag for this one case.
- Delaying the stamp's adoption until the next save was considered and
  rejected: a plain reload, then an undo the user keeps, would emit a spurious
  `ConflictPreserved`.
- **Confirmed by the user on 2026-09-27.**
- The vault rule "no write destroys bytes it did not put there" holds in
  spirit: the reload put those bytes in the buffer, they sit in its undo
  history, and `⌘Z` is the user's decision. Zed behaves the same: `did_reload`
  records the saved version and mtime (`buffer.rs:1760-1774`), and an undo then
  a save overwrites.
- After `⌘Z` the caret sits at the end of the last hunk in document order.
  `Buffer::undo` returns the range of the last edit it replays
  (`buffer.rs:199-208`); `apply_diff` recorded the hunks last first, so the
  undo replays them first to last. `EditorState::undo` settles there
  (`state.rs:281-301`), as after any undo.

#### The caret and the scroll

The selection and the marked range are plain `ByteOffset`s in `EditorState`
(`editor/src/selection.rs:12-17`, `state.rs:30-34`), owned by `EditorView`
(`app/src/editor/mod.rs`). The `Buffer` entity is `NoteStore`'s
(`store.rs:20-27`), shared with the view.

Today the view learns of a change it did not make through `cx.observe`
(`EditorView::new`, `open_buffer`). `on_buffer_changed` (`mod.rs`) reparses and
repaints and leaves the selection alone. The view's own changes, typing and
undo, go through `edit_buffer` (`mod.rs`), where `EditorState` places
the caret itself (`state.rs:65-78`, `:281-301`). A reload is the first foreign
change to a note that stays open. A selection left past the new end panics:
`to_utf16` slices the text up to it (`buffer.rs:237`), and the platform asks
for the selected range on its next input query (`input.rs:22-34`).

Three options:

- **(a) Anchors around the reload.** Rejected. `Buffer::anchor` takes
  `&mut self` (`buffer.rs:380`), so the view would lease the buffer before the
  transaction. The view hears of the reload only through an effect, which gpui
  runs after the update that applied it (`gpui/src/app.rs:1760-1785`). "Before"
  would need a second event and a second turn, with typing possible between
  them. Each anchor also stays in the table as a tombstone, walked on every
  later edit (`docs/buffer-model.md`, Anchors).
- **(c) Anchors in `EditorState`, always.** Rejected, which confirms the
  earlier F8. Every caret move would create and forget anchors under a mutable
  lease. `as_mut` notifies (`mod.rs`), which schedules an autosave for
  a caret move (the `copy` rule in `.claude/rules/editor.md`); `move_cursor`
  reads through `buffer.read` today (`mod.rs`). The table would gain two
  tombstones per move, and every keystroke walks it (`buffer.rs:417-421`). Zed
  can do this because its anchors are values resolved against a snapshot
  (`docs/buffer-model.md`, "Not Zed's `Anchor`").
- **(b) A pure mapping through the `Diff`. Chosen.**
  `Diff::map(&self, offset: ByteOffset, bias: Bias) -> ByteOffset` in
  `inkra-text` folds `anchor::transform` (`anchor.rs:19-38`) over the hunks,
  last first. `apply_diff` replays the hunks in that order (`diff.rs:164-169`),
  and each `replace` moves every anchor with that same function
  (`buffer.rs:166`, `:417-421`). So, for any `Diff` that `diff()` can produce,
  a mapped offset equals an anchor created before `apply_diff` and resolved
  after; a proptest pins it. `diff()` never emits two touching hunks:
  `similar`'s `Replace` hook merges adjacent non-equal ops
  (`similar-3.2.0/src/algorithms/replace.rs`). So `Diff::map`'s own contract on
  touching hunks gets a hand-built unit test (test 18b).
  It costs O(hunks) per offset, for five offsets per reload. No lease, no
  table, no second turn.

**How the view learns: an event from the `Buffer` entity.** The update that
applies the diff emits `Reloaded(Arc<Diff>)` (F8, step 3). `Reloaded` is
declared in `app/src/buffer_events.rs`, outside `vault`, so the view imports
nothing from the store, with `impl EventEmitter<Reloaded> for Buffer`; the
orphan rule allows it because `Reloaded` is local. `EditorView` subscribes
beside its observer in `watch_buffer`, called from `new` and `open_buffer`, and
holds both `Subscription`s in `_buffer`.

A `NoteStore` event (`NoteReloaded { id }`) was rejected. The view knows its
buffer, not its note's id (`mod.rs`), so the shell would route the event
by `showing`. An event queued before a note switch would then map the new
note's selection. A buffer event reaches exactly the views showing that
buffer, and `open_buffer` drops a stale one with the old subscription. The
store still talks to no view: the event travels with the buffer. Zed routes it
the same way: `Buffer` emits `BufferEvent::Reloaded` (`buffer.rs:1772`), and
the editor receives it through its multibuffer (`multi_buffer.rs:2039`,
`editor.rs:10213`).

**The handler**, `EditorView::on_buffer_reloaded`, needs no lease:

1. `self.state.map_through(&diff)`, a new pure method on `EditorState` in
   `inkra-editor`:
   - An empty selection (the caret) maps with `Right`.
   - A non-empty selection maps its start with `Right` and its end with `Left`,
     keeping `reversed`. It collapses to the mapped start when the mapped ends
     meet or cross (the selection lay inside one hunk's removed text).
   - The goal resets to `Goal::None`, as after every edit (`state.rs:74-76`).
   - The marked range maps like a non-empty selection. It stays if it is still
     non-empty, else it becomes `None`.
2. It records the top line (below).
3. `cx.notify()`.

The biases are Zed's (`editor/src/selections_collection.rs:1166-1182`): a
caret anchors after, a selection's start after and its end before. So:

- A caret inside a replaced word lands after its replacement. A caret at the
  end of a replaced word lands there with either bias (`anchor.rs:34-36`,
  pinned by `diff/tests.rs:210`).
- A selection spanning a hunk keeps its ends on their text. It never grows to
  take in text inserted at its edges.
- A mapped offset needs no clip. An offset outside every hunk moves by whole
  hunks. One inside a hunk lands on the hunk's edge, which is a grapheme
  boundary of the new text (`diff.rs:122-149`; a line hunk starts a line).

**An IME composition during a reload.** A composition's edits make the note
dirty (`state.rs:109-128`), so a reload reaches it only when the periodic
autosave (`store.rs:17-18`) saved it mid-composition. The marked range maps
like a selection, so the composition stays on its own text and never absorbs
external text at its edges. `marked_text_range` answers the mapped range
(`input.rs:36-39`). If a hunk replaced the whole composition, the range becomes
`None`, as an undo drops it (`state.rs:297-301`). The next
`replace_and_mark_text_in_range` then targets the selection
(`target_range`, `state.rs:86-95`). The view does not call the platform to end
the composition.

**The scroll keeps the top line's text at the top.** The handler has no
`Window`: `cx.subscribe` gives none (`gpui/src/app/context.rs:94-106`), and
`subscribe_in` needs one when it subscribes (`context.rs:341-346`), which
`EditorView::new` lacks (`bootstrap.rs:41`; `.claude/rules/shell.md` forbids
threading one through `bootstrap::root`). So the work is split:

- **The handler records.** With the scroll offset at zero, nothing: the text's
  top stays at the top. Else, with the layout of the old text, let
  `y = viewport.top() - last_text_top - offset.y`: the text-local y at the
  viewport's top (`mod.rs`; `last_text_top` is set in
  `TextElement::paint`, `element.rs:215`). Let `row` be
  `row_for_offset(offset_for_position(point(px(0.), y)))`. It stores
  `restore_top = Some(TopLine { offset, below })`, with
  `offset = diff.map(row_bounds(row).start, Left)` and
  `below = y - top_for_row(row)`.
- **As built 2026-09-28:** the field is now `scroll_to: Option<ScrollTo>`
  (`editor/scroll.rs`), shared with a search hit's reveal; `restore_top` is
  the method that computes a `Top`. See the vault search integration spec, S4.
- **Two reloads before one render.** The second finds `restore_top` still
  pending, and `last_layout` still describes the text before the first reload.
  So the handler maps the pending top line through the new diff and reads the
  layout only when none is pending. A test context draws after every update,
  so the test emits both `Reloaded` in one update
  (`two_reloads_before_a_render_keep_the_top_lines_text_at_the_top`).
- **`render` restores** (`mod.rs`). It takes `restore_top`, calls
  `reshape(window, cx)` (`mod.rs`, which reads the style stored in the
  layout), and sets the scroll offset's y to
  `min(0, viewport.top() - last_text_top - (top_for_row(row_for_offset(offset)) + below))`.
  `render` runs before the div's `Interactivity::prepaint` reads the handle
  through `clamp_scroll_position` (`gpui/src/elements/div.rs:2390-2391`,
  clamp at `:2475-2480`). So the first frame that shows the new text shows it
  at the kept position; from `prepaint` it would land a frame late (the
  `reveal_cursor` rule in `.claude/rules/editor.md`).
- The restored offset is a request; the div's clamp may tighten it. The
  visible result is the mapped row, or the nearest row the shrunk content
  allows.
- No `reveal_cursor`: the scroll stays even when the caret leaves the viewport
  (decision 1).
- `Left`, as Zed's scroll anchor (`editor/src/scroll.rs:254`, `:357`). Text
  inserted at the top line's start shows at the top. A top line replaced whole
  shows its replacement at the top.
- Until that render, `last_layout` describes the old text. A keystroke landing
  in between reads lookups that clamp (`.claude/rules/editor.md`, Layout), and
  `place` clips (`state.rs:267-274`). This is already the state after any write
  that bypasses `edit_buffer` (`mod.rs`).

Deviation from Zed: Zed's selections and scroll position are anchors at all
times. Ours are offsets, mapped once per reload through the `Diff` with Zed's
biases. Two panes on one note stay unsupported (`.claude/rules/editor.md`);
with a buffer event each view would map its own offsets, so the reload is not
what blocks them.

#### The diff

**The diff: `similar` 3.2, in `inkra-text`.** Researched 2026-09-27 at the
user's request (is the whole crate needed, or should we take the blocks we
need?):

- Zed uses `imara-diff` 0.2 (`language/src/text_diff.rs:255-373`): a line diff
  with the histogram algorithm, then a word refinement inside hunks where both
  sides hold at most 512 bytes and 8 lines (`:6-7`, `:322-335`).
- `imara-diff` 0.2.0 was released 2025-06-14: past the 12-month freshness rule,
  with merged fixes unreleased (issues #44, #48). It adds `hashbrown` 0.15 (a
  third version in the build) and `foldhash`.
- Taking its blocks is the worst option: the histogram diff falls back to
  Myers when a common token occurs over 63 times (blank lines in prose do),
  and that Myers holds 10 `unsafe` sites our `unsafe_code = "deny"` rejects.
  About 1,800 lines to copy and keep, under Apache-2.0's notice duties.
- Writing our own: a naive Myers needs a memory cap and a fallback for a note
  rewritten whole; an estimated 150–250 lines plus tests, with worse hunks on
  moved blocks.
- `similar` 3.2.0 (2026-08-17, Apache-2.0, MSRV 1.85) adds no transitive
  crate with `default-features = false, features = ["std"]`; `text` is
  unneeded, since the diff calls `capture_diff_slices` on its own tokens.

So `inkra-text` depends on `similar` and ports Zed's structure: line diff, then
word refinement under Zed's caps, hunks as byte ranges. Deviation from Zed: the
crate, for freshness and dependencies; the hunks it produces for a reload are
of the same kind. `similar`'s `deadline` reads a clock, which `inkra-text` may
not: none is set, and the size of a note bounds the work. Upgrade trigger: a
reload diff shows in a profile.

Built in W2a as `inkra_text::diff(base_version, old, new) -> Diff` and
`Buffer::apply_diff(now, &diff) -> bool`. The app snapshots the text and
`Buffer::version()` together and diffs off-thread. A `Diff` carries its base
version, as Zed's `Diff::base_version` (`language/src/buffer.rs:583`), and
`apply_diff` applies it only if the buffer is still at that version; else it
edits nothing and returns false. So the version check holds by construction,
not by a caller remembering it. `diff` is the only constructor, so a `Diff`'s
hunks are in order, disjoint and each edits something: `apply_diff` checks
none of it. Deviations from Zed's code:

- The line and word diffs use Myers, not histogram: in a one-off release-build
  probe on a 1 MB note with every other line edited, `similar` 3.2's histogram
  took 1.24 s and its Myers 19 ms, for the same 8,963 hunks.
- The tokenizer has no language scope: a word char is alphanumeric or `_`.
- The tokenizer reads grapheme clusters, not chars, and a cluster takes its
  first char's kind. So every hunk starts and ends on a grapheme boundary of
  both texts, and a combining mark or a ZWJ sequence is replaced whole. This
  exceeds Zed: its `tokenize` classifies each char (`text_diff.rs:383-411`),
  so a changed base under a mark gives a hunk inside the grapheme.
- No "both sides non-empty" test before the word pass: a word diff against an
  empty side gives the same single hunk.
- `apply_diff` refuses a stale diff whole, as `reload_impl` does before calling
  Zed's (`buffer.rs:1723`). Zed's `apply_diff` itself rebases hunks past later
  edits (`:2459-2491`) for its other callers, such as formatting; a reload
  needs no rebase.

Figures (`cargo bench -p inkra-text --bench diff -- --quick`, Apple M5 Pro):

- A 50 KB note with one changed line diffs in 24 µs and applies in 0.9 µs.
- A 1 MB note rewritten wholly diffs in 3.5 ms and applies in 40 µs, as one
  hunk.

A proptest holds the diff: applying its hunks to the old text yields the new
text.

### F9. Gone from disk

Redesigned on 2026-09-28, before any W2c code: **a gone note keeps its id.**
The first draft re-keyed the open note to the id of its recreated file; the
review of that draft found a window that opened a second buffer on one file
(below, "Why no re-key").

**The state.** `OpenNote` holds a `Location`: `Vault`, `Detached(path)` or
`Gone(path)`. A vault note carries no path: the store resolves it against the
vault at write time, so a rename in the app never leaves a stale one.
`Detached` is the note of `EntryId::DETACHED`. `Gone` holds the note's last
vault path, set in the same value that marks the note gone, so no note is gone
with a stale path. Gone is a location, never a re-key to `EntryId::DETACHED`:
that id is one shared sentinel and keys `NoteStore.open`, and two notes can be
gone at once. `NoteStore::is_gone(id)` answers it for the shell.
`NoteStore::write_path` gives the path a write goes to: the vault's for
`Vault`, the variant's path otherwise.

- **Deviation from Zed.** Zed's `DiskState::Deleted`
  (`language/src/buffer.rs:406-416`) carries no path: the buffer's `File`
  keeps the path beside it, valid in every state. Our path is not always
  valid: a vault note's path is the vault's, and only a detached or gone note
  holds its own. So the path lives in the variant that needs it.

**One pass in `swap`, two steps.** F7's pass stays the only place a note is
found gone; W2c adds no observer.

1. **Reclaim**, on the new state, before the re-keys replay and before the
   `mem::replace`. It visits each open note other than `DETACHED` with no
   entry in the new state, in `EntryId` order, so the outcome does not depend
   on `HashMap` order. For each, it takes the last path: the path of
   `Location::Gone`, else `live.absolute(id)`. A note with `ops.trashing` set is
   skipped (below, "Trashing a gone note").
   - `Vault::id_at(&RelPath) -> Option<EntryId>` runs one binary search on
     the sorted entries (O(log n)), with the query keyed as `Kind::Note`.
     `compare` consults a kind only where two paths' components diverge, so a
     row at exactly that path compares `Equal` whatever its kind. `id_at`
     returns the row's id only if the row is a note: a folder at the path, or
     a folder named `x.md`, gives `None`.
   - If that id is a key of `self.open`, the store refuses: it is a live open
     note, or a note that took the path earlier in this pass. Else
     `Vault::reclaim(id, other) -> bool` gives the entry the note's id: the
     row's id, `by_id` and the `by_identity` mapping move from `other` to
     `id`.
   - `reclaim` refuses, returns `false` and mutates nothing unless `other`'s
     row exists and is `Kind::Note`: its own contract, not the caller's. It
     also refuses `DETACHED` and an `id` that already has an entry, which
     would give two rows one id. Pure, in `inkra-vault`
     (`vault/src/vault/reclaim.rs`).
   - **Corrected in W2c:** not O(1). `by_identity` maps an inode to an id, so
     moving `other`'s inodes walks the map, O(n), as `reidentify`'s `retain`
     does on every save. It runs only for a note that comes back. The new
     state is not shared yet, so `Arc::make_mut` copies nothing.
2. **Detect**, F7's pass after the `mem::replace`, per open note:
   - No entry: gone. If not gone yet, the location becomes
     `Gone(previous.absolute(id))`, the last vault path. With no path in the
     previous state either, the note is not marked gone. A note with
     `ops.trashing` set is skipped: our own trash closes it (`tree_ops.rs`,
     `trash`). No reload check: a gone note never reaches F8.
   - An entry, and the note gone: the location becomes `Vault`. The previous
     state had no entry for the id, so F7 queues a reload check, as for any
     note whose entry is new.
   - Otherwise F7 as written.
   - Where gone flips either way, the store emits `GoneChanged(id)`. The
     shell subscribes to it, as to the store's other typed events.

Reclaiming before the replay orders the re-keys of saves that landed during
the scan. The re-key of the save the walk read finds its inode already mapped
to the note's id, so `replay_reidentify` changes nothing (F5). The re-key of a
later save finds a live entry and applies as an ordinary `reidentify`, so the
entry holds that save's mtime.

**An open note's id is never reclaimed.** The refusal covers two cases:

- `mv other.md note.md` over the open note, with `other` open: `other` keeps
  its id at the note's path, and the note stays gone. With `other` not open,
  the note reclaims the entry, as the limit below states.
- Two open gone notes with one last path. This is reachable: B goes gone at
  `shared.md`; A is renamed in the app onto `shared.md`; A is deleted
  outside. A file then appears at `shared.md`. The lower id reclaims it; the
  other finds that id in `open`, refuses, and stays gone.

**Writing a gone note.**

- Typing makes the note dirty; autosave writes it to its last path. `write()`
  and `flush_all` take the path from `NoteStore::write_path`, which reads it
  from `Location::Gone` as from `Location::Detached`. A `Vault` note
  resolves through `vault.absolute(id)`, and `write()` emits `SaveFailed` on
  `None` (`save.rs`, `write_path`). A note that is not gone and has no entry
  keeps that path.
- The save's `reidentify(id, …)` finds neither a live entry nor a mapped
  inode, so it changes nothing, as today. The save's echo walks the note's
  folder, finds the file under a fresh id, and the swap reclaims it. The note
  keeps its id, and `current`, `showing`, the quick-open MRU, the sidebar
  selection and the index need nothing. The index keys by id, and ids stay
  stable. Its `forget` and `apply` heal a note that left and came back through
  the next diff.
- The reload check that the swap queues finds the entry's mtime equal to the
  expected stamp's, which the save set, and stops before loading (F7, the
  echo). A later save landing during the echo's scan is replayed after the
  reclaim, so the entry holds its mtime and the check stops the same way.
- The race path already kept the id before this rule, so the rule is
  consistent with it. If a scan runs while the recreating save lands, the swap
  replays its re-key, and `reidentify` moves the row minted for the inode to
  the note's id (`vault.rs`, `reidentify`, the `minted` branch). With the
  reclaim first, the reclaim makes that move and the replay changes nothing.
- `NoteSaved { id }` for the recreating save is harmless: `NoteIndex::insert`
  ignores an id its notes do not hold. The index sees the note again through
  the swap's `Layout`, under its own id.
- **A missing folder is recreated, for a gone note only.** A gone note's
  folder may be gone too. `Fs::save` fails on a missing folder, on both
  backends: `MemFs::save` returns `NotFound`, as `RealFs` does when it
  creates the temporary file. A separate `Fs::create_parents(path)` creates
  every missing folder above `path` (`mkdir -p`, idempotent): `RealFs` through
  `create_dir_all`, `MemFs` through `insert_parents`, with its watch events.
  `write()` and `flush_all` call it before the save only when the note is
  `gone`, never for `DETACHED`: they pass `Folder::Create` for a gone note
  and `Folder::MustExist` otherwise. `write_through` runs it under the
  `Writer`'s lock, after the sequence check.
  - **Corrected in the W2c review:** W2c first made `RealFs::save` create the
    folders itself whenever the destination had no file. A dirty present note
    whose folder was removed outside, autosaved before the swap marked it
    gone, then recreated the old folder: a second copy of the note beside a
    folder moved elsewhere. `MemFs::save` always created the folders, so no
    test could catch it. Kept apart from `save`, the recreation needs the
    `gone` flag that only the store holds; before the swap, that save fails
    with `SaveFailed`, and the note stays dirty.
  - The recreated folder is a new folder with a new id, collapsed in the tree.

**An external restore** (a `git checkout`, a file copied back) is the same
reclaim. The swap's reload check then applies F8: a clean note reloads the
restored text; a dirty note keeps its edits, and its next save preserves the
restored bytes beside it (conflict path).

**Closing.** There is no close action (vault spec, Deferred: buffer eviction).
"Closing" is switching to another note: `set_current` saves the note left
only when it is dirty, so an untouched gone note writes nothing. It stays in
`open`, unreachable from the tree, and a later file at its path is reclaimed
for it. The quit flush writes a dirty gone note to its last path; it still skips
a note with `ops.trashing` set.

**Trashing a gone note.** `trash`'s postcondition is "the note is not in
`open`". Before W2c, `trash` returned `false` silently when the vault held no
entry, and a gone note would stay open. Now, when the entry is absent and the
note is open, `trash` takes the success path's close branch: `open` and
`observers` removed, `current` cleared, `NoteClosed`, a rescan. Unsaved text
is discarded: the confirmation already says it will be lost. Details:

- The close stays an operation on the note's chain, and it decides there, as
  `trash` resolves its path there (`tree_ops.rs`, `trash_target`). An entry
  present: `fs.trash`, then close, as before. No entry and the note gone: the
  last path, checked as below, then close. No entry and the note not gone:
  close without `fs.trash`.
  - **Corrected in the W2c review:** the chain step reads the entry when it
    runs, never the `gone` taken when `trash` was issued. A present note
    whose file leaves while its trash waits on the preempted write reaches
    the step with no entry; reusing the issue-time `gone` (false) emitted
    `TrashFailed` and left the note open. Its location stays `Vault`, since
    F7's pass skips a note with `ops.trashing` set, so nothing on disk is
    its file. The issue-time `gone` only chooses between cancelling and
    preempting the pending autosave.
- A gone note is not preempted with `schedule_save(id, ZERO)`: that would
  write the text the trash discards. Its pending autosave is cancelled
  through `Ops::cancel_pending`; the vault rule against cancelling guards the
  previous save's text, and a gone note has none on disk.
- A write already running lands first and recreates the file. If the swap's
  replay gives it the note's id before the close runs, the close trashes it
  as usual. If not, the vault holds no entry for it: the swap has not run
  yet, or its reclaim skipped the note, since `ops.trashing` is set.
- **Corrected in the final W2c review:** the close then left that file on
  disk, and it came back as a new note. Now the step, after the chain so
  that every write has landed, loads the file at the last path through
  `Fs::load`. It trashes the file only if the loaded stamp equals the stamp
  the note's `Writer` expects: that of its last write, or of the file it
  last loaded. Otherwise, or with no file there, it closes the note and
  touches nothing: a file that appeared at the path meanwhile is not the
  note's. Never "trash whatever sits at the path".
  - A load, not a new `Fs` stat method: `reload` already compares a loaded
    stamp with the expected one, a trash of a gone note is rare, and a note
    is small. A second `Fs` backend owes nothing new.
  - Stamps are compared for equality, never ordered (vault rule). An
    unrelated file with the same mtime and size would be trashed: the
    conflict check trusts the stamp alike.

**A note moved inside the vault is not gone:** its inode keeps its id, and the
store resolves its path at write time already (`Location::Vault`).

**The limit.** A file at the path that is unrelated to the note is taken as
its return: a different note created at the path while ours is gone becomes
the note, and a clean note reloads its text. F5 accepts the same ambiguity
for a replace between two reads; no signal tells the two apart. A dirty note
preserves the unrelated file's bytes beside it on its next save, so no bytes
are lost.

**The banner.** One line at the top of the editor pane, shown only while the
note showing is gone: "Deleted from disk. Typing restores it." `AppShell`
renders it from `showing` and `NoteStore::is_gone`, and re-renders on
`GoneChanged` for the note showing, with no observer of the whole store.
Colours and size come from `theme.rs` (`UI_TEXT_SIZE`,
`UI_LINE_HEIGHT`), never a literal: `gone_banner_background` and `gone_banner_text`,
added for it. `EditorView` is unchanged: it knows its buffer, not its note. No
key binding is added.

- The editor slot becomes a column: the banner, then the editor in a
  `flex_1().min_h_0()` box. Without `min_h_0` a flex child's minimum is its
  content, the editor grows to the note's height and never scrolls; the
  scroll-restore tests of `shell/tests/reload.rs` fail without it.
- The `gone-from-disk` shot shows the band across the editor pane's top, its
  one line level with the tree's first row, the note's text below it, and the
  deleted note absent from the tree.

**Why no re-key.** The first draft re-keyed the store to the recreated file's
id, as an `after_chain(old_id, …)` operation, and emitted `NoteRekeyed { old,
new }`. The operation waits behind the note's chain, which holds a 2 s
autosave timer while the user types. Meanwhile the tree and quick open list
the new id: a click loads a second buffer on the file, and the re-key then
either strands the buffer on screen or leaves two notes on one file. The
reclaim happens in the swap, so no state ever shows the new id. The W2b
constraints written for that draft are moot: no move of `open`, `observers`
and `ops`, no chained re-key, no `NoteRekeyed`, no holder to update. Two
constraints stand: the gone check is F7's pass, and a gone note never
reloads.

The implementation updates the id rule in `.claude/rules/vault.md` and the
vault design's Identity section: an open note gone from disk reclaims a file
new to the vault at its last path.

**Tests** (W2c, each written first and failing):

- `inkra-vault`: `id_at` finds a note, and finds nothing for a missing path,
  a folder at the path, or a folder named `x.md`. `reclaim` moves the row's
  id, `by_id` and `by_identity`. It refuses a folder's id or an absent one,
  and then leaves the vault unchanged.
- `vault/tests/real_fs.rs`: `RealFs::save` to a path whose folders are missing
  fails with `NotFound` and creates nothing; `create_parents`, twice, then
  `save` writes the file. `MemFs` agrees on both steps.
- Store, `vault/tests/gone.rs`, on `MemFs`. Measured in W2c: on `MemFs` the
  recreating save's own echo lands while a scan runs, so "typing recreates it
  under its id" passed before the reclaim existed, through the replay (above,
  the race path). The external-restore tests and the quick-open test are the
  ones that failed first; they reach the reclaim alone.
  - An open clean note removed on disk stays open, keeps its text, is
    `gone`, emits no `Reloaded`, and emits one `GoneChanged`. The same for a
    dirty note, which keeps its edits.
  - Our own trash of an open note closes it as today; it is never `gone`.
  - A present dirty note whose folder is removed outside, autosaved while the
    swap is held (`block_read_dir`): `SaveFailed`, the folder is not
    recreated. After the swap the note is `gone`, and the next write
    recreates it. The quit flush test removes the folder too.
  - Typing in a gone note: the autosave writes the buffer's text at its last
    path. After the echo's swap the note is not `gone`, its id is unchanged,
    the vault holds one entry at the path under that id, and nothing reloads.
  - An external restore of a clean gone note: the note keeps its id, is not
    `gone`, and shows the restored text.
  - An external restore of a dirty gone note: the text is kept, and the next
    autosave emits `ConflictPreserved` with the restored bytes preserved.
  - Switching away from an untouched gone note writes nothing.
  - The quit flush writes a dirty gone note at its last path.
  - `mv other.md note.md` over an open note, with `other` open: the note is
    `gone`, and `other` keeps its id.
  - Two open gone notes with one last path (above): a file appears there, the
    lower id reclaims it, and the other stays `gone`.
  - Trashing a gone note, clean and dirty: it leaves `open`, `NoteClosed` is
    emitted, and nothing is written.
  - A present dirty note whose trash waits on its preempted write
    (`block_save`), its folder removed outside meanwhile: the trash resolves
    `true`, the note leaves `open` and `current`, `NoteClosed` is emitted, and
    no `TrashFailed`.
  - A dirty gone note whose autosave recreated the file, trashed while the
    echo's swap is held (`block_read_dir`): the file is trashed, the note
    closes, and no `TrashFailed`. Failed first: the file stayed on disk.
  - A gone note with an unrelated file at its path, trashed while the swap is
    held: the note closes, and the file stays untouched.
- Shell: the banner shows only while the note showing is gone, following
  `GoneChanged`.
- Shot: a `gone-from-disk` scene renders the banner; look at it.

## Testing

`app` tests run on `MemFs` with zero latency; every key binding added gets a
`simulate_keystrokes` test. Written before the code; each fails first.

W1:
1. A note edited through `MemFs` shows its new links in backlinks after one
   batch, with no create, rename or trash.
2. A note created outside the app is named in quick open; one deleted leaves
   it; one renamed keeps its id.
3. A folder moved in with notes inside: all its notes appear. A folder
   removed: its whole subtree leaves.
4. An autosave's echo walks no folder beyond the note's own (count `read_dir`
   calls; `MemFs::read_dirs` exists) and reloads nothing in the index.
5. A `rescan` event triggers one full scan.
6. Events arriving during a scan start one more scan, not one per batch.
7. Dotfiles and our `.inkra-*.tmp` files trigger no scan.
8. `vault`: `walk_changed` unit tests and a proptest against a full walk: for
   any tree and any set of changed paths whose parents are passed, applying
   `walk_changed` gives the same entries as a full rescan.

W2 (12 and 14 are W2c, detailed in F9, Tests; the rest are W2b). Tests 9, 15, 16 and 16b root at
`AppShell` (`open_vault`), since they need the store and the editor together
(`shell/tests/reload.rs`). Tests 10, 11, 11b, 13, 17 and 17b root at the store
(`vault/tests/reload.rs`): the shell's index loads a changed note too, which
`MemFs::loads` and `block_load` would count as the reload's:

9. A clean open note changed on disk (`MemFs::insert_file`: a line inserted
   before the caret's word, a word changed after it) shows the new text, and
   the caret is inside the same word. `simulate_keystrokes("cmd-z")` restores
   the old text in one step. After `AUTOSAVE_DELAY` the old text is on disk and
   no `ConflictPreserved` was emitted (F8, Undo).
10. A dirty open note changed on disk keeps its text and emits no
    `Reloaded`. The next autosave emits `ConflictPreserved`, and the
    preserved file holds the external text. The check queued for the external
    change runs behind that autosave. It loads once when the scan started by
    the save's echo still holds the re-key (F7, the echo), which depends on
    the scheduler, so this test does not assert `MemFs::loads`.
    `a_check_queued_before_our_save_loads_once_while_a_scan_holds_the_rekey`
    holds that scan with `block_read_dir` on the root and asserts the one load
    and no `Reloaded`.
11. Typing during a reload drops it. `block_load` holds the load, a key is
    typed, the gate is released: the typed text stays and the next autosave
    emits `ConflictPreserved`.
11b. Typing during the background diff. The reload module holds a
    `#[cfg(test)]` gate that parks after the diff resolves and before the
    foreground apply. Hold it, type a key, release: `apply_diff` returns
    false, no `Reloaded` is emitted, `saved_version` and the expected stamp do
    not move, and the next autosave emits `ConflictPreserved` with the
    external bytes in the preserved file.
12. An open note deleted on disk stays open and `gone`; typing recreates it
    under the same id; switching away from it untouched writes nothing; an
    external restore reloads a clean one (F9, Tests).
13. Our own save's echo loads nothing and leaves the buffer's version
    unchanged. Twice: with no scan running, and with a scan held open by
    `block_read_dir` on the root while the save lands (F7, the echo).
14. The render: the gone banner, looked at with the `gone-from-disk` shot.
15. An IME composition, saved clean by the autosave, then a line inserted
    above it on disk: `marked_text_range` covers the same composing text. A
    second case replaces the composing text on disk: the marked range is
    `None` and nothing panics.
16. The scroll: a 200-line note scrolled so line 120 is at the viewport's top,
    then 30 lines inserted at the note's start on disk. After the next draw
    the offset at the viewport's top is line 120's start, read from the layout
    rather than computed from pixel sizes (`.claude/rules/app-tests.md`).
16b. Scrolled near the bottom, an external edit shrinks the note below the
    viewport's height: nothing panics. The scroll offset is 0, which gpui's
    clamp decides, not the restore
    (`a_reload_that_shrinks_the_note_below_the_viewport_does_not_panic`).
17. A touch (the same bytes, a new mtime): no `Reloaded`, the version
    unchanged; typing then autosaving emits no `ConflictPreserved`.
17b. A reload schedules no autosave: the store's buffer observer skips the
    reload's notify, since the note is clean (F8, the `saved_version`
    invariant; `a_reload_schedules_no_autosave`).
18. `inkra-text`: a proptest that `Diff::map(offset, bias)` equals an anchor
    created at `offset` before `apply_diff` and resolved after, for any two
    texts, any char-boundary offset and either bias.
18b. `inkra-text`: `Diff::map` on a hand-built `Diff` with two touching hunks,
    at their shared edge and inside each, with both biases. The fields are
    private, so the test builds it as a struct literal in `diff/tests.rs`.
19. `inkra-editor`: `EditorState::map_through` unit tests. A caret inside a
    replaced word lands after the replacement. A selection does not grow at
    either edge. A selection inside one hunk collapses to the mapped start. A
    marked range the diff removed becomes `None`. The goal resets.

Shot: a `reload-scroll` scene renders test 16's end state once, to look at it.
`Stage::open_vault` builds a `MemFs` it does not return (`shot/stage.rs:51-60`);
W2b keeps it on the `Stage` so a scene can change a file after opening.
`cargo mutants` runs after the `inkra-text` and `inkra-editor` changes.

`RealFs::watch` gets one test on a real temporary folder in
`vault/tests/real_fs.rs`, the one place a test touches the disk: create a note,
receive an event under its path within a second.

## Performance

Measured in W1b, release, on the 50,000-note vault from
`crates/app/examples/startup_vault.rs`, with a temporary probe that logs each
scoped scan and each reconcile (the timeline stops after launch):

| Figure | Target | Measured |
|---|---|---|
| An external edit to one note: batch to index updated | under 150 ms (100 ms of it is the batch) | 141–146 ms |
| Background work of that scoped scan | one `read_dir` of the note's folder | walk 2.0–3.3 ms, then `Vault::applied` 32–38 ms |
| An autosave's echo: foreground work | under 0.1 ms | 0.0013–0.0043 ms (2026-09-28). Before: 1.17–1.26 ms with 100 collapsed folders, 2.12–2.37 ms flat, almost all `Sidebar::rebuild` on the swap's notify; W1b's 0.021–0.031 ms probably did not count it. The scan task now finds the walk's vault equal to its base (1.6 ms of background, criterion) and the swap notifies nobody. Release, Apple M5 Pro, a `#[gpui::test]` over a 50,000-note `MemFs`, 8 echoes each; an echo whose scan starts before the save's re-key still notifies (F6) |
| The coalesced-reconcile spec's Performance table | its targets | measured 2026-09-28: see that spec's Performance section |
| A reload of a 50 KB note with one changed line: foreground work (snapshot, apply, mapping, one reshape) | under 1 ms | 0.49–0.69 ms: the reshape 0.47–0.62 ms, the markdown reparse 60 µs, the rest under 6 µs (W2b, release, Apple M5 Pro, the `shot` harness with a temporary probe, 5 reloads of a 51 KB note) |
| A burst of 1,000 notes in 2 folders | — | not re-measured: with folders-only `Pending` the burst names 2 folders and stays scoped |

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| `notify` 9 | 9.0 ships stable | `EventKindMask::CORE`, `with_fsevent_latency`; Zed's fork resumes from the last event id across restarts |
| Poll watcher for network mounts | A vault on SMB, NFS or FUSE misses events | Zed's `requires_poll_watcher` (`fs_watcher.rs:287`) |
| Producer-side change sets | A third consumer of the vault's changes | Coalesced-reconcile spec, Deferred |
| A reload that merges into a dirty note | The user asks | Rejected by the conflict spec: it fabricates a version neither side wrote |
| Root deleted while running | The user deletes the vault folder | Zed checks the root every 5 s (`worktree.rs:85-89`) |
| Apply a scoped walk as a delta | The margin is missed on a real vault | `Vault::applied` is O(n): 30–38 ms of background work per scoped scan at 50,000 notes (probe), about 5 ms of margin under the target. Lever: apply the walk as a delta instead of rebuilding the sorted entries |
| `MemFs::save` in place for a hard-linked path | The next test that needs an in-place save through the real save path | `MemFs::save` always mints a fresh identity, even for a hard-linked path; `RealFs` writes in place when `nlink > 1` |
| A size bound on a note that is diffed (a 100 MB `.md`) | A reload diff shows in a profile, or a vault holds a note over some size | `text` stays policy-free; the bound belongs to `app`'s reload (or `vault`'s load) |
