# Scan session: the scan worker's rules move into `inkra-vault`

Status: **done** (2026-09-29). A refactor: behaviour stays identical.

## Why

`NoteStore`'s scan worker (`crates/app/src/vault/scans.rs`, 310 lines) and its
reclaim pass (`gone.rs`) assemble a dozen `Vault` primitives in an order that
only the app knows:

- `start_scan` sets `running` and clones the base in the same turn
  (`scans.rs:148-150`). A re-key after that turn is recorded, not applied.
- `reidentify` records a re-key during a scan, with `in_place` computed
  against the live vault, or applies it now (`scans.rs:109-136`).
- `swap` (`scans.rs:231-285`) runs `reclaim_gone` first, then
  `replay_reidentify` for each recorded re-key, then `mem::replace`, then
  `check_open_notes`. Its error branch (`:247-265`) replays the same re-keys
  with `reidentify` instead, and marks the vault scanned on the full pass
  only (open-first O8).
- `scan_ended` (`:288-309`) answers the waiters, then picks the next scan
  from `Next`, `Pending` and the `incomplete` flag (watcher spec, F4).
- The background half picks `walk_seeded`, `walk_changed` or `walk` from the
  seed, `Pending` and `has_read_whole` (`:194-213`).

None of it needs gpui, yet every test of it is a `#[gpui::test]` that holds a
walk with a `MemFs` gate and runs the executor until parked. The rules are
spread across two crates: the vault rule file has to say "the swap reclaims
before it replays, through `replay_reidentify`, never `reidentify`", because
nothing in the types enforces it.

A second, smaller finding: `inkra_vault::Watcher` (`fs.rs:73`) is an empty
marker trait. Its only use is `Box<dyn Watcher>` in `Watch`, to keep the
platform watch alive until the store drops it.

## What Zed does

`Worktree` hands every change to its `BackgroundScanner`, which owns the one
mutable snapshot (`vendor/zed-reference/crates/worktree/src/worktree.rs:4339`).
A save does not patch the foreground snapshot: `refresh_entry` (`:2149`) sends
a `ScanRequest` (`:163`) and waits for the scanner to apply it. The scanner
merges queued requests into one (`next_scan_request`, `:6280-6287`) and sends
snapshots back to the foreground, which only calls `set_snapshot` (`:1443`).
Serial scans and coalesced requests follow from the single owner, so Zed has
no replay log.

`Fs::watch` returns a stream and an `Arc<dyn Watcher>` (`fs/src/fs.rs:74-77`).
Zed's `Watcher` has `add` and `remove`, with a real and a fake implementation.

**Deviation, kept from open-first O2:** the foreground `Vault` stays the owner.
A save re-keys the live vault at once, so the next save and `open_note` find
the note in the same turn, without a round trip to a worker. The price is the
replay log, which this spec puts behind one type.

**Deviation:** our `Watcher` has no method: a vault watches its root once and
never adds a path. A trait with no method is not a seam, so it becomes a drop
guard type.

## The deepened module: `inkra_vault::scans`

New files: `crates/vault/src/scans.rs`, `scans/run.rs`, `scans/claims.rs`,
and their tests. It sits beside `watch.rs`, whose `Pending` it consumes. It
lives in `inkra-vault` because every type it touches (`Vault`, `Walk`, `Fs`,
`Jobs`, `Pending`) lives there, and it needs no clock, no `Window` and no gpui.
`futures::channel::oneshot` is already a dependency of the crate.

### What it hides

- The coalescing state: the running scan, the next scan (`Next`), and the
  waiters of `rescan()`.
- The re-key log, and the rule "record during a scan, apply otherwise".
- The swap's order: reclaim, replay, replace. The error path's order: replay
  with `reidentify`, then O8's mark on the full pass only.
- The choice of walk, and the `changed` comparison off the foreground.

### Interface

