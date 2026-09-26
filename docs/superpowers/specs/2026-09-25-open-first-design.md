# Open first, scan behind — design

Date: 2026-09-25. Status: **reviewed** by the user on 2026-09-25. Slice 6 of the search work
(renumbered in [`../plans/2026-09-25-smart-quick-open.md`](../plans/2026-09-25-smart-quick-open.md)).
Decided with the user on 2026-09-25, one question at a time.

## Why

The user's priority is an app that stays fluid and opens at once. Slice 5
measured the release app on a vault of 50,000 notes (Apple M5 Pro, warm cache):

| Step | Figure | Budget |
|---|---:|---|
| vault scan, before the window opens | 266–276 ms | — |
| first frame | 397–483 ms (863 ms cold) | 100 ms |
| longest `IndexSync` foreground turn | 5.4–6.4 ms | 8 ms, aim 4 ms |

The window waits for the whole scan: `bootstrap::launch` blocks on
`Vault::scan` before `open_window` (`crates/app/src/bootstrap.rs:56-73`). The
longest turn is most probably `IndexSync::reconcile` copying the 50,000 vault
entries on every rescan (search spec D5 measured that copy at 1.4–4.9 ms in
slice 2); it is not profiled.

## What the user decided

1. The first frame shows the vault's **root level**: its folders and notes.
   Subfolders are scanned behind it. They are closed by default, as the
   sidebar's `expanded` set already starts empty
   (`crates/app/src/sidebar/mod.rs:65`), so nothing moves on screen.
2. Reopening the last note is wanted, **later** (Deferred).
3. `⌘P` pressed before the scan ends opens at once and says "Scanning the
   vault…"; it lists results after one single build, as today.
4. Approach 1 of three: a root pass, then **one** background pass for the rest.
   Level-by-level application and a priority scan of an opened folder are
   deferred.
5. The rescan's cost is fixed in the same slice: the vault's entries are
   shared, not copied.
6. The scan takes what is best now, knowing a file watcher comes later: the
   spec leaves its seams (O3) and a sonnet red team, blue team and researcher
   reviewed it on 2026-09-25 (see "Review").

## Scope

| In | Out, deferred below |
|---|---|
| The window draws before any read of the vault | Reopening the last note |
| `Vault` as an immutable shared state; the sort and the id minting off the foreground | The tree filled level by level |
| One scan worker: scans never overlap, and requests during a scan coalesce into one | A priority scan of an opened folder |
| A scan replaces only the folders it read; a folder it failed to read keeps its entries | A file watcher (this spec leaves its seams) |
| A parallel walk, kept if the bench shows a gain | |
| `scanned`: no index build and a "Scanning the vault…" state until the first full scan | |
| `reconcile` clones an `Arc`, not 50,000 entries | |

## What Zed does, and what we take from it

Zed's worktree runs a `BackgroundScanner` that owns the snapshot. It sends
`ScanState::Updated { snapshot, changes, scanning }` to the foreground, which
only calls `set_snapshot` and sets an `is_scanning` flag
(`vendor/zed-reference/crates/worktree/src/worktree.rs:1415-1422`). A snapshot
is a `SumTree` of entries, cloned cheaply (`:184-206`). `scan_dirs` walks with
one job per core, each reading one folder and queueing its subfolders
(`:5276-5345`). A watcher event reloads only the touched paths
(`reload_entries_for_paths`, `:5671-5760`), and events are batched every
100 ms (`FS_WATCH_LATENCY`, `:83`).

Taken: one owner of scans, whose results the foreground swaps in; a scanning
flag; the parallel walk; a scan that can cover part of the tree, which is the
seam a watcher needs.

Deviations:

- **One batch, not periodic snapshots.** Zed sends a snapshot per progress
  tick during the initial scan (`send_status_update`, `:5347`). The folders are
  closed, so a partial tree shows nothing more; one swap costs one index build
  (decision 3).
- **`Arc<[Entry]>`, not a `SumTree`.** A scan rebuilds the entry list, so a
  persistent tree's cheap edits buy nothing yet. The `sum_tree` crate is also
  GPL-3.0-or-later.
- **The first frame reads nothing.** Zed also opens the worktree empty, but we
  keep `launch`'s contract that an unreadable vault exits: only a readability
  check runs before the window, not a `read_dir`, so a vault of 50,000 notes
  in its root costs the first frame nothing.
- **A folder that fails keeps its entries.** Zed's `reload_entries_for_paths`
  removes a path's descendants before it reads them again (`:5721-5733`), so a
  failed read drops them. Our scans are the only source of the tree until the
  watcher, so a failed read must not empty a folder (O3).
- **One entry that fails fails its folder.** Zed's `scan_dir` logs an entry
  whose metadata fails and goes on (`:5400-5413`); `RealFs::read_dir` fails the
  folder instead, so a file deleted between the listing and its `lstat` fails
  its folder, which keeps its entries until the next scan. Kept: the next scan
  settles it, and it changes no `Fs` contract.
