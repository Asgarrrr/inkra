# Coalesced reconcile: one build at a time, none for an mtime

Status: **implemented (slice A: R1; slice B: R2, R3). Performance figures measured in the watcher slice.**

Item 1 of the roadmap's "Index and search: the 2026-09-27 audit". It is the
file watcher's prerequisite: the watcher turns every burst of filesystem events
into a scan swap, and every swap into a `reconcile`.

## Why

`IndexSync::reconcile` (`crates/app/src/index.rs:186` before this spec, now
`index/building.rs`) runs on every notify of
the `Vault` entity. The vault notifies when a scan swaps in: after a create, a
rename, a trash, and, once the watcher lands, after each batch of events. A
save does not notify: `Vault::reidentify` reports only an id that moved.

Each notify costs the whole vault, even when one mtime moved:

- `Built::new` runs `NoteSet::build` over every entry, off the foreground:
  40–106 ms at 50,000 notes (audit figure).
- `apply_notes` walks every note's mtime on the foreground, and
  `NoteIndex::set_notes` swaps the set: two turns of 1.4–4.9 ms and
  4.2–4.8 ms (audit figure).

And a notify during a build **drops** that build: `generation` is bumped, and
`apply_notes` discards any build that is not the latest
(`a_stale_build_is_dropped`, `index/tests.rs:146`). Under a stream of notifies
closer together than one build — a watcher batching every 100 ms, a sync client
downloading a folder — no build ever applies. Names, backlinks and the reload
of changed notes stall until the stream stops. The search spec recorded this as
"Coalesced rebuilds" (`2026-09-24-search-design.md:1222`) with the watcher as
its trigger.

## What Zed does

Zed's worktree hands its consumers a delta, `UpdatedEntriesSet`
(`vendor/zed-reference/crates/worktree/src/worktree.rs:4040`): each changed
path with a `PathChange`. Consumers never rebuild from the whole snapshot.