```rust
/// The worker's state. `Default` is idle.
pub struct Scans { running: Option<Session> }

/// A pass to run off the foreground: its base and what it reads. `Send`.
pub struct Scan { base: Vault, read: Pending, seed: Option<Walk> }

/// A walk's result, applied onto the base off the foreground.
pub struct Scanned { result: Result<(Vault, bool /* changed */, bool /* incomplete */)> }

/// The root pass's listing, and the scan it continues.
pub struct Listed { applied: Result<Vault>, scan: Scan }

/// What the open notes hold at a swap (watcher spec, F9).
pub struct Claims {
    /// Every open id: a reclaim never takes one from its note.
    pub open: HashSet<EntryId>,
    /// Notes that may take back the note at their last path; the path when
    /// the caller keeps one (`Location::Gone`), else the live vault's.
    pub claimants: BTreeMap<EntryId, Option<PathBuf>>,
}

/// What a swap did, for the caller's notify and open-note check.
pub struct Swapped {
    /// The result swapped in; `false` when the walk failed.
    pub applied: bool,
    /// The live vault changed: the caller notifies its entity.
    pub notify: bool,
    /// The replaced state, for `check_open_notes` and a background drop.
    pub previous: Option<Vault>,
}

impl Scans {
    pub fn start(&mut self, live: &Vault) -> Option<Scan>;          // initial: Everything
    pub fn request(&mut self, live: &Vault) -> (oneshot::Receiver<()>, Option<Scan>);
    pub fn batch(&mut self, batch: Pending, live: &Vault) -> Option<Scan>;
    pub fn reidentify(&mut self, live: &mut Vault, id: EntryId,
                      identity: FileIdentity, mtime: Mtime) -> bool; // notify
    pub fn swap_root(&mut self, live: &mut Vault, listed: Listed,
                     claims: &Claims) -> (Swapped, Scan);
    pub fn swap(&mut self, live: &mut Vault, scanned: Scanned,
                claims: &Claims) -> (Swapped, Option<Scan>);
}

impl Scan {
    /// No full scan has applied: the root pass runs first (open-first O6).
    pub fn is_initial(&self) -> bool;
    pub async fn list_root(self, fs: &dyn Fs) -> Listed;
    pub async fn walk(self, fs: &dyn Fs, jobs: &dyn Jobs) -> Scanned;
}
```

`Session` (private, was `Running`) holds `next: Next`, the waiters the running
scan answers, and `rekeys: Vec<Rekey>`. `Next` and `Rekey` keep their current
shape and stay private. The names above are the spec's. The implementer may
rename a type if clippy or a clearer name asks for it, and records the change
in "Built".

### Invariants, each held by the types

1. **A base exists only with a session.** `Scan` has no public constructor.
   `start`, `request`, `batch`, `swap_root` and `swap` return one only in the
   call that sets or keeps `running`. So the base is cloned in the turn that
   starts recording, as `scans.rs:149` states in a comment today.
2. **`Scans` never holds the base.** The base moves into the `Scan`, and the
   background task drops it with the walk. A held clone would share the live
   state, so the error path's `reidentify` would copy both 50,000-entry maps
   on the foreground (open-first O2, "A re-key during a scan only records").
3. **A re-key during a scan only records.** `Scans::reidentify` computes
   `in_place = live.id_for(identity) == Some(id)` at the record, as today, and
   returns `false`. With no scan, it calls `Vault::reidentify` and returns its
   result.
4. **The swap's order is one function body.** On success: reclaim against
   `claims`, in `BTreeMap` (id) order, then `replay_reidentify` for each
   re-key in record order, then `mem::replace`. `notify` is
   `changed || reclaimed || any replay moved`. On failure: `reidentify` for
   each re-key onto `live`, no reclaim, no `previous`.
5. **O8 by method, not by flag.** `swap` marks a live vault that is not
   scanned yet after a failed walk. `swap_root` never marks. The `Pass` enum
   goes.
6. **The root swap rebases in the same call.** `swap_root` takes the re-keys,
   swaps, and returns the full pass's `Scan`, based on `live` after the swap
   and carrying the root listing as its seed. It always sets `notify` on
   success, as `swap(..., true, Pass::Root)` does today.