- **Ids by inode alone.** Zed reuses an id by path and inode, and by inode alone
  only with an equal mtime or path, because an inode can be reused
  (`reused_entry_id`, `:3292-3325`). Ours stay keyed by `(device, inode)`, as
  the vault spec decided; `reidentify` (O2) relies on it within one save.
- **A hand-rolled queue, not an MPMC channel.** Zed's jobs share an unbounded
  `async_channel` of folders, and each queued folder holds a sender, so the
  channel closes when the last folder is read (`:4458-4490`, `:6532`). Our
  walk keeps the folders in a `Mutex`, with a count of folders being read and
  the wakers of the jobs that wait for them (O4). It avoids a dependency on
  `async-channel` in `inkra-vault`.

## Decisions

### O1. `Vault` is an immutable shared state

`Vault` holds its state behind one `Arc`: the entries as `Arc<[Entry]>`, the
`by_id` and `by_identity` maps, `next_id`, the root, and `scanned`. Cloning a
`Vault` is an `Arc` bump; `entries()` borrows the shared slice, and
`shared_entries()` hands the `Arc<[Entry]>` out for a background task (O7).
`Vault::new(root)` holds nothing and is not scanned; `is_scanned()` reads the
flag and `mark_scanned()` sets it (O8).

`Vault::scan`, `rescan`, `collect` and `apply` stay, with today's behaviour
(but for the id order and hard links, O3), as thin wrappers over O3's
functions, so the 20-odd call sites in `vault`, `index` and `app` tests and
benches do not change. `scan` and `rescan` are full scans and mark the vault
scanned. Only `NoteStore` moves to the worker.

- **Corrected (review, 2026-09-26): `collect` and `apply` are gone.** Once
  every caller was a test, they only duplicated `rescan`: a test that rescans
  calls `rescan`, and one that shapes a walk calls `walk` and `applied`.

### O2. One scan worker

`NoteStore` owns a single scan worker, as today's `rescan` already chains each
walk after the previous one (`crates/app/src/vault/tree_ops.rs:201-226`).

- **Scans never overlap.** A scan starts from the state the previous scan
  produced, so ids are minted from the latest `next_id` and `by_identity`, in
  one serial history, as today's `&mut self` `apply` guarantees.
- **Requests coalesce.** A request while a scan runs marks the worker dirty.
  When the scan applies, one more scan runs and answers every request made
  meanwhile. A burst of requests costs two scans, not one per request. Each
  request's `oneshot` resolves when a scan that started after the request
  applies.
- **A write between a scan's start and its apply is not lost.** `reidentify`
  (`crates/app/src/vault/store.rs:307-318`) patches the live state after every
  save: an atomic save gives the note a new inode, and a save in place moves its
  mtime. The worker records each patch made
  while a scan runs, and replays it onto that scan's result before the swap.
  A save during a scan is the only source of a patch. Replaying is
  O(patches × entries): each replayed re-key of a live id runs
  `by_identity.retain` over every entry. A scan usually replays one patch,
  about 57 µs at 50,000 entries (the `vault` bench).
- **Corrected (plan, 2026-09-25): `reidentify` reclaims the id.** The replay
  above fails its own test when the held walk reads the file after the save:
  the walk sees the new inode, `applied` mints it a fresh id, the note's id is
  gone from the result, and `reidentify` of a dead id was a no-op. The rule is
  now: when `id` names no live entry and `identity` names a live one, that entry
  takes `id` back, because the inode our write produced is our note. The replay
  is then correct whichever inode the walk read. It also closes an older race:
  a scan that swaps in between `fs.save` and the foreground re-key minted the
  saved file a fresh id and orphaned the open buffer.
- **A re-key during a scan only records** (plan challenge, 2026-09-25). The
  scan's base shares the live state, so re-keying the live vault then would
  copy both 50,000-entry maps on the foreground. The swap replays the re-keys
  onto the result, or onto the live vault when the walk failed. The base is
  taken in the turn that starts the scan, so no re-key escapes the record.
- **Corrected (review, 2026-09-26): a re-key carries the save's mtime.** Two
  saves during one held scan replay two re-keys, but the walk read only the
  first save. The entry kept that save's mtime, so `IndexSync` reloaded the
  note after the swap. `reidentify` now takes the save's `Mtime` (the entry
  holds no size, so not the `Stamp`). It sets that mtime unless a walk read the
  saved inode and minted it a fresh id, the reclaim's case. That walk read the
  inode at or after the save, so its mtime is as new or newer, and stays: a
  write in place after the save keeps the inode and moves the mtime, and
  `IndexSync` must reload the note (review of 5c67e65).
- **Corrected (review of 3591ca4, 2026-09-26): a re-key copies the entries
  only when it changes one.** An `IndexSync` build holds the shared entries,
  so `Arc::make_mut` on them copies 50,000 entries on the foreground.
  `reidentify` now calls it only when the entry's id or mtime changes. A save
  still moves its note's mtime, so a save during a build still copies them:
  134.9–135.3 µs at 50,000 entries, two runs of the `reidentify shared 50k`
  bench (Performance). An earlier throwaway variant measured 161 µs once.
