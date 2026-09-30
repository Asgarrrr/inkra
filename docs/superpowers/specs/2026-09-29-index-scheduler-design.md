# Index scheduler: `LiveIndex` in `inkra-index`

Status: **implemented 2026-09-29.** Refactor: the index, quick open,
vault search, the backlinks panel and the launch line behave as before.

## Why

`IndexSync` (`crates/app/src/index.rs` and `index/{building,loading,merging,diff}.rs`,
about 770 lines) mixes two subjects. One is gpui wiring: spawning, the
entity, `NotesChanged`, `Timeline` marks, the 4 ms turn budget. The other is a
scheduler with no gpui in it: which notes need a load, which load or save is
superseded, when a word merge is due. Checked against the code on
`arch/deepening`:

- **Five collections kept in step by hand.** `indexed`, `latest`, `queued`,
  `queue` and `forgotten`, plus `loading`. `apply_notes` prunes four of them
  and flags the fifth (`building.rs:140-146`); `next_load` skips stale `queue`
  entries against `queued` (`loading.rs:153`).
- **Supersede rules spread over four functions.** `queue_changed`'s
  `latest > seq` guard (`building.rs:178-183`), `is_latest` in `apply` and
  `forget` (`index.rs:259-283`), and `failed`'s `note_sets` comparison
  (`loading.rs:133-144`).
- **Own-save echo.** `saved` records the mtime in `indexed` and mints a seq
  (`index.rs:241-256`); `queue_changed` then skips the rescan that finds that
  mtime, or any older build read (`building.rs:175-185`).
- **Vault reads from the scheduler.** `next_load` (`loading.rs:147`),
  `failed` (`loading.rs:134-137`) and `retry_forgotten` (`building.rs:159-172`)
  read the `Vault` entity through `cx`.
- **The diff lives in `app`.** `diff.rs` compares `Entry` slices, and
  `Built::new` converts entries to tuples for `NoteSet::build`
  (`building.rs:21-39`), though `inkra-index` already depends on `inkra-vault`.
- **The merge lifecycle is steered by telemetry.** `settling` and
  `marked_indexed` (`index.rs:79-83`) choose `MergeWhen::Settle` over `Due`
  (`merging.rs:10-14`); the settle is asked on an edge computed in
  `timed_turn` (`index.rs:203-222`).

The consequence is the test surface. Twenty-odd `#[gpui::test]`s reach into
private fields (`indexed`, `queued`, `loading`, `build`, `note_sets`), call
private steps (`next_seq`, `apply`, `forget`, `merge_words`), or read
test-only counters (`merges`, `reconciles`). `words_tests.rs:264` forges a
settle by writing `merges`, `marked_indexed` and `settling`. None of this logic
is under `cargo mutants`, which examines `inkra-index` but not `inkra-app`.

## What Zed does

Zed keeps the analogous state on the gpui entity. `Buffer` records
`saved_mtime` in `did_save` (`vendor/zed-reference/crates/language/src/buffer.rs:1636-1649`),
and `file_updated` asks for a reload only when the disk state moved
(`buffer.rs:1778-1810`): our own-save echo rule, one buffer at a time. A
reload in flight is superseded by replacing `reload_task` (`buffer.rs:1676`),
and its result applies only if the version did not move meanwhile
(`buffer.rs:1735-1750`).

The worktree splits the other way. `BackgroundScannerState`
(`worktree/src/worktree.rs:277`) holds the scan's state as plain data, and the
`LocalWorktree` entity only receives its snapshots. We follow that split: the
state and its rules go in a plain type; the entity keeps the tasks.

## The module after the change

`inkra_index::LiveIndex`, in `crates/index/src/live.rs`. It owns the
`NoteIndex` and decides, from four inputs, what the index must load, what it
drops, and when its words merge. It reads no file and no clock, and spawns
nothing: each input returns the work to run, and the caller runs it where it
wants.