7. **The full swap ends the scan.** `swap` answers the waiters only when
   `applied`; a failed walk drops them, so each receiver gets `Canceled`. It
   then picks the next scan as `scan_ended` does: `Batch(_)` after an
   incomplete walk becomes `Batch(Everything)`, an empty `Batch` starts
   nothing, and anything else starts.
8. **The walk's choice is `Scan::walk`'s.** With a seed: `walk_seeded`.
   `Folders` on a base that `has_read_whole`: `walk_changed`. Otherwise a
   full `walk` from the root, whose error is the `Scanned` error. Then
   `applied`, `mark_scanned`, and `changed = next != base`, all off the
   foreground (O1).
9. **A claimant's path resolves against the live vault.** A `None` path in
   `Claims.claimants` resolves to `live.absolute(id)`, on the live vault
   before the swap, inside `inkra_vault::scans`. It never resolves against
   the new state, which has no entry for a gone id. Only then does the path
   match against the new state's entries.

### The reclaim rule moves with the swap

`NoteStore::reclaim_gone` becomes a private function of `scans/claims.rs`
over `Claims`, with the same rule: each claimant with no entry in the new
state takes its last path, from `Claims` or `live.absolute(id)`, and reclaims
the note there unless that id is in `open`. `EntryId::DETACHED` never claims:
`reclaim` refuses it, and `live.absolute(DETACHED)` is `None`. The store keeps
the policy it alone knows: `gone.rs` shrinks to `fn claims(&self) -> Claims`.
A claimant is an open note, other than `DETACHED`, without `ops.trashing`.
`open` is every key of `self.open`.

### Narrowed primitives

Once `scans` is their only caller outside `inkra-vault`, `replay_reidentify`,
`reclaim`, `id_at`, `walk_changed`, `mark_scanned` and `has_read_whole` become
`pub(crate)` where the compiler finds no other caller. `reidentify`,
`walk_seeded` and `walk` stay `pub`: the bench and `crates/vault/tests` call
them. The app can then no longer run the swap's steps in the wrong order.

## `Watcher` becomes a drop guard

```rust
/// Dropping it stops the watch and ends its stream.
pub struct Watcher(Box<dyn Any + Send + Sync>);
impl Watcher { pub(crate) fn new(held: impl Any + Send + Sync) -> Self; }
```

`Watch.watcher` holds a `Watcher`, not a `Box<dyn Watcher>`. `RealFs` passes
its `RecommendedWatcher`, and `MemFs` passes the `Arc<UnboundedSender>` that
`MemWatcher` wraps today, so `MemWatcher` and the `impl Watcher for ...` lines
go. The test's `Unwatched` becomes `Watcher::new(())`. The re-export in
`lib.rs` stays.

## What moves where

| From | To |
|---|---|
| `app/src/vault/scans.rs`: `Scans`, `Running`, `Next`, `Rekey`, the `Pass` enum, `swap`'s body, `scan_ended`'s choice, the background walk choice | `vault/src/scans.rs` and `scans/run.rs`; `Pass` goes |
| `app/src/vault/gone.rs`: `reclaim_gone` | `vault/src/scans/claims.rs`, over `Claims` |
| `vault/src/fs.rs`: `trait Watcher` | `struct Watcher`, same file |

The app keeps, in `scans.rs`: `Background` (the gpui `Jobs`), the spawn of
each `Scan` on the background executor, the two `timed` turns, the
`Timeline` marks for `Root` and `Scanned`, `vault.update` with `cx.notify()`
when `notify`, `check_open_notes` with `previous`, and the background drop of
`previous`. `rescan()` with no vault still returns a receiver whose sender is
dropped. The file should fall from 310 lines to about 120.

## Tests

The new interface is the test surface. A held walk becomes plain sequential
code: the test calls `start`, changes the disk, records re-keys, then runs
`scan.walk(&fs, &Serial)` under `block_on`, then `swap`. New file
`crates/vault/src/scans/tests.rs` (split per concern if it passes ~400 lines),
`#[test]` against `MemFs`, plus one proptest.