- **Corrected (review, 2026-09-26): an in-place save sets its mtime.** Commit
  5c0c199 set the save's mtime only when the vault had no id for the saved
  inode, on the premise that every save gives the note a new inode. `RealFs`
  writes a hard-linked or symlinked note in place, so the scan before the save
  already mapped the inode to the note's id. The entry kept its pre-save mtime,
  and `IndexSync` reloaded the note after every save. Accepted: after an
  in-place save, a walk, then an external in-place write that walk read, the
  replay sets the save's mtime, and the external write waits for the next scan.
- **Corrected (review, 2026-09-26):** `reidentify` is a no-op for
  `EntryId::DETACHED`, so no entry carries it (commit 0644cb5). What broke: a
  note opened by path inside the vault root saves through `DETACHED`, and when
  a scan read its new inode first, the reclaim gave `DETACHED` to that scanned
  entry.
- **Corrected (review, 2026-09-26): the reclaim remaps a kept identity.** When
  `identity` maps to an id that no entry carries (a mapping a partial walk
  kept), `reidentify` maps it to `id`. The next scan that finds the file then
  gives it our note's id: the inode our write produced is our note.
- **`reidentify`, the whole rule** (moved from its doc comment, 2026-09-26).
  After every save, it maps the saved inode to `id` and drops any other
  identity kept on `id`. It gives the entry the save's mtime, unless a scan
  read the saved inode and minted it a fresh id. That entry then takes `id`
  back: the inode our save produced is our note. It is a no-op when `id` names
  no live entry and the inode no id, and for `EntryId::DETACHED`, so no entry
  ever carries that id. It returns whether an entry's id moved; the worker
  notifies only then.

### O3. A scan covers folders, and replaces only what it read

```rust
pub struct Walk {                 // what one scan read from disk
    pub read: Vec<RelPath>,       // the folders whose listing it got, whole
    pub found: Vec<DirEntry>,     // their children
}
impl Vault {
    pub async fn walk(fs: &dyn Fs, root: &Path, from: &RelPath, jobs: &dyn Jobs) -> Result<Walk>;
    #[must_use]
    pub fn applied(&self, walk: Walk) -> Vault;
}
```

`applied` replaces the children of every folder in `walk.read` with what it
found there, recursively for the subfolders it read, and keeps every other
entry. It mints and reuses ids by `(device, inode)` as today, sorts with
today's `compare`, and runs on the background executor.

- A folder whose `read_dir` failed mid-walk (renamed or deleted under the walk,
  a permission change) is not in `read`, so its known entries stay instead of
  vanishing. Its kept entries wait for the next requested scan.
- An entry whose folder the result no longer holds goes with it: a folder
  renamed under an unread parent takes its kept subtree along.
- The root pass is the `Walk` of the root's own listing,
  `Walk { read: [root], found: fs.read_dir(root) }`: no second walk function.
- A full scan is `walk(fs, root, "")`. The first pass after the window opens
  reads the root alone (see O6).
- This is the seam the watcher needs: an event for a folder becomes a `walk`
  from that folder. FSEvents' `MustScanSubDirs` maps to a `walk` of that
  subtree, and a dropped-events signal to a full scan.
- `id_for` looks an id up by inode, never by path (moved from its doc comment,
  2026-09-26). A path lookup would answer with whatever holds that name now,
  which after an external move is a different file.

`Vault::apply`, the wrapper of O1, was `applied` with every folder read; it is
gone (O1, Corrected).

- **Corrected (plan, 2026-09-25): ids in path order.** `applied` sorts what the
  walk found before it mints: fresh ids follow path order, and hard links
  collapse to the path that sorts first. Today they follow the walk's order,
  which a parallel walk does not fix. A file the walk found also claims its
  inode from an entry kept under an unread folder: a move into a read folder
  leaves the unread one. So a hard link spanning a partial walk's boundary
  shows the path the walk read, until a full scan.
- **Corrected (plan, 2026-09-25): `MemFs::read_dir` fails on a missing
  folder** with `NotFound`, as `RealFs` does; it listed nothing before. A folder
  renamed mid-walk must fail, not read as empty and lose its notes. `insert_dir`
  creates the parents, as `insert_file` does.
- **Corrected (plan, 2026-09-25): no automatic rescan.** The draft had the
  worker mark itself dirty after a folder failed, so the next scan settles it.
  A permanently unreadable folder would then rescan forever. Its kept entries
  wait for the next requested scan: our own create, rename, trash or conflict
  now, the watcher later.