Why `inkra-index`: the rules are about the index's content (a load supersedes
a load, a build prunes the notes gone), the inputs are `inkra-vault` types the
crate already reads, and the crate is under `cargo mutants` and `proptest`.
Why it owns the `NoteIndex`: `apply` and `forget` must insert or remove only
when the ticket is the latest, and `apply` prunes `indexed` from
`NoteIndex::is_indexed` right after the insert (`index.rs:277-280`). Split
across two owners, the caller would sequence them again.

```rust
/// A `NoteIndex` kept current: which notes to load, which load is superseded,
/// when the words merge.
pub struct LiveIndex { /* private */ }

/// One load or save of one note; it acts only while it is the latest minted for that note.
#[derive(Clone, Copy, Debug)]
pub struct Ticket { /* id, seq, note_sets at the start */ }

/// A load to start: read `path`, parse it, hand the result to `landed`.
pub struct Load { pub ticket: Ticket, pub path: PathBuf }

/// The diff of two entry snapshots, run off the foreground.
pub struct BuildJob { /* seq at the read, last, entries */ }
pub struct Built { /* seq, entries, Change */ }

pub struct Applied {
    /// A notify came while the build ran: call `start_build` again.
    pub dirty: bool,
    /// The note set changed: names and link resolution may differ.
    pub layout: bool,
    pub retired: Retired,
}

/// The previous note set, entries and live set; free them off the foreground.
#[must_use = "free it off the foreground"]
pub struct Retired { /* ... */ }

/// A word merge to run off the foreground; `settle` when the startup asked for it.
pub struct Merge { /* WordsMerge, settle */ }
pub struct Merged { /* MergedWords, settle */ }

pub struct TurnEnd {
    pub merge: Option<Merge>,
    /// True once: the startup load ended and its settle applied.
    pub indexed: bool,
}

impl LiveIndex {
    pub fn index(&self) -> &NoteIndex;

    /// Input 1, vault entries changed. `None` while a build runs; that build then returns `dirty`.
    pub fn start_build(&mut self, entries: Arc<[Entry]>) -> Option<BuildJob>;
    pub fn apply_build(&mut self, built: Built, vault: &Vault) -> Applied;

    /// Input 2, a note saved: its mtime is the one indexed, and the parse applies through `landed`.
    pub fn saved(&mut self, id: EntryId, mtime: Mtime) -> Ticket;

    /// Input 3, a load slot free: the next queued note, while fewer than `slots` load.
    pub fn next_load(&mut self, vault: &Vault, slots: usize) -> Option<Load>;

    /// Input 4, a load or a save's parse landed; true when the index changed.
    pub fn landed(&mut self, ticket: Ticket, loaded: inkra_vault::Result<NoteContent>, vault: &Vault) -> bool;

    /// After every foreground turn: the startup step, a due merge, the `Indexed` edge.
    pub fn end_turn(&mut self) -> TurnEnd;
    #[must_use = "free the retired words off the foreground"]
    pub fn merged(&mut self, merged: Merged) -> RetiredWords;

    /// `(loaded, total)`: the notes of the last build, less those queued or loading.
    pub fn progress(&self) -> (usize, usize);
    pub fn awaits_first_build(&self) -> bool;
    pub fn is_building(&self) -> bool;
}

impl BuildJob { pub fn run(self) -> Built; }
impl Merge { pub fn run(self) -> Merged; }
```

`vault: &Vault` is the live vault the caller reads at that moment, not the
build's snapshot: it finds an id in O(1) (coalesced-reconcile spec, R1), and
`landed` must see a trash made after the build read.

### Invariants

- **L1. One counter orders loads and saves.** `saved`, a queued load and a
  retry each mint a seq and record it in `latest[id]`. `landed` inserts or
  forgets only when `ticket.seq == latest[id]`; otherwise it changes nothing.
- **L2. Own-save echo.** `indexed[id]` is the mtime the index holds or has
  queued. `apply_build` queues a note unless `indexed[id] == mtime`, or
  `latest[id] > seq` where `seq` is the counter `start_build` read: a save
  since the read holds a newer mtime than the build's.
- **L3. One queue entry acts per note.** `queued[id]` is the seq of the
  note's live queue entry; `next_load` skips an entry whose seq differs, and
  requeues at the front, in order, an entry whose note is loading. A queued
  note the live vault no longer lists is dropped.