Each row names the behaviour, its success criterion, and the gpui test it
replaces. "Kept" means the gpui test exercises wiring and stays.

| Behaviour | Criterion (new vault test) | gpui test |
|---|---|---|
| Three requests during a scan cost one more scan | `request` ×4: only the first returns a `Scan`; its `swap` answers the first receiver alone and returns one `Scan`; that swap answers the other three | `three_requests_during_a_held_scan_cost_one_more_scan` moves |
| Requests during the initial scan cost one more scan, no second root pass | `start` then `request` ×3: `swap_root`, then `swap` return one more `Scan`, not `is_initial`; `read_dirs("/vault") == 2` over both | `requests_during_the_initial_scan_cost_one_more_scan` moves |
| Batches during a scan merge into one scoped scan | `request`, `batch` ×3 over two folders: `swap` returns a `Scan` reading both folders; running it lists each once | `batches_during_a_scan_cost_one_more_scan` moves |
| An incomplete scoped read is followed by one full scan | a `batch` on an unreadable folder: `swap` returns a `Scan` reading `Everything`; that scan's `swap` returns `None` | `a_scoped_read_that_fails_…` and `…_keeps_failing_costs_one_full_scan` move |
| A batch after a failed initial scan reads the whole vault | initial scan on an unreadable root, then `batch`: the `Scan` walks from the root | `a_batch_after_a_failed_initial_scan_reads_the_whole_vault` moves |
| The initial scan lists the root once | `start`, `list_root`, `swap_root`, `walk`, `swap`: `read_dirs` is 1 per folder; `live.is_scanned()` | `the_initial_scan_lists_the_root_once` moves |
| A failed root pass does not mark; a failed full pass does (O8) | unreadable root: after `swap_root`, `!is_scanned`; after `swap`, `is_scanned` and `applied == false` | `a_failed_root_pass_leaves_the_vault_unscanned_until_the_full_pass` moves; `a_failed_root_pass_marks_no_root_step` kept (Timeline) |
| A re-key during a scan only records | `reidentify` during a session returns `false` and leaves `live` equal to the base | new |
| A save re-keyed during a scan keeps its id | save, `reidentify`, walk after the save, `swap`: the entry keeps `a`, `id_for(saved) == Some(a)` | `a_save_re_keyed_during_a_held_scan_is_found_by_the_next_save` moves |
| An in-place save keeps the save's mtime | `insert_file` in place, `reidentify` during the session, `swap`: the entry's mtime is the save's | `an_in_place_save_re_keyed_…_keeps_the_saves_mtime` moves |
| A failed walk replays onto the live vault and answers nobody | `request`, save, `reidentify`, unreadable root, `swap`: receiver `Canceled`, `id_for(saved) == Some(a)` | `a_failed_walk_answers_no_request_and_replays_…` moves |
| A failed walk whose replay reclaims notifies | as the current test's disk history, then `swap` of a failed walk: `notify` | `a_failed_walk_that_replays_a_reclaim_notifies` moves |
| A `DETACHED` re-key gives no entry its id | `reidentify(DETACHED, …)` during a session, `swap`: no entry carries `DETACHED` | `a_detached_note_inside_the_vault_gives_no_entry_its_id` moves |
| An echo that changes nothing does not notify | walk with no change: `notify == false`, `previous` is `Some` | new; `an_echo_read_after_the_saves_re_key_does_not_notify_the_vault` kept |
| Reclaim before replay (F9) | a claimant gone at the walk, a save to its path replayed: the row takes the claimant's id, the replay changes nothing | new |
| Claims in id order; an open id is never taken | two claimants with one last path: the lower id reclaims; a path held by an open id: no reclaim | new; `two_gone_notes_…_the_lower_id_reclaims_it` and `an_open_note_moved_over_…` kept |
| Proptest: the session equals the serial history | random saves interleaved with one scan, recorded or applied: after `swap`, each saved inode maps to its note's id, and `assert_consistent` holds | new |