- **Corrected (review, 2026-09-26): a partial walk keeps the identities it no
  longer sees; a walk from the root drops them.** A partial walk is a `walk`
  from a folder: it answers only for the folders it read. A file that left
  them may have moved outside, so `applied` keeps its `(device, inode)` → id
  mapping with no live entry. The next scan that finds the file gives it that
  id back. `id_for` may therefore name no live entry.
  A walk whose `read` holds the root answers for every identity: `applied`
  keeps the mappings of the entries it keeps, and drops every other. That
  holds even when a folder fails or lies below `MAX_DEPTH`: such a folder is
  found but not read, so its known entries stay live entries, and their
  identities stay with them. The root pass, the seeded full pass and every
  scan today are walks from the root. The root pass applies onto
  `Vault::new`, which holds no identity, so it drops none.
  What broke before (review of 3591ca4): only a complete scan, one that read
  every folder, dropped an identity. A vault with a permanently unreadable
  folder never got one, so a deleted note's mapping stayed. When the
  filesystem reused its inode, an unrelated file took the id of that note,
  maybe an open one. Accepted trade-off: a file that moved into a folder that
  fails, unseen by the walk, gets a fresh id once that folder reads.
  `reidentify`'s reclaim drops any other identity kept on the id, so one id
  keeps one identity.
  One exception to "a folder that fails keeps its entries": a re-key through
  a path that became a hard link maps the inode to that path's id. The entry
  on the file's other path then has no identity, and a root walk that fails
  its folder drops it. The vault collapses a hard link to one entry anyway.
- **Corrected (release figures, 2026-09-26): the full pass continues the root
  pass's listing.** The full pass read the root again, so a vault of 50,000
  notes in its root listed them twice: names were ready at 576–590 ms, against
  487–495 ms before slice 6. `Vault::walk_seeded(fs, root, seed: Walk, jobs)
  -> Walk` is the one addition to the API: it reads every folder `seed` found
  and did not read, and every folder below, on the same parallel jobs as
  `walk` (O4). The worker keeps the root pass's `Walk` and seeds the full pass
  with it, so the full pass reads the root and every folder, and drops the
  identities a full scan drops. It cannot fail, since it reads
  no start folder: a subfolder that fails is left out of `read`, as in `walk`.
  A failed root pass leaves no listing, and the full pass walks from the root.
  The root listing is older than the rest of the full pass by one swap; a
  write in between requests a scan, which lists it.

### O4. The parallel walk