- **L4. A build prunes.** A `layout` apply removes every id not in the new
  set from `indexed`, `latest`, `queued` and `forgotten`, and marks its
  in-flight load not live: that load keeps its slot until it lands, and is not
  pending. It reserves the maps for the notes not yet indexed (open-first
  spec, O7).
- **L5. Failure.** A load failing for a note the live vault lists, with a
  `layout` applied since it started, and still the latest, queues a fresh
  load. Otherwise, if still the latest, the note is forgotten: removed from
  the index and `indexed`, and recorded in `forgotten`. Each apply, `layout`
  or not, queues again every forgotten note the live vault lists.
- **L6. One build at a time.** `Build` is `Idle`, `Running` or `Dirty`
  (coalesced-reconcile spec, R1). Every build that lands applies. The first
  build is always a `layout` (`last` is `None`).
- **L7. Startup.** `enum Startup { Loading, Settling, Settled, Done }`
  replaces `settling` and `marked_indexed`, as the smart-quick-open Deferred
  row "One startup state in `IndexSync`" asked. `end_turn` steps it:
  `Loading` and not building → `Settling`; `Settling` asks
  `MergeWhen::Settle`, and becomes `Settled` when its merge applies or finds
  nothing to merge; `Settled` and not building → `Done`, and `indexed` is
  true that once; `Settled` and building → `Loading`. Every other state asks
  `MergeWhen::Due`. This equals today's edge (`building && !marked &&
  !is_building`) because `is_building` changes only inside a turn: between
  turns, `start_build` and `saved` leave `queued`, `loading` and `last` as
  they were.

### Ordering

The turn's end is `LiveIndex::turn`'s since the index-turn-effects spec
(`2026-09-30-index-turn-effects-design.md`, D2): it refills the slots, steps
the startup and the merge, reruns a dirty build, then compares for `notify`.
No caller calls `next_load` or `end_turn`; `IndexSync` marks `Indexed`
outside its timed turn.

- `apply_build`: `note_sets += 1` and `set_notes` (layout only), prune (L4),
  replace `last`, queue the changed notes (L2), retry the forgotten (L5).
- `landed`: free the ticket's slot first, then apply or fail, so a retry can
  take the slot it frees.

### What moves where

| Now (`crates/app/src/index/`) | After |
|---|---|
| `indexed`, `next_seq`, `latest`, `queue`, `queued`, `forgotten`, `loading`, `build`, `last`, `note_sets`, `settling`, `marked_indexed`, `index` | `LiveIndex` fields; `settling` and `marked_indexed` become `Startup` |
| `next_seq`, `is_latest`, `queue_load`, `forget`, `apply` (`index.rs`) | `live/loads.rs` |
| `queue_changed`, `apply_notes`, `retry_forgotten`, `Build`, `Built` (`building.rs`) | `live/builds.rs` |
| `failed`, `next_load`, the `Loading` slot (`loading.rs`) | `live/loads.rs` |
| `diff.rs`, `diff_tests.rs` | `live/diff.rs`, `live/diff_tests.rs`, unchanged but for paths |
| the settle edge (`timed_turn`), `merge_words`'s choice, `apply_merge`'s state | `live/startup.rs` |
| `progress`, `is_building`, `awaits_first_build` | `LiveIndex`; `IndexSync` delegates `progress` as it is: with no vault, `LiveIndex` stays empty and `progress` is `(0, 0)`. It wraps the two `bool`s as `self.vault.is_some() && self.live.…()`: `LiveIndex` has no `last` without a vault, so its `is_building` alone would hold, and `a_detached_note_emits_nothing` pins it false |

`IndexSync` keeps: the entity, its subscriptions, `revision` and
`simulate_load` (four other views' tests use it), `NotesChanged`,
`Timeline::mark` and `Timeline::turn`, `timed_turn`, `drain` with
`APPLY_BUDGET` and `yield_now`, `LOADS_PER_CORE` and the slot count from
`num_cpus`, every `spawn`, and the background drops of `Retired` and
`RetiredWords`. `revision` stays in the shell: it serves observers, and each
input reports whether the index changed.

`reconcile`'s trailing `mark_indexed` call (`building.rs:100`) goes. By L7's
argument it never marks: outside a turn, `Startup` is never `Settled` or
`Loading` with nothing building. Step 2 proves it before deleting it.

Test hooks: `merges`, `reconciles`, `settling`, `marked_indexed` go, with
every test write to `indexed`, `next_seq`, `apply`, `forget`, `merge_words`.
A test that needs a ticket for a note mints it through `LiveIndex::saved(id,
mtime)`, reached as the private `live` field from a test module under
`index`, as `next_seq` is reached today. `Ticket` keeps its fields private and
gets no test constructor: `land_at_once` (`loading_tests.rs`) takes each ticket
from `saved`.
`hold`, `apply_budget` and `turns` stay: they hold a spawned task and time
the shell's turn, which stays in `app`. The coalesced-reconcile Deferred row
"Test-only fields in one probe" keeps its trigger with three fields left.

`NoteSet::build` keeps its tuple signature: the conversion moves into
`inkra-index` with `Built`, and its 30 bench and test callers stay as they
are (Deferred).

## Tests

The new interface is the test surface. `live/tests.rs` holds plain
`#[test]`s over a `Vault` from `test_support::vault` and a `MemFs`, driving
the four inputs by hand: `start_build` → `run` → `apply_build`, `next_load` →
`MemFs::load` → `NoteContent::parse` → `landed`. Each test below fails on a
named fault; step 1 injects each fault once and records the result in "As
built".