We take the idea, not the type. `Vault` keeps publishing whole snapshots
(`Arc<[Entry]>`, cheap to share), and `IndexSync` derives its own delta off the
foreground by comparing two snapshots. Reason: one consumer needs a delta
today, and the vault's `applied` would need a second output for it. When a
second consumer needs one (the watcher's open-buffer reload may), the diff
moves into `inkra-vault`.

## Decisions

### R1. One build in flight; a notify during it marks the index dirty

`IndexSync` holds `build: Build` in place of `generation`, with
`enum Build { Idle, Running, Dirty }`. Two bools would also allow "dirty while
idle", and clippy's `struct_excessive_bools` rejects a fourth bool on the struct.

- `reconcile` with a build in flight sets `Dirty` and returns.
- A build that lands **always applies**. It read the vault after the index's
  current state did, so it is never older than what it replaces.
- After it applies, if the state was `Dirty`, `reconcile` runs again from the
  vault's current entries.

So every build applies, and a change is in the index at most two builds after
its notify.

`built: Option<u64>` becomes `note_sets: u64`, the number of `Layout` builds
applied. It must stay a counter, not a flag: `failed`
(`crates/app/src/index/loading.rs:133-144`) retries a load when a build applied
since the load started (`Landed.note_sets`), because a build may have moved the
note and a rename keeps its mtime. `a_load_failing_after_a_build_is_retried_at_the_same_path`
(`index/loading_tests.rs:244`) holds that. An `Mtimes` apply (R2) never bumps
`note_sets`: a rename changes a path, so it always arrives as `Layout`.
`awaits_first_build` reads `last.is_none()` (R2).

Applying a build that is already behind the vault is safe with the current
guards:

- A save newer than the build's read is kept by the `seq` check in
  `apply_notes` (`latest > seq`), as today.
- A note trashed after the build's read is named for one more build. Its load
  fails, and `forget` drops its links. The dirty rebuild then removes its name.

A failed load records the note in `forgotten: HashSet<EntryId>`. Every apply,
`Layout` or `Mtimes`, queues again each forgotten note still in the vault,
through the same check as any change; a successful load clears it, and a
`Layout` without it drops it. Before R2 every notify rebuilt and so retried
it; an `Mtimes` diff lists only mtimes that moved, and a note that failed on a
transient error keeps its mtime. Cost: O(forgotten), normally zero. The retry
reads the live vault's `by_id`, since the snapshot has no id index.

### R2. An mtime-only change skips `NoteSet::build`

`IndexSync` keeps `last: Option<Arc<[Entry]>>`, the vault entries the last
applied change was computed from. It is `None` until the first build applies,
and it is replaced by every apply, `Layout` or `Mtimes`; the replaced snapshot
is freed on the background, as `set_notes`'s previous set is. `reconcile` sends
`last` and the vault's current entries to the background, which returns one of:

- `Change::Mtimes(Vec<(EntryId, Mtime)>)`: `last` is `Some`, and after dropping
  every `Kind::Dir` entry from both snapshots, the notes have the same length
  and, at every position, the same `id` and `path`. The vector holds the notes
  whose mtime differs; it may be empty.
- `Change::Layout(Box<Built>)`: anything else, built as today (boxed: clippy's
  `large_enum_variant`).

`last: None` always gives `Layout`. Without it, a vault with no notes (only
`image.png`: `read_dir` keeps `.md` files only) would diff empty against empty,
never apply a build, and leave quick open on "Scanning"
(`a_vault_with_no_notes_is_indexed_once_its_build_applies`,
`index/startup_tests.rs:38`).

Folders are dropped because neither `NoteSet::build` (`note_set.rs:50`) nor
`Built::new`'s mtimes read them: an empty folder created or trashed is not a
rebuild. A folder renamed moves its notes' paths, so it is still `Layout`. Note
order is compared because `NoteIndex::set_notes` reorders the corpus by it.

The comparison is O(n) off the foreground and reads no file.

Applying `Mtimes` does not call `set_notes`, does not emit `NotesChanged`, and
does not bump `revision` by itself. `apply_change` dispatches on `Change`; for
each `(id, mtime)` both paths run `queue_changed`, the check `apply_notes` ran
before: skip when `indexed[id] == mtime` or a newer write is
known (`latest > seq`), else record the mtime and `queue_load`. The load then
bumps `revision` when it applies, as every load does. Foreground cost:
O(changed notes).

Why this is enough: the corpus keeps a loaded note's text and mtime across a
`set_notes` (`crates/index/src/corpus.rs:59`), and `NoteSet`'s mtimes only
seed notes not yet loaded. A rebuilt `NoteSet` with the same ids, paths and
order carries nothing new.

A re-keyed entry (`reidentify` moved an id) differs by `id`, so it takes the
`Layout` path.

An mtime that moved back (a sync client restoring an older date) differs, so
the note reloads, as in today's `apply_notes`. Ranking reads the corpus's
`modified`, which only a load writes (`corpus.rs:37-47`, `find/rank.rs:47-57`),
so skipping `set_notes` leaves no stale mtime in ranking.

### R3. Module

The comparison and `Change` go in `crates/app/src/index/diff.rs`, with its
tests in `index/diff_tests.rs`. `index.rs` was 402 lines after slice A, past
the split threshold, so slice B first moved `Built`, `Build`, `reconcile` and
`apply_notes` into `index/building.rs` as a pure move; the dispatch on `Change`
lives there. `index.rs` is about 290 lines. No API is added to `inkra-vault` or `inkra-index`.

Since 2026-09-29 the comparison, `Build` and the apply live in
`crates/index/src/live/{diff,builds}.rs`, behind `inkra_index::LiveIndex`
(index scheduler spec); `IndexSync` keeps the spawns.

## Testing

Written before the code. Each of the first two fails on `main`.

1. **Starvation.** An indexed vault; a note's text changed through `MemFs`;
   a rescan swapped in; then, while each build is still in flight, another
   swap, several times. Assert that the note's new links appear before the
   stream stops. On `main` every build is dropped and they never do.
   A build is held in flight by a `#[cfg(test)] hold:
   Option<Shared<oneshot::Receiver<()>>>` on `IndexSync`, awaited in
   `reconcile`'s background task before any work (the diff, or `Built::new`), as `quick_open`'s scan gate
   does (`quick_open/mod.rs:55-60`, `:319-320`). Not `executor().tick()`: it
   steps whatever the scheduler picks next, not a chosen build.
2. **Mtime fast path.** An indexed vault; one note changed through `MemFs`;
   a rescan that changes no name. Assert: no `NoteSet` built (`note_sets`
   unchanged; `reconciles` counts every diff started), exactly that note
   reloaded,
   its new links indexed, `NotesChanged` not emitted.
3. **Layout still rebuilds.** Create, rename and trash each count one build
   and emit `NotesChanged`. The existing `index/tests.rs` cases for them stay
   green unchanged. An empty folder created counts no build; a folder renamed
   counts one.
4. **Bootstrap.** A vault with no notes applies exactly one build, and
   `awaits_first_build` clears (`startup_tests.rs:38` stays green).
5. **Rename retry.** `a_load_failing_after_a_build_is_retried_at_the_same_path`
   stays green unchanged; `a_load_failing_after_only_mtime_changes_forgets_the_note`
   asserts a load failing while only an `Mtimes` apply landed forgets instead
   of retrying at once, and
   `a_forgotten_note_is_reloaded_at_a_rescan_moving_another_notes_mtime`
   asserts the next `Mtimes` apply brings it back.
6. **Diff unit tests** in `diff_tests.rs`: `last: None` → `Layout`; same
   snapshot → empty `Mtimes`; one mtime → that id; a folder added or removed →
   `Mtimes`; a note path, a note id, a note count or a note order change →
   `Layout`. A proptest over random edits of a snapshot checks that `Mtimes`
   is returned exactly when the notes' ids and paths are equal position by
   position. A second proptest,
   `mtimes_only_when_a_build_would_make_the_same_names`, checks the claim R2
   rests on through the real build path: whenever `Mtimes` is returned, a
   `NoteSet` built from either snapshot lists the same names in the same order.
7. **Changed tests, stated here so no reviewer reads them as a regression:**
   - `a_stale_build_is_dropped` is replaced by
     `a_build_landing_behind_the_vault_applies_then_rebuilds`: the old
     invariant is the starvation cause.
   - `a_rescans_own_turn_counts_in_the_longest_turn` (`startup_tests.rs`),
     `the_startup_load_runs_one_build` (`scanning_tests.rs`),
     `a_rebuild_alone_is_not_building` (`tests.rs`) and `land_at_once`
     (`loading_tests.rs`) read `generation` or `built`; they move to `build`,
     `note_sets` and the `reconciles` counter.

`./scripts/test-names.sh` before and after, since tests move files.
`cargo mutants` on the diff module: it is logic in `app`, but pure.

## Performance

Measured the way the search spec's slice 4 figures were: a generated vault of
50,000 notes on disk, a release build, the timeline log. One external edit
followed by a rescan, before and after:

| Figure | Before (audit) | Target |
|---|---|---|
| Background work per mtime-only rescan | 40–106 ms | under 2 ms (the diff) |
| Foreground work per mtime-only rescan | 1.4–4.9 ms + 4.2–4.8 ms | under 0.1 ms |
| Builds applied under a notify every 50 ms for 2 s | 0 | at least one per two notifies' worth of builds |

A layout change keeps today's cost. Making it incremental is not in scope.

**Measured in the watcher slice**, which adds the trigger (an external edit).
The timeline stops counting turns once its launch line is logged
(`timeline.rs:84-88`), so each figure comes from a temporary probe. Two runs:

- W1b (watcher spec, Performance): the real app, release, on the 50,000-note
  vault from `crates/app/examples/startup_vault.rs`. An external edit to one
  note reaches the index 141–146 ms after the event, 100 ms of it the batch.
- 2026-09-28: release, Apple M5 Pro, a `#[gpui::test]` over a 50,000-note
  `MemFs` with the index and the sidebar, 5 external edits of one note each in
  a vault of 100 folders and in a flat one. Foreground cost does not depend on
  the `Fs`.

| Figure | Target | Measured (2026-09-28) |
|---|---|---|
| Background work per mtime-only rescan | under 2 ms (the diff) | 0.27–0.31 ms (`Change::between`) |
| Foreground work per mtime-only rescan | under 0.1 ms | 0.002–0.006 ms (`apply_change`). The swap's notify also rebuilds the sidebar: 1.1–1.2 ms with 100 collapsed folders, 2.1–2.2 ms flat (`Sidebar::rebuild`, not this spec's) |
| Builds applied under a notify every 50 ms for 2 s | at least one per two notifies' worth of builds | covered by behaviour, not timed: the tests in Testing 1 hold builds in flight and assert each one applies |