`walk` reads folders on up to `num_cpus` background jobs, each reading one
folder and queueing its subfolders, as Zed's `scan_dirs`. `inkra-vault` has no
executor, so `walk` takes the jobs' runner as a parameter: `pub trait Jobs
{ fn parallelism(&self) -> usize; fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a>; }`.
The app runs them with `BackgroundExecutor::scoped_priority(Priority::Low, …)`
as Zed does; `Serial` runs one job inline for `scan`, `rescan` and tests.
`walk_seeded` (O3, Corrected) runs on the same jobs. If the
serial walk stays, `walk` takes no runner and no trait ships. The walk is kept
only if the `vault` bench shows a gain over the serial walk at 50,000 notes,
warm (cold is Deferred); otherwise it stays serial and the figures go to
Evolution.

- **A panicking job** (review, 2026-09-26): the `Reading` guard keeps it from
  hanging a walk run on threads
  (`a_job_that_panics_ends_the_walk_instead_of_hanging_it`). Under gpui's
  dispatcher, a background panic aborts the app: no `catch_unwind` wraps a
  task, and the macOS dispatcher runs each one through an `extern "C"`
  trampoline (`vendor/zed-reference/crates/gpui_macos/src/dispatcher.rs:198`).

### O5. `scanned`

A vault is `scanned` once a full scan has applied. The first full scan
turns it true, and it stays true.

- `IndexSync::reconcile` returns at once while its vault is not scanned: the
  startup load runs one build, after the full scan.
- `IndexSync::awaits_first_build()` (was `is_scanning`) is true until the
  first build applies, since `IndexSync::is_building` was false before the
  first `reconcile` (slice 6 changed it, see "Built in slice 6"). `reconcile`
  builds nothing before the full scan, so this covers the scan and the one
  build after it: quick open never says "No recent notes" before it can list.
- Quick open, while `awaits_first_build()`, lists nothing and shows "Scanning the
  vault…". Its existing `NotesChanged` subscription then lists the results.
- **The first names list at once** (review, 2026-09-26). The first
  `NotesChanged` while quick open holds `Answered::Scanning` runs a plain
  `Run::Search`, without the refresh's 200 ms wait: the user is waiting, and
  nothing is selected to keep.

### O6. Launch

1. Before the window, a readability check of the root through `Fs`
   (`Fs::check_dir`: `RealFs` opens the folder and lists nothing); on error,
   exit as today. The store gets `Vault::new(root)`, not scanned.
2. `open_window`; the sidebar is empty for one pass.
3. The worker walks the root alone, applies it (the sidebar shows the root
   level), then walks the whole tree and applies it (`scanned` turns true).
4. `IndexSync` reconciles once.

`Launch::Note` (a single file on the command line) is unchanged.

### O7. `reconcile` shares the entries

`reconcile` passes `shared_entries()` to the background build, which derives
`NoteSet::build`'s `(EntryId, String, Kind)` there, and the notes' ids and
mtimes `apply_notes` diffs. The foreground part of a rescan becomes
an `Arc` clone and a spawn. `apply_notes`'s mtime diff reads the shared entries
too.

- **Measured (2026-09-26): `apply_notes` stays on the foreground.** It is the
  startup's longest turn, 5.09 ms at 50,000 notes. A throwaway release test,
  no window, split it at startup: `NoteIndex::set_notes` 0.06–0.12 ms, the
  diff's `retain` calls cost nothing (the maps are empty), and the diff's loop
  3.4–5.0 ms over two runs. On a rescan with nothing changed: `set_notes`
  1.1 ms, the `retain`s 0.8–1.0 ms, the loop 0.4 ms. The loop is no pure
  computation: its cost is 50,000 inserts into `indexed`, `latest`, `queued`
  and `queue`, which grow from empty and rehash as they grow. Saves, loads and
  forgets mutate those maps between a build's start and its apply, so a
  background copy would need a replay log, as O2's re-keys do. The lever,
  taken: `apply_notes` reserves the four maps before the loop, for the notes
  not yet indexed (the build's notes less `indexed`'s live ones), so a
  rescan's full maps do not double. In the same run, the loop took 1.3–1.5 ms
  with the four reserved, against 3.4–3.9 ms without. The orchestrator
  re-measures the final build (Deferred).
- **Corrected (review, 2026-09-26): a save during a build is not reloaded.**
  `apply_notes` leaves a note whose latest seq is newer than the seq the build
  recorded: a save since the build read the vault set a newer mtime, and the
  build's older one would queue a second load. Known cost: a failed load's
  retry after `reconcile` also mints a newer seq, so the build keeps the older
  indexed mtime, and the next build reloads that note once more.

### O8. Errors

A `walk` fails only when its starting folder cannot be read, a start that is
a file or does not exist included. The vault keeps
its state and the error is logged. During the initial scan, `scanned` still
turns true, so the index builds on what is known and quick open never waits
forever. Nothing retries on its own (O3, Corrected): the next requested scan
does.

### O9. The timeline

The startup line becomes: `launch: first frame {t}, root {t}, scanned {t},
names ready {t}, indexed {t} (…)`. It is due once "indexed" and "first frame"
are recorded, as slice 5 settled.

## Built in slice 6

What shipped, per decision:

- **O1.** `Vault` holds one `Arc` of its state. `Vault::new`, `is_scanned`,
  `mark_scanned` and `shared_entries` are new; `scan` and `rescan` stay as
  wrappers, and `collect` and `apply` are gone (O1, Corrected).
- **O2.** `NoteStore`'s scan worker lives in `crates/app/src/vault/scans.rs`.
  Scans are serial, requests coalesce, and re-keys made during a scan replay
  onto its result.
- **O3.** `Walk` and `Vault::applied`, in `crates/vault/src/vault/walk.rs` and
  `vault.rs`. A proptest oracle checks them against the old `apply`.
- **O4.** The parallel walk, behind `pub trait Jobs`. `Serial` runs it for
  `scan`, `rescan` and tests; the app runs it on the background executor.
- **O5.** `IndexSync` builds nothing before the full scan.
  `awaits_first_build()` drives quick open's "Scanning the vault…".
- **O6.** `Fs::check_dir` runs before the window; the root pass and the full
  pass run after it. The full pass continues the root pass's listing
  (`Vault::walk_seeded`), so the initial scan lists the root once.
- **O7.** `reconcile` and the sidebar rebuild read `shared_entries()`.
- **O8.** A failed full pass logs its error and still marks the vault
  scanned; a failed root pass does not.
- **O9.** The timeline gains the `root` and `scanned` steps, and
  `Timeline::turn` counts the worker's swap beside `IndexSync`'s turns.

The parallel walk is kept: the `vault` bench (Performance) shows it 53–54 %
faster nested and within 2 % flat.

Deviations taken during the build:

- **`QuickOpenDelegate` holds an `Answered { Nothing, Scanning, Recent, Query }` enum**,
  not a `scanning` bool beside `searched`. A fourth bool trips clippy's
  `struct_excessive_bools`; the enum replaces `searched` and `query_is_empty` too.
- **`IndexSync::is_building()` is `awaits_first_build() || loaded < total`.**
  No build runs before the full scan, so `loaded < total` alone was false
  during it. A review found two effects: the backlinks panel said "No
  backlinks", and a save during the scan froze a wrong `Indexed` mark.
- **`Stage` releases its read-dir gates on drop.** A walk still held owns a
  vault handle, which gpui reports as leaked when the app drops.

## Performance

| Step | Budget |
|---|---|
| First frame, any vault shape, release | 100 ms |
| Root level shown, 50,000 notes in 100 folders | recorded |
| Root level shown, 50,000 notes in the root | recorded |
| Every foreground turn of the scan worker and of `IndexSync`, rescans included | 4 ms aim, 8 ms hard |
| `walk` serial and parallel, `applied`, at 50,000 notes | recorded; decides O4 |

A new bench, `crates/vault/benches/vault.rs`, times `walk` (serial and
parallel), `applied` and a save's re-key at 50,000 entries. The release
timeline is taken on two generated vaults, the nested one
(`cargo run -p inkra-app --release --example startup_vault`) and a flat one of
50,000 notes in the root, warm, before and after. Cold figures need
`sudo purge`, which the slice's agents cannot run: every slice-6 figure is
warm, and says so (Deferred).

The `vault` bench, Task 6.2 (`cargo bench -p inkra-vault -- --quick`, Apple
M5 Pro, 18 cores, warm cache, `RealFs`, 18 jobs for the parallel walk; two
runs, criterion's estimate):

| Bench | Run 1 | Run 2 |
|---|---:|---:|
| `walk serial nested 50k` | 135.5 ms | 133.7 ms |
| `walk parallel nested 50k` | 62.3 ms | 62.4 ms |
| `walk serial flat 50k` | 127.9 ms | 129.3 ms |
| `walk parallel flat 50k` | 130.2 ms | 127.8 ms |
| `applied from empty nested 50k` | 76.7 ms | 76.9 ms |
| `applied again nested 50k` | 79.0 ms | 79.5 ms |
| `reidentify 50k` | 56.7 µs | 55.9 µs |

**O4 decided: the parallel walk is kept.** Nested, it is 53–54 % faster than
the serial walk (the rule asks 20 %). Flat, it is within 2 % of serial either
way (the rule allows 10 % slower): one folder is one job. A save's re-key
stays well under 1 ms.

`walk` again, with the seeded pass (vault review, 2026-09-26, same machine,
one run of `cargo bench -p inkra-vault -- --quick walk` at `d091879`). Load
average 2.2–2.3 over one minute, no process over 8 % of one core:

| Bench | Figure |
|---|---:|
| `walk serial nested 50k` | 128.3 ms |
| `walk parallel nested 50k` | 62.2 ms |
| `walk serial flat 50k` | 125.0 ms |
| `walk parallel flat 50k` | 125.7 ms |
| `walk seeded flat 50k` | 247.7 µs |

`walk seeded flat 50k` is the initial scan's full pass on the flat vault. The
seed holds the root's listing and the root holds no folder, so the pass reads
nothing: it checks the seed's 50,000 entries for folders and starts its jobs.
Before `walk_seeded`, that pass listed the root again, as `walk parallel flat
50k` does (O3, Corrected).

`reidentify shared 50k` (review of 3591ca4, same machine, two runs of
`cargo bench -p inkra-vault -- --quick reidentify`) times a re-key that changes the mtime while a build holds the entries,
so it copies them: 135.3 µs and 134.9 µs. In the same runs, `reidentify 50k`
took 33.1 µs.

`applied` sorts once (review of 3591ca4, 2026-09-26). It sorted what the walk
found, then every row again, and `compare` split both paths into components
on every comparison. It now sorts the found and kept rows together, splits
each path once, and collapses hard links while it mints. `sort_by_cached_key`
cannot hold a key that borrows its row, so `applied` sorts row indices by the
split paths instead.

Paired figures (vault review, 2026-09-26). "Before" is a detached worktree of
`6bbcde9`, the commit before the change (`193671d`); "after" is this branch
at `d091879`, whose `applied` is the change's. Same machine, same session: two rounds of
`cargo bench -p inkra-vault -- --quick applied`, before then after in each
round. Load average 1.9–2.1 over one minute; no process took over 20 % of one
core, and no game ran.

| Bench | Before | After |
|---|---:|---:|
| `applied from empty nested 50k` | 76.4 ms, 82.4 ms | 31.4 ms, 30.4 ms |
| `applied again nested 50k` | 80.0 ms, 79.9 ms | 31.9 ms, 32.6 ms |

The change makes `applied` 2.4–2.7 times faster. The "before" figures
match Task 6.2's (76.7 ms, 79.0 ms).

The change's first figures, 180.5–195.4 ms before and 69.0–78.2 ms after,
were taken while a game held about 77 % of one core. They compare with no
other figure here. Under that load, two throwaway variants ran once each.
Each run's two figures are most probably `from empty` then `again`, the
bench's order. The single sort alone took 171.4 ms and 179.5 ms. The single sort with a `compare` that
allocates nothing but splits the paths per comparison took 98.8 ms and
108.6 ms.

The release timeline, taken by the orchestrator on 2026-09-26 (Apple M5 Pro,
release, warm cache, four runs each). "Before" is a release build of
`be505c2`. "After" is the slice's build with O7, before commits `b0ae3b8` and
`37d5dfc`.

| Timeline, release, warm | Nested, before | Nested, after | Flat, before | Flat, after |
|---|---:|---:|---:|---:|
| first frame | 382–518 ms | 160–165 ms | 494–501 ms (1.2 s first run) | 161–173 ms |
| root | — | 157–160 ms | — | 352–368 ms |
| scanned | — | 300–305 ms | — | 556–569 ms |
| names ready | 376–510 ms | 324–330 ms | 487–495 ms | 576–590 ms |
| indexed | not taken | 1.2–1.3 s | not taken | 1.4–1.5 s |
| longest turn | 5.7–7.6 ms | 4.8–5.3 ms | 5.3–6.6 ms | 4.6–5.1 ms |

A one-note vault gives the floor: first frame 152–162 ms after, 149–155 ms
before. That is window and GPU start-up, independent of the vault.

A probe build of the same state, on the nested vault, timed each foreground
step: `reconcile` 7.8 µs, the full pass's swap 1.34 ms, the sidebar rebuild
1.25 ms, and `apply_notes` 5.09 ms, the longest turn. O7 (Measured) splits
`apply_notes`: its diff's loop is 3.4–5.0 ms of it, `set_notes` 0.1 ms.

What the figures show:

- **The first frame no longer depends on the vault's size or shape.** It is
  160–173 ms on both vaults of 50,000 notes, within about 20 ms of the one-note
  vault's floor.
- **The 100 ms budget is missed by the platform floor**, not by the vault:
  about 150 ms with a one-note vault, before and after (Deferred).
- **Every foreground turn stays under the 8 ms hard budget.** The longest,
  `apply_notes`, misses the 4 ms aim by about 1 ms (O7, Deferred).
- **The flat vault's names came later than before**, 576–590 ms against
  487–495 ms: the full pass listed the root again. Commit `b0ae3b8` (O3,
  Corrected) seeds the full pass with the root pass's listing, which changes
  the flat vault's `scanned`, `names ready` and `indexed` figures above.
  Commit `37d5dfc` changes no code: `apply_notes`'s figure stands until its
  lever lands. The orchestrator re-measures the final build.

**Measured at the gate, 2026-09-26**, by the orchestrator: the release build
of `92b0ece`, Apple M5 Pro, warm cache, runs 2–4 of 4 through
`/tmp/launch-figure.sh`, measured in one sitting so each vault compares with
the floor taken beside it.

| Vault | First frame | Root | Scanned | Names ready | Indexed | Longest turn |
|---|---:|---:|---:|---:|---:|---:|
| one note (the floor) | 148–160 ms | — | — | — | — | 0.02 ms |
| 50,000 nested | 142–155 ms | 142–151 ms | 241–249 ms | 265–273 ms | 1.1 s | 2.3–2.4 ms |
| 50,000 flat | 151–166 ms | 290–323 ms | 327–360 ms | 349–382 ms | 1.2–1.3 s | 2.3–2.5 ms |

The first frame equals the one-note floor at 50,000 notes, in either shape.
The floor moves between sittings (121–128 ms and 148–162 ms on the same day),
so a first-frame figure is read against a floor taken beside it. Before slice 6,
the same machine gave a first frame of 382–518 ms (nested) and 494–501 ms
(flat), and a longest turn of 5.3–7.6 ms.

## Testing

- **Oracle** (`inkra-vault`, proptest): over random trees and random histories
  of renames, recreations and hard links, a full `walk` then `applied` gives
  the same entries, order and ids as today's `Vault::apply`, kept as a
  test-only reference. A partial `walk` from a random folder, applied, equals a
  full scan of the same tree. A walk from the root with a random folder that
  fails keeps that folder's entries, but a hard-link path a re-key left
  without an identity, and every mapping it keeps names a live entry. 20,000
  cases at the gate.
- A folder that fails to read mid-walk keeps its entries.
- `MemFs::read_dir` gains a gate, as `load` and `save` have
  (`crates/vault/src/mem_fs.rs:32-33`), so an app test can hold a walk.
- `app`, headless against `MemFs`:
  - the window draws with an empty sidebar, then the root level, then the rest;
  - a folder expanded during the scan fills in when the scan applies;
  - a create during the initial scan, held by the gate: the create's id
    survives, and each file keeps one id across the two scans;
  - a save that re-identifies its note during a held scan: the next save
    still finds the note;
  - three requests during a held scan cost one more scan, and each `oneshot`
    resolves after it;
  - a folder renamed during a held walk keeps its notes, then the next scan
    moves them;
  - `⌘P` during the scan shows "Scanning the vault…", then results, with a
    `simulate_keystrokes` test;
  - the startup load runs one index build;
  - every existing test stays green.
- Shots, looked at by the orchestrator: the sidebar with the root level only;
  quick open during "Scanning the vault…".
- Mutants zero missed on the changed files.
- **Known miss (review, 2026-09-26): one `bootstrap.rs` mutant.**
  `delete field window_bounds from struct WindowOptions expression in run`
  survives. `run` starts the platform and opens a real window, so no headless
  test sees the window's bounds. `.cargo/mutants.toml` cannot exclude it:
  cargo-mutants 27.1.0 ignores `exclude_re` for struct-field mutants.

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Reopen the last note | Slice 9, which gives the app a local state folder | Stored per vault in `~/Library/Application Support/Inkra/`, never in the vault: a synced state file conflicts across devices |
| The tree filled level by level | A user opens a folder that stays empty during the scan | Approach 2 of the brainstorm |
| A priority scan of an opened folder | Same trigger | Approach 3; merges a partial scan with the full one |
| Scoped rescans for our own writes | A rescan over its budget on a real vault | A create, rename or trash walks only its folder (O3's `walk` from a folder) |
| Window start-up floor, ~150 ms with a one-note vault | The user notices the launch | Profile gpui window creation, Metal and font loading |
| Cold figures, and O4 decided cold | A session where `sudo purge` can run between launches | The slice-6 figures are all warm |
| `apply_notes`'s diff at startup, 3.4–5.0 ms of foreground before its maps were reserved (O7, Measured) | The final slice-6 figures keep it the longest turn over the 4 ms aim, or slice 8 adds foreground work to it | Lever taken: `indexed`, `latest`, `queued` and `queue` are reserved for the build's notes not yet indexed before the loop, 1.3–1.5 ms in the probe; a rescan reserves only its new notes. The orchestrator re-measures the final build |
| A budget check of a rescan's foreground turn after startup | A stutter on create, rename or trash in a large vault, or before slice 8 adds foreground work per keystroke | The timeline stops counting turns once its line is logged; a bench or test of the swap, the sidebar rebuild and `reconcile` at 50,000 entries would keep it measured |
| Ids that survive a restart | Slice 9's spec, before its format freezes | `EntryId`s are minted per process, so a persisted index keys notes by `(device, inode)`, and `Vault` gains a constructor seeded with those identities so a scan reuses their ids |
| The file watcher | The user edits the vault outside the app and expects it to follow | `notify` 8.2 (stable line), raw events batched every 100 ms as Zed does, no `notify-debouncer-full`, whose time window hides the difference between `MustScanSubDirs` (walk that subtree) and dropped events (full scan). Its licence, CC0-1.0, is not in `deny.toml`: adding it is a decision for that spec. Renames arrive as two events on FSEvents; ids by inode already pair them. Needs scoped rescans first: `Scans` then holds the pending folders, not a plain list of answers, and falls back to a full scan past a few |

## Evolution

Levers the slice-6 review found and left for later. Each keeps today's
behaviour and its tests.

| Lever | Trigger | What changes |
|---|---|---|
| Split `applied` into pure phases | `applied` gains a rule (scoped rescans, the watcher), or a bug in it needs a test of one step | Gathering the rows, sorting them, minting ids and building the maps become pure functions, each tested alone; `applied` chains them |
| One map of per-note state in `IndexSync` | A fifth per-note map, or a bug where `indexed`, `latest`, `queued` and `queue` disagree on a note | One `HashMap<EntryId, NoteState>` replaces `indexed`, `latest` and `queued`; `queue` keeps the load order; `apply_notes` reserves two maps, not four |
| One `walk` entry point | A third way to start a walk, such as the watcher's batch of folders, or a change made twice in `walk` and `walk_seeded` | `walk` takes `From::Folder(RelPath)` or `From::Seed(Walk)`; `walk_seeded` goes |
| `applied(&Walk)`, dropping the seed clone | The root pass's clone of its `Walk` shows in a profile of the flat vault, 50,000 `DirEntry`s | `applied` borrows the walk, so the worker applies the root pass and moves the same `Walk` into the seeded pass |
| One place per `Timeline` step | The next step the launch line gains | Adding a step touches five places today: the `Step` variant, the field, `new`, `record`'s arm and `line`. One table of steps makes it one |

## Review

A red team, a blue team and a researcher (sonnet) reviewed the first draft on
2026-09-25. What changed:

- **Corrected.** The draft let scans run concurrently and dropped stale ones
  by generation, as `IndexSync` drops a stale build. `IndexSync` mints no ids;
  a vault scan does, so two scans from one starting state would mint diverging
  ids for the same file. Scans are now serial (O2).
- **Corrected.** A `read_dir` that failed mid-walk (a folder renamed during
  the scan) silently dropped the subtree. A scan now replaces only the folders
  it read (O3).
- **Corrected.** `reidentify` during a scan was left to the plan; the worker
  replays it (O2).
- **Corrected.** "No new mechanism" for "Scanning the vault…" was false:
  `is_building()` is false before the first `reconcile`. `is_scanning()` is new
  (O5).
- **Corrected.** A vault with its notes in the root made the root read, before
  the window, as slow as the whole scan. The first frame now reads nothing
  (O6), and the flat vault is measured.
- Taken from the researcher: the parallel walk (O4), and the `walk`-from-a-
  folder seam for the watcher (O3).
- Taken from the blue team: `scan`, `rescan` and `apply` stay as wrappers
  (O1); requests coalesce, so a burst costs two scans (O2).