| Behaviour | Test (`live/tests.rs`) | Fault it catches |
|---|---|---|
| L6 one build in flight | `a_build_asked_while_one_runs_is_dirty_and_runs_after_it` | `start_build` returns a job while `Running` |
| L6 every build applies | `a_build_landing_behind_the_vault_applies` | `apply_build` drops a build read before a later `start_build` |
| L6 first build | `a_vault_with_no_notes_applies_one_layout_and_stops_building` | `last: None` diffs as `Mtimes` |
| R2 fast path | `an_mtime_only_change_queues_that_note_and_keeps_the_set` | `Mtimes` calls `set_notes` or queues every note |
| L2 echo | `a_save_since_the_build_read_is_not_queued`, `a_rescan_finding_the_saved_mtime_loads_nothing` | `latest > seq` dropped; `indexed` not recorded in `saved` |
| L1 supersede | `a_load_landing_after_a_newer_save_is_dropped`, `a_stale_failure_keeps_the_newer_mtime` | `landed` ignores the seq |
| L3 queue | `a_note_queued_twice_is_loaded_once`, `a_note_queued_while_it_loads_waits_for_its_load` | `next_load` reads a stale entry, or starts a second load of one note |
| slots | `next_load_fills_at_most_the_slots_in_vault_order` | `<` becomes `<=` |
| L4 prune | `a_load_in_flight_for_a_note_gone_is_not_pending`, `a_note_reloading_during_its_load_counts_once` | a pruned map left out; `live` not cleared |
| L5 retry | `a_load_failing_after_a_layout_is_retried`, `a_load_failing_after_only_mtimes_forgets_the_note`, `a_forgotten_note_is_queued_at_the_next_apply_of_either_kind`, `a_failed_load_behind_a_save_still_parsing_keeps_the_note`, `a_failed_load_behind_a_queued_reload_keeps_the_note` | `note_sets` compared as equal; `forgotten` not retried on `Mtimes` |
| save before its note joins | `a_save_landing_before_its_note_joins_is_reloaded_by_the_next_build` | `indexed` kept after an ignored insert |
| L7 settle | `the_load_ending_settles_then_reports_indexed_once`, `a_settle_with_nothing_to_merge_reports_indexed_in_that_turn`, `a_save_during_the_settle_stays_fresh_and_indexed_is_reported`, `building_again_before_indexed_settles_again`, `a_reload_after_indexed_starts_no_settle`, `a_load_past_the_policy_starts_a_due_merge` | the state machine stepped on the wrong edge |