Kept in `app/src/vault/tests/scans.rs`, because each crosses a store
operation, the save path, `IndexSync`, `Timeline` or `Background`:
`a_create_during_a_held_scan_…`, `a_create_during_the_initial_scan_…`,
`a_scan_that_swaps_in_between_a_save_and_its_re_key_…`,
`a_folder_renamed_during_a_held_walk_…`, `a_scans_swap_counts_in_the_longest_turn`,
`a_save_during_the_initial_full_pass_keeps_its_note`,
`a_failed_root_pass_marks_no_root_step`, `a_scan_reads_folders_on_several_jobs`.
`rescan.rs` stays whole: its three tests cover the save path's `rescan` and
`reidentify` calls. `watch.rs` keeps `a_rescan_event_runs_one_full_scan` and
the other tests not listed above.

Gates: `./scripts/test-names.sh` before and after, diffed; every name that
leaves the list appears in the "moves" column above. `cargo mutants` on
`crates/vault/src/scans*` with zero missed. `./scripts/check.sh` green.

## Risks

- **A foreground copy on the error path.** If any clone of the base outlives
  the walk, `reidentify` on `live` copies the maps (invariant 2). The
  `reidentify shared 50k` bench shape shows the cost. The review checks that
  `Scans` stores no `Vault`.
- **An extra turn.** Today the root swap and the full pass's base happen in
  one `this.update` closure. `swap_root` keeps that by returning the `Scan`;
  the app must not split the call across two updates.
- **Borrowing in the app.** `swap` needs `&mut self.scans`, `&mut Vault` from
  `vault.update`, and `Claims` from `self.open` and `self.ops`. Build `Claims`
  first, then call `vault.update(cx, |live, cx| self.scans.swap(...))`; the
  fields are disjoint.
- **Lost gpui coverage.** A moved test no longer proves that the store calls
  the session. The kept tests cover each call site: `rescan`, the watch batch,
  the save's `reidentify`, the initial scan, and the reclaim.
- **A save's re-key during a scan takes `vault.update`.** Today the
  record-only path reads the vault with `vault.read`; with one
  `Scans::reidentify(&mut self, live: &mut Vault, ...)` it goes through
  `vault.update`, which adds gpui's lease on the entity. Accepted: a save
  outside a scan pays the same `vault.update` today, the call is O(1), and
  recording copies no map (invariant 2). Splitting the signature would make
  the app ask whether a scan runs, which is the rule this module hides.
- **`Claims` allocates per swap.** Open notes number in tens, so two small
  maps per swap cost nothing next to the walk.

## Plan

Each step builds, passes `./scripts/check.sh`, and is one commit.

1. **`Watcher` as a drop guard.** Replace the trait with the struct, update
   `RealFs`, `MemFs` and the watch test. Criterion:
   `dropping_the_watcher_ends_the_stream` (`mem_fs/watch_tests.rs`) and the
   watch tests in `vault/tests/real_fs.rs` stay green.
2. **The re-key log and the swap.** Add `scans.rs` with `Scans`, `Session`,
   `Rekey`, `Claims`, `Swapped`, `reidentify`, `swap_root` and `swap`, and
   move `reclaim_gone` into `scans/claims.rs`. The app's `Running` holds a
   `Scans` session; `Next` stays in the app for now, and `swap` returns no
   next scan yet. Move the re-key, failed-walk, `DETACHED` and O8 tests;
   write the reclaim-order tests first, red against a stub.
3. **Coalescing and the walk.** Move `Next`, the waiters, `scan_ended`'s
   choice, and the background walk choice into `Scans` and `Scan`. The app's
   `scans.rs` shrinks to spawning, `timed`, the marks and the wiring. Move
   the request, batch, incomplete and initial-scan tests; add the proptest.