`Vault::reidentify` on a save (criterion, `crates/vault/benches/vault.rs`,
50,000 notes): 0.062 ms, and 0.217 ms when `IndexSync::last` shares the
entries, so the re-key copies them. Under 0.5 ms: not deferred.

An autosave's echo no longer reaches this spec: its swap changes nothing and
notifies nobody (watcher spec, F6 and Performance).

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Incremental layout change (one create or rename without a full `NoteSet::build`) | A rescan with a layout change shows in the timeline once the watcher lands | Needs `NoteSet` to take a delta; `names` is a sorted snapshot today |
| Producer-side change sets, Zed's `UpdatedEntriesSet` (`worktree.rs:4040`) | The file watcher: its event batches already know the changed paths, and the buffer reload is a second consumer | `Vault::applied` returns `(path, id, change)` beside the new vault; `diff.rs` goes, and `forgotten` may too |
| ~~Derive dirtiness instead of `Build::Dirty`~~ | **Closed 2026-09-28.** `Build::Dirty` stays. Since the watcher, a swap that changes nothing replaces the entries without notifying (watcher spec, F6), as a save's `reidentify` already did, so `!Arc::ptr_eq(last, vault.shared_entries())` would read each echo and each save as a change. The enum is exact, and Testing 1 holds it | Zed's `Buffer::reparse` (`language/src/buffer.rs:1926-1995`) re-runs when `version.changed_since(parsed)`; its version moves only on an edit, which our `Arc` does not give |
| Test-only fields in one `#[cfg(test)] probe: Probe` | The next test-only field | `apply_budget`, `turns`, `hold`; `merges` and `reconciles` went with the index scheduler spec (2026-09-29) |
| Split `index/tests.rs` (1,032 lines) | The next test added to it | Helpers (`indexed_vault`, `hold_builds`, `rescan`, `create`) to a shared test module; `./scripts/test-names.sh` before and after |