`live/model_tests.rs`, one proptest: random interleavings of file edits,
creates, renames and trashes through `MemFs` and a rescanned `Vault`,
`start_build` / `run` / `apply_build` with builds held across later
changes, saves, loads landing in any order, and loads failing. After every
step: at most `slots` loads, every pending note is a note of the applied set,
and `progress().0 <= progress().1`. After draining (builds until not dirty,
loads until none): `index()` answers as a `NoteIndex` built fresh from the
final vault, the check `random_operations_leave_the_index_as_a_fresh_build`
makes today.

App tests after the change:

- **Move to `live/tests.rs`,** rewritten on the interface: every test that
  writes or reads `indexed`, `queued`, `loading`, `build`, `note_sets`,
  `merges`, `reconciles`, `settling` or `marked_indexed`, or calls
  `next_seq`, `apply`, `forget` or `merge_words`. That is
  `tests.rs` (`a_save_landing_before_its_note_joins_the_index_is_reloaded`,
  `a_stale_forget_…`, `a_stale_failed_load_…`, `a_failed_load_behind_a_save_still_parsing_…`,
  `a_build_landing_behind_the_vault_…`), `scanning_tests.rs`
  (`two_notifies_behind_one_held_build_start_one_rebuild`),
  `words_tests.rs` (`a_save_during_the_settle_…`), and the counter reads in
  `startup_tests.rs`. The table above names their successors.
  `tests.rs`'s `a_failed_load_behind_a_queued_reload_keeps_the_note` reads
  `queued` and `indexed`; it moves too, as the L5 test of the same name.
- **`loading_tests.rs`, test by test.** `a_turn_over_its_budget_…`,
  `drain_yields_…` and `a_turn_applies_every_landed_load_…` stay: they time the
  shell's turn. `land_at_once` mints each ticket with `live.saved(id, mtime)`
  and sends `Landed { ticket, loaded }`. `a_load_missing_a_renamed_note_behind_a_save_is_not_retried`
  stays as an app integration test, its `next_seq(a)` replaced by
  `live.saved(a, mtime)`. The others read only `progress`, `backlinks` and
  `found`, and stay unchanged.
- **Stay in `app`, with the counter reads replaced by behaviour:**
  `note_sets` becomes a count of `NotesChanged` (`scanning_tests::notes_changed`
  exists), `merges` becomes `words_settled()` and the launch line,
  `reconciles` and `build` go where a pure test covers them. These keep the
  wiring: Timeline marks, the turn budget (`apply_budget`, `turns`), held
  builds against `MemFs` gates, `NotesChanged` emission, and the gpui random
  test, less `assert_pending_are_notes`, which moves into the model test.
- `diff_tests.rs` moves as is.

`./scripts/test-names.sh` before step 1 and after each step; every name that
leaves `app` has a successor in the table or in "As built".

## Performance

The foreground work per input is the same code, moved: nothing is added per
keystroke, and a save still costs one seq, one map insert and one parse off
the foreground. `apply_build` keeps its reserve (O7) and hands back the
previous `NoteSet`, `last` and live set in `Retired` for a background drop,
as today. Criterion: on the 50,000-note vault of
`crates/app/examples/startup_vault.rs`, release, the launch line's
`longest turn` stays under 8 ms and within noise of `arch/deepening`, three
runs each.

## Deviations from Zed

- **The state is a plain type beside the entity,** not fields of it as on
  `Buffer`. Our case has 50,000 notes, six collections and a proptest model;
  Zed's `BackgroundScannerState` is the precedent for the split.
- **A per-note seq, not a version vector.** Zed's `clock::Global` orders
  concurrent edits; a load and a save need only "newer than".
- **The diff stays a comparison of two snapshots** (coalesced-reconcile spec,
  "What Zed does"). It moves from `app` to `inkra-index`; the move to
  `inkra-vault` still waits for a second consumer.

## Risks

- **L7 drifts from today's edge.** The argument rests on `is_building`
  changing only inside a turn. The four settle tests in `words_tests.rs` and
  `startup_tests.rs` stay in `app` unchanged as the oracle, and the pure L7
  tests pin each transition.
- **A rewritten test goes vacuous.** Each fault in the table is injected once
  (step 1 and 2) and must fail its test.