4. **Narrow and document.** Narrow the primitives to `pub(crate)`. Update
   `.claude/rules/vault.md` (the swap's order now lives in `Scans::swap`),
   CLAUDE.md's spec list, and this spec's status and "Built" section. Run
   `cargo mutants` on the new files and `test-names.sh`. In the same commit,
   update the two done specs that describe the moved code as present fact:
   the file-watcher spec's `Watch` code block (F1, the `Watcher` trait) and
   its `scans.rs` line citations (F2, F7's "The swap detects" and "Our own
   save's echo") point at `Scans`, `Watcher` and `crates/vault/src/scans.rs`;
   open-first's O2 line says the worker's rules live in
   `crates/vault/src/scans.rs` and `NoteStore` keeps spawning and wiring.

## As built

The code follows the plan. The deviations, each with its reason:

- **Steps 2 and 3 are one commit.** The app switched to `Scans` once. An
  intermediate app-side `Next` would have been written only to be deleted.
- **No `start`.** The initial scan is `batch(Pending::Everything, live)`, the
  call `NoteStore::new` already made through `start_scan`.
- **`Listed` does not hold the `Scan`.** It holds `result: Result<(Vault,
  Walk)>` and `read`. The base drops in the root pass's background task, as
  before, so a failed root pass replays onto a live vault that shares no map
  (invariant 2).
- **`Swapped` is `{ notify, replaced: Result<Vault> }`.** One field replaces
  `applied` and `previous`, and it carries the walk's error. `inkra-vault`
  prints nothing, so the app logs it (`NoteStore::swapped`).
- **`Scan` holds a private `Pass { Root, Full(Option<Walk>) }`**, not
  `seed: Option<Walk>`. After a failed root listing the full pass has no seed
  and is not initial; a seed alone cannot say both. The app's `Pass` enum is
  gone: O8 is by method, as planned.
- **The waiters are answered inside `Scans::swap`**, before the store's
  `check_open_notes`; before, they were answered after it. Both run in the
  same foreground turn, so every woken task sees the checked notes.
- **The app does not filter `DETACHED` out of the claimants.** Its path is
  `None`, `live.absolute(DETACHED)` is `None`, and `Vault::reclaim` refuses
  it, so it never claims.
  `a_detached_note_inside_the_vault_gives_no_entry_its_id` passes it as a
  claimant with a path.
- **`vault::test_helpers` is `pub(crate)`**, so the proptest reuses
  `assert_consistent`.
- **Narrowed to `pub(crate)`:** `has_read_whole`, `mark_scanned`,
  `replay_reidentify`, `id_at`, `reclaim`, `walk_changed`.
- **`app/src/vault/scans.rs` is 158 lines**, not about 120: `swap_root`,
  `swap` and `swapped` each keep one `vault.update` and the notify.

Tests, against the table:

- `a_failed_walk_that_replays_a_reclaim_notifies` became
  `a_failed_walk_whose_replay_moves_an_id_notifies`. Since F5 the gpui
  version's replay moved no id; the vault test sets up a gone note written
  back under a new id, so the replay moves one.
- `a_save_re_keyed_during_a_scan_keeps_its_id` walks before the save, not
  after: then only the replay knows the new inode.
- "Reclaim before replay" is `a_gone_notes_save_during_a_scan_takes_back_its_id`.
  No state-level case found distinguishes reclaim-then-replay from
  replay-then-reclaim; `replace`'s body holds the order.
- Added: `an_empty_batch_starts_no_scan`,
  `a_batch_during_a_scan_that_a_request_follows_joins_the_request`,
  `a_re_key_with_no_scan_applies_now`,
  `a_re_key_with_no_scan_says_whether_an_id_moved`,
  `a_claimant_with_no_path_takes_it_from_the_live_vault` (invariant 9), and,
  to kill two surviving mutants, `a_reclaim_alone_notifies` and
  `a_batch_after_the_initial_scan_reads_only_its_folders`.
- `cargo mutants --no-config --gitignore false -p inkra-vault
  --test-package inkra-vault --in-diff <arch/deepening diff>`: 47 mutants,
  27 caught, 20 unviable, 0 missed.