- **A retired value dropped on the foreground.** `Retired` is `#[must_use]`;
  the performance criterion catches a 9 ms free (search spec, slice 2).
- **Mutants.** The logic enters `cargo mutants`' scope. Zero missed is the
  bar. Of the three `loading.rs` exclusions in `.cargo/mutants.toml`, two stay
  with the shell's code: `load_more`'s `*` (the slot count) and
  `apply_landed`'s `<` (the wall-time budget). The third, `&&` in the slot
  lookup `load.id == id && load.seq == seq`, moves with its code into
  `live/loads.rs`, retargeted there with the same equivalence argument.
- **Size.** Step 1 moves about 400 lines. It stays one commit so no
  intermediate `pub` surface is published and withdrawn.

## Plan

Each step builds, passes `./scripts/check.sh`, and is one commit.

1. **Loads and builds.** Run `./scripts/test-names.sh` and keep the output.
   Write `live/tests.rs` for L1–L6 and `live/diff_tests.rs` (moved) against
   the interface, red. Create `live.rs`, `live/{loads,builds,diff}.rs`; move
   the fields and functions of the table; `IndexSync` delegates, passing
   `vault.read(cx)` and `num_cpus() * LOADS_PER_CORE`. Remove the moved app
   tests and the `reconciles` hook; replace `note_sets`/`build` reads. Inject
   each L1–L6 fault, record the failures. Move the slot-lookup `&&`
   exclusion in `.cargo/mutants.toml` to `crates/index/src/live/loads.rs`.
   `cargo mutants` on `inkra-index`.
2. **Startup and merges.** L7 tests, red. Add `live/startup.rs` with
   `Startup`, `end_turn`, `Merge`, `merged`; `timed_turn` calls `end_turn`
   and spawns the merge; `merging.rs` goes. Before deleting `reconcile`'s
   `mark_indexed` call, replace its body with a panic and run the app tests:
   none may reach it. Remove the `merges`, `settling`, `marked_indexed`
   hooks. Inject the L7 faults.
3. **Model and docs.** Add `live/model_tests.rs`; move
   `assert_pending_are_notes`' checks into it. Take the performance figure.
   Close the smart-quick-open Deferred row "One startup state", update the
   coalesced-reconcile Deferred row "Test-only fields" and R3's module note,
   add this spec to `CLAUDE.md`'s docs list and "deciding which notes to load
   → `index`" to its "Where code goes". Fill "As built".

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| `NoteSet::build(&[Entry])` in place of tuples | The next change to `NoteSet::build`'s callers | 30 bench and test callers; `test_support::entries` goes with it |
| The last three test hooks (`hold`, `apply_budget`, `turns`) in one probe | The next test-only field on `IndexSync` | As the coalesced-reconcile row says |
| The diff in `inkra-vault` | A second consumer of a vault delta | Coalesced-reconcile spec, Deferred |
| A load's read checked against its note | A note showing another note's text after renames during the startup load | The load compares `Vault::id_for(loaded.identity)` with its ticket's id; a mismatch fails the load, which L5 retries after the layout the renames bring. Found by the model test, see As built |

## As built

**Layout.** `crates/index/src/live.rs` (the struct, `progress`,
`is_building`, `awaits_first_build`) and `live/{loads,builds,diff,startup}.rs`,
tests in `live/{tests,startup_tests,model_tests,diff_tests}.rs`. The
`Change::Layout` payload, today's `Built`, is named `Layout`; `Built` is the
`BuildJob`'s result. `Retired` holds the previous `last`, note set and live
set, dropped in one background task instead of three. `Merge` is
`#[must_use]`. `IndexSync` is 242 lines, `building.rs` 66, `loading.rs` 105.

**Commits.** Steps 1 and 2 landed as one commit. Step 1 moves the
`NoteIndex` into `LiveIndex`, and `merging.rs` needs it mutably; split, the
first commit would publish an `index_mut` the second withdraws.

**Deviations.**

- `IndexSync::is_building` keeps the `vault.is_some() &&` wrapper, against
  review finding 5 (spec amended in `01c4c00`): `LiveIndex` has no `last`
  without a vault, so its `is_building` alone holds, and
  `a_detached_note_emits_nothing` pins it false. `progress` is a plain
  delegate.
- **Saves pass through `landed`,** so a save's ticket frees no slot only
  because the lookup is `load.id == id && load.seq == seq`. With `||`, a save
  landing during its note's load frees that load's slot. The `&&` exclusion in
  `.cargo/mutants.toml` is therefore deleted, not moved;
  `a_save_landing_during_a_load_keeps_its_slot` kills the mutant.
- `a_build_landing_behind_the_vault_applies_then_rebuilds` stays in `app`,
  its counter reads replaced by `NotesChanged` counts: it is the only test of
  the shell's `dirty` → `reconcile` wiring.
- `a_save_during_a_build_is_not_reloaded_when_the_build_applies` read
  `indexed`; it now rescans and asserts no reload.
- The four app settle tests keep their launch-line and `words_settled`
  assertions; the `merges` and `settling` reads went.
- L7's check (step 2): on `arch/deepening`, an `assert!` that `reconcile`'s
  trailing `mark_indexed` would mark nothing passed all 599 app tests; the call
  is gone.

**Faults injected** (step 1 and 2), each against `live::` only:

| Fault | Failing tests |
|---|---|
| `start_build` returns a job while `Running` | `a_build_asked_while_…`, `a_build_landing_behind_the_vault_applies` |
| `apply_build` drops a dirty build | `a_build_landing_behind_the_vault_applies` |
| `last: None` diffs as `Mtimes` | `no_last_snapshot_is_a_layout`, `a_vault_with_no_notes_applies_one_layout_…` |
| `Mtimes` queues every note | four `diff_tests` |
| `latest > seq` dropped | `a_save_since_the_build_read_is_not_queued` |
| `indexed` not recorded in `saved` | `a_rescan_finding_the_saved_mtime_…`, `a_stale_failure_…` |
| `landed` ignores the seq | `a_load_landing_after_a_newer_save_…`, `a_stale_failure_…`, `a_failed_load_behind_a_save_still_parsing_…` |
| `next_load` reads a stale entry | `a_note_queued_twice_is_loaded_once` |
| a second load of one note | `a_note_queued_while_it_loads_…` |
| `<` becomes `<=` on the slots | `next_load_fills_at_most_…` and two more |
| `queued` not pruned | `a_load_in_flight_for_a_note_gone_…` (it first survived; the test now also removes a queued note) |
| `live` not cleared | `a_load_in_flight_for_a_note_gone_…` |
| `note_sets` compared as equal | the three L5 retry tests |
| forgotten not retried | `a_forgotten_note_is_queued_…` |
| `indexed` kept after an ignored insert | `a_save_landing_before_its_note_joins_…` |
| slot lookup `&&` → `\|\|` | `a_save_landing_during_a_load_keeps_its_slot` |
| each L7 transition | at least one of the six L7 tests each |

The model test alone catches the L1, L4, slot and ignored-insert faults. It
misses the L2 and L5 ones: they cost extra loads, and its final drain hides
them.

**Mutants.** `cargo mutants --in-diff` on `inkra-index`: 92 mutants, 78
caught, 14 unviable, 0 missed, 0 timeouts. An earlier timeout (`==` → `!=`
on `Build::Dirty`) hung the model's drain; the drain is now bounded and
fails, and the mutant is caught.

**A race the model found.** A load reads its path when it runs, not when
`next_load` returns it. If two renames between those moments put another
note at that path, that note's text lands under the loading note's id, and
nothing reloads it: a rename keeps the mtime. `arch/deepening` has the same
logic, so this slice keeps it; the model reads each file when its load starts.
Deferred below.

**Performance.** Release, the 50,000-note nested vault, `launch-figure.sh`,
runs alternated with `arch/deepening` on a machine shared with other builds
(load 7 to 11): after, longest turn 1.7–1.8 ms in six runs, indexed
0.97–1.25 s; before, 1.7–1.8 ms bar one 10.0 ms outlier, indexed 1.01–1.31 s.
Under 8 ms and within noise.
