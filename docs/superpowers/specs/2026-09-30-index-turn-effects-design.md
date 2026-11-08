# Index turn effects: `LiveIndex` returns the whole turn's work

Status: **proposed 2026-09-30.** Refactor: the index, quick open, vault
search, the backlinks panel and the launch line behave as before, except that
`IndexSync` notifies only when a reader's answer can differ (see D5).

This completes the index-scheduler spec's choice, "each input returns the
work to run, and the caller runs it" (`crates/index/src/live.rs:1-4`). Today
each input returns *part* of the work; the rest is an order the caller must
know. It does not reverse "spawns nothing": `inkra-index` still spawns
nothing and reads no clock.

## Problem

`LiveIndex`'s Interface is seven methods plus an ordering protocol that
lives in its callers. Checked on `arch/specs`:

- **The slots are refilled by the caller.** After `apply_build` and after a
  batch of `landed`, the caller must loop `next_load` with a slot count
  (`crates/app/src/index/loading.rs:25-45`, called from `building.rs:62` and
  `loading.rs:89`). A path that lands a result and forgets `load_more`
  stalls the queue with no error.
- **The turn end is the caller's.** `end_turn` must run after every
  foreground turn that can change `is_building`, or the settle never starts
  and `Indexed` is never marked. L7's correctness rests on the argument
  "`is_building` changes only inside a turn" (`crates/index/src/live/startup.rs:51`).
  Nothing enforces it: `timed_turn` (`crates/app/src/index.rs:132-148`) is
  the one place that calls `end_turn`, and `saved` (`index.rs:193`) and
  `reconcile`'s `start_build` (`building.rs:20`) run outside it. They are
  safe only because the scheduler spec proved they leave `queued`, `loading`
  and `last` alone.
- **The dirty rebuild is the caller's.** `Applied.dirty` goes back through
  `timed_turn`'s return value to a second `update` that calls `reconcile`
  (`building.rs:35-40`). The `is_scanned` gate that decides when a build may
  start is also the caller's (`building.rs:16`).
- **`revision` is kept by the caller from three sources.** `land` bumps it
  when `landed` returns true (`index.rs:214-217`), `apply_build` when
  `layout` (`building.rs:57-58`), and the test-only `simulate_load`
  (`index.rs:93-96`). `IndexWatch` keys on it (`watch.rs:55,68`). The rule
  "the revision moves when an answer changed" is `LiveIndex` knowledge,
  reassembled by the caller.
- **`notify` follows no rule.** `land` notifies only on a change
  (`index.rs:214-217`); `apply_landed` (`loading.rs:90`) and `apply_build`
  (`building.rs:63`) always notify; `apply_merge` never does
  (`index.rs:168-173`).
- **Retired values come back in two shapes.** `Applied.retired`
  (`building.rs:56`) and `merged`'s `RetiredWords` (`index.rs:170-172`) are
  each dropped off the foreground by a separate spawn.

The protocol has three Adapters, not one. `IndexSync` is the production one.
The pure tests' `Harness` re-sequences it by hand (`live/tests.rs:66-72`,
`load_all` at `:96-100`, `turn` at `:150-152`), and so do the startup tests'
`load_all_in_turns` and `merged` (`live/startup_tests.rs:7-26`). The proptest
`Model` copies it with a comment, "As `IndexSync` does: a change during the
build asks for another" (`live/model_tests.rs:106-110`), and its `drain`
(`:238-250`) never calls `end_turn`. So the random interleavings never start,
run or apply a word merge, and L7 is covered only by six hand-written
sequences.

**Deletion test.** Delete `LiveIndex`'s Interface as it stands and the
ordering knowledge does not vanish: it already sits, duplicated, in three
callers. Concentrating it in one Module passes the test.

## Decision

### D1. One input shape: a turn

Every input to `LiveIndex` runs inside a turn, and every turn ends inside
`LiveIndex`. The turn is a closure, as gpui's `App::update` runs its closure
and then `finish_update` flushes the effects
(`vendor/zed-reference/crates/gpui/src/app.rs:1158-1177`). A caller cannot
start a turn and forget to end it.

```rust
impl LiveIndex {
    /// `slots`: how many loads may run at once.
    #[must_use]
    pub fn new(slots: usize) -> Self;

    /// Runs `inputs` against the live `vault`, then ends the turn: the work it made due.
    pub fn turn<R>(
        &mut self,
        vault: &Vault,
        inputs: impl FnOnce(&mut Turn<'_>) -> R,
    ) -> (R, Effects);

    // A getter added here joins `Seen` in `live/model_tests.rs`, or D5's check misses it.
    pub fn index(&self) -> &NoteIndex;
    /// Moves when an answer may differ: a layout applied, a note's content inserted or forgotten.
    pub fn revision(&self) -> u64;
    /// From index-status-once, which lands first; `progress` stays `pub(crate)`.
    pub fn status(&self) -> Status;
}

/// The inputs of one foreground turn; only `LiveIndex::turn` makes one.
pub struct Turn<'a> { /* live: &'a mut LiveIndex, vault: &'a Vault, what the end reads */ }

impl Turn<'_> {
    /// The vault's entries changed. A build starts once the vault is scanned; one running turns dirty.
    pub fn vault_changed(&mut self);
    /// A build's result, from `BuildJob::run`.
    pub fn built(&mut self, built: Built);
    /// A note saved: `mtime` is the one indexed, and the parse of its text comes back through `landed`.
    pub fn saved(&mut self, id: EntryId, mtime: Mtime) -> Ticket;
    /// A load or a save's parse landed.
    pub fn landed(&mut self, ticket: Ticket, loaded: inkra_vault::Result<NoteContent>);
    /// A word merge's result, from `Merge::run`.
    pub fn merged(&mut self, merged: Merged);
}

/// Everything the turn made due. Run each job off the foreground and hand its result
/// back in a later turn; free `retired` off the foreground.
#[must_use = "run its jobs; free `retired` off the foreground"]
pub struct Effects {
    /// Read, parse, hand to `Turn::landed`. At most the free slots, in queue order.
    pub loads: Vec<Load>,
    /// Diff off the foreground, hand to `Turn::built`. At most one runs (L6).
    pub build: Option<BuildJob>,
    /// Run off the foreground, hand to `Turn::merged`.
    pub merge: Option<Merge>,
    /// `None` when the turn retired nothing, so an idle turn spawns no drop.
    pub retired: Option<Retired>,
    /// The note set changed: names and link resolution may differ.
    pub layout: bool,
    /// `revision` or `progress` moved: a reader's answer may differ.
    pub notify: bool,
    /// True once: the startup load ended and its settle applied.
    pub indexed: bool,
}
```

`loads` is a `Vec` because one refill fills every free slot; `build` and
`merge` are `Option`s because at most one of each runs at a time (L6,
`is_merging`), so a turn can make at most one of each due.

`Ticket`, `Load`, `BuildJob::run`, `Built`, `Merge::run` and `Merged` keep
their shapes. `Applied`, `TurnEnd` and `landed`'s `bool` leave the public
Interface. `start_build`, `apply_build`, `next_load`, `end_turn` and
`merged` become private steps of `Turn`.

### D2. The turn's end, in one place

`turn` runs `inputs`, then, in this order:

1. **Refill.** `next_load` until the slots are full or the queue is empty,
   into `loads`. Today `load_more` runs at this point.
2. **Step the startup and the merge.** Today's `end_turn`: its `merge`
   goes into `merge`, its `indexed` into `indexed`.
3. **Rerun a dirty build.** When the turn applied a build that was dirty,
   start the next one from `vault.shared_entries()` into `build`. Today it
   starts in a separate update after the timed turn; `start_build` changes no
   field `is_building` reads, so its place in the order changes no answer.
4. **Notify.** `notify` is true when `(revision, progress)` differs from the
   turn's start.

### D3. The build gate moves in

`vault_changed` reads `vault.is_scanned()` and starts nothing before it. It
is the same gate as `reconcile`'s (`building.rs:16`), and D2's rerun passes it
by construction: a dirty build means a build started, so the vault was
scanned. One place decides when a build may start.

### D4. `revision` belongs to `LiveIndex`

`apply_notes` (a layout), `apply_content` (a latest ticket's insert) and
`forget` (a latest ticket's forget) bump it. Those are the three places
where `landed` returns true or `layout` is set today, so the value an
observer sees moves on the same inputs. A merge, a stale ticket, an
`Mtimes`-only apply and a save's mint do not bump it.

`IndexWatch` reads `IndexSync::revision`, which delegates. The four views'
refresh tests call `IndexSync::simulate_load`; it stays, as a `#[cfg(test)]`
offset `IndexSync` adds to `live.revision()`. `inkra-index` gets no
test-only public method.

### D5. `notify` follows one rule

`IndexSync` notifies when `Effects.notify` is true. `status` is derived
from `progress` and the first-build flag, and that flag changes only with a
layout, which bumps the revision. So a turn with `notify` false changes none
of `LiveIndex`'s getters. Fewer notifies than today: an
`Mtimes` apply that queued nothing and a batch of stale tickets no longer
notify.

This is a checked property, not an argument over today's observers:

- **In `inkra-index`, a test.** The model (`live/model_tests.rs`) takes a
  `Seen` snapshot before and after every turn it runs: `revision`,
  `progress`, `status`, and the index through the model's existing
  `answers`. When `notify` is false, the two snapshots
  must be equal. `Seen` lists every getter of `LiveIndex` in one place, and
  the getter block carries a one-line comment that a new getter joins it.
- **In `IndexSync`, a shape.** Its read methods (`index.rs:87-128`) only
  delegate to `live`, or read `vault`, which `new` sets once
  (`index.rs:60`) and nothing reassigns. That rule is written on
  `IndexSync`'s getter block. A reader of `IndexSync` therefore reads
  `LiveIndex`'s getters, or constants, and the model covers them whatever the
  reader is.

The residual gap: a new mutable `IndexSync` field with its own getter. It
breaks the stated rule on the block it is added to, so review sees it in
the diff, not in a grep of callers.

### `IndexSync` after the change

`IndexSync` becomes an executing Adapter. It keeps the entity, the
subscriptions, `NotesChanged`, the `Timeline` marks, `drain` with
`APPLY_BUDGET` and `yield_now`, `LOADS_PER_CORE` and `num_cpus` (now passed
once, to `LiveIndex::new`), every spawn, and the test hooks `hold`,
`apply_budget`, `turns`. One helper replaces `timed_turn`, `end_turn`,
`load_more`, `apply_merge`, `land`, and `reconcile`'s manual
`Timeline::turn`:

```rust
/// One timed foreground turn; its effects run, then the `Indexed` mark outside the timing.
fn turn<R>(
    &mut self,
    cx: &mut Context<Self>,
    inputs: impl FnOnce(&mut Turn<'_>) -> R,
) -> Option<R>;

/// Spawns each job with its hand-back, drops `retired` off the foreground,
/// marks `NamesReady` and emits `NotesChanged` on `layout`, notifies on `notify`.
fn run(&mut self, effects: Effects, cx: &mut Context<Self>);
```

Each hand-back is a spawned task that calls `this.update(cx, |this, cx|
this.turn(cx, |t| t.landed(..)))` (or `built`, `merged`). The drain's
batch is one turn: its closure lands up to `APPLY_BUDGET` of results through
`t.landed`. Timing moves inside the update, with an `Instant`, as
`reconcile` already does; `timed` stays for `vault/scans.rs`.

| Now | After |
|---|---|
| `load_more` (`loading.rs:25-45`) | D2 step 1, `LiveIndex` |
| `end_turn`, `apply_merge`, `timed_turn` (`index.rs:132-173`) | D2 step 2; `IndexSync::turn` and `run` |
| `dirty` → `reconcile` (`building.rs:35-40`) | D2 step 3 |
| `is_scanned` gate (`building.rs:16`) | `Turn::vault_changed` (D3) |
| `revision`, its three bumps | `LiveIndex::revision` (D4); a test offset in `IndexSync` |
| the three `notify` sites | `Effects.notify` (D5) |
| `Retired` and `RetiredWords` dropped by two spawns | `Effects.retired`, one spawn |
| `Harness::{build, next, load_all, turn}`, `load_all_in_turns`, `merged`, `Model::apply_build`'s rerun | one test executor over `Effects` (Tests) |

Estimated size: `IndexSync` (`index.rs`, `building.rs`, `loading.rs`) goes
from 415 lines to about 320; `live/` gains about 90 for `Turn` and `Effects`.

## Merge order

**index-status-once (`2026-09-30-index-status-once-design.md`) lands
first.** Both specs state this order. This spec is written against its
shape: `LiveIndex::status` is public, `progress` is `pub(crate)`, and
`is_building` and `awaits_first_build` are private. `IndexSync` reads
`status`, not the three accessors it deletes. D2 step 4 keeps comparing
`(revision, progress)`, which is in-crate; `Seen` adds `status`. The line
citations in Problem are to the code before index-status-once. no-vault-mode
lands its slice 2 after this spec (its Overlap section).

## Invariants now held by types

- **T1. No input outside a turn, no turn without its end.** `Turn` has no
  public constructor; `turn` ends it after the closure returns. L7's
  premise, "`is_building` changes only inside a turn", stops being a proof
  obligation on the caller: there is no other way in. The steps stay
  `pub(super)`, so the guarantee holds outside `live`; `live`'s own
  white-box tests still reach them (slice 5).
- **T2. The slots refill every turn.** No caller can forget it.
- **T3. One place starts a build.** `vault_changed` and a dirty apply both
  go through D3's gate, and `build` is an `Option`: at most one job per turn,
  and none while one runs (L6).
- **T4. The revision has one owner.** It moves iff an answer changed.
- **T5. One retired value per turn.** At most one build and one merge apply
  in a turn, since only one of each runs at a time (L6, `is_merging`). So
  `Retired` holds each part as an `Option`, and `Effects.retired` is `None`
  when all three are.

## Deviations from Zed

- **Effects are returned, not flushed.** gpui's `App::update` flushes
  `pending_effects` itself (`app.rs:1170-1177`). `inkra-index` has no
  executor and must stay gpui-free, so it returns `Effects` as data, and
  `IndexSync` is the flush.
- Zed has no index scheduler to compare with; the scheduler spec's
  deviations stand.

## Tests

The executable success criterion is written first, against the new
Interface, in `crates/index/src/live/`. It fails to compile before the
change (red); after, each test fails on its named fault.

A test executor replaces the three hand sequences: `Harness::turn(inputs)`
runs one turn and keeps its `Effects` in `pending` (loads, build, merge), and
helpers take one pending job, run it and hand it back in a new turn. The
existing pure tests keep their names and assertions; their bodies move from
`next`/`start_build`/`end_turn` to the executor.

New tests (`live/turn_tests.rs`):

| Test | Fault it catches |
|---|---|
| `a_landed_load_refills_its_slot_in_the_same_turn` | D2 step 1 skipped |
| `a_turn_fills_at_most_the_slots_in_vault_order` | refill ignores `slots` (replaces `next_load_fills_at_most_the_slots_in_vault_order`) |
| `a_dirty_build_returns_its_rerun_read_from_the_live_vault` | D2 step 3 skipped, or reads the build's snapshot |
| `a_vault_change_before_the_scan_starts_no_build` | D3 gate dropped |
| `a_layout_moves_the_revision_once` | bump missing or doubled in `apply_notes` |
| `a_failed_load_moves_the_revision` | `forget` does not bump |
| `a_stale_ticket_a_merge_and_a_mint_leave_the_revision` | bump on a superseded ticket, on `merged`, or on `saved` |
| `a_turn_that_changes_no_answer_does_not_notify` | `notify` always true |
| `an_mtimes_apply_notifies_through_progress_alone` | `notify` reads only `revision` (an `Mtimes` apply that queues a note moves progress, not the revision) |
| `an_idle_turn_retires_nothing` | `retired` always `Some` |
| `a_save_minted_in_its_own_turn_does_no_work` | T1's end steps the startup, refills, starts a build or a merge, or notifies on a mint that made none of them due: the mint's `Effects` has no loads, no build, no merge, no `retired`, and `layout`, `notify`, `indexed` false |
| `a_save_minted_with_a_free_slot_refills_it` | the same test passes vacuously: with a queued note and a free slot, the mint's turn returns that note's load, so the empty-`Effects` case above is a real assertion |

`live/model_tests.rs` executes `Effects` instead of calling steps. Its ops
become: vault edits, `VaultChanged`, `RunBuild`, `Land(at)`, `Fail(at)`,
`Save`, `LandSave(at)` and a new `RunMerge` (at most one merge runs). Every
load reads its file in the turn that returned it, as `StartLoad` did: a
separate `Read` op lets renames land another note's text under the
loading id, the race the scheduler spec defers ("A load's read checked
against its note"). One new check after every turn: D5's `Seen` equality
when `notify` is false. One new check after draining: `indexed` was
reported exactly once over the run. `words_settled()` is not checked: once
startup is `Done` only due merges run, so a save during the settle stays
fresh by design (`a_save_during_the_settle_stays_fresh_and_indexed_is_reported`). The merge and settle
paths enter the random interleavings for the first time.

App tests:

- **Die, with a pure successor:** `tests.rs`
  `a_build_applying_changes_the_revision_once` and
  `a_failed_load_changes_the_revision` (→ the two revision tests);
  `a_build_landing_behind_the_vault_applies_then_rebuilds` (→
  `a_dirty_build_returns_its_rerun_…`, with the model's held builds);
  `startup_tests.rs`
  `a_startup_load_ending_in_a_dirty_rebuild_settles_before_it_is_marked_indexed`
  and `words_tests.rs`
  `a_startup_load_ending_in_a_rebuild_settles_before_it_is_marked_indexed`
  (→ a pure `a_dirty_rebuild_ending_the_load_settles_before_indexed` plus the
  model's `indexed`-once check); `loading_tests.rs`
  `a_note_queued_twice_is_loaded_once` and
  `a_note_queued_while_it_loads_waits_for_its_load` (their pure namesakes in
  `live/tests.rs` already exist; the refill wiring they also covered is now
  `LiveIndex`'s).
- **Stay, as the Adapter's wiring:** every `Timeline` test in
  `startup_tests.rs` but the dirty-settle one; `loading_tests.rs`
  `at_most_one_load_per_core_runs_at_once` (the `num_cpus * LOADS_PER_CORE`
  product, still a mutant exclusion), the three turn-budget tests
  (`apply_budget`, `turns`, `drain_yields_…`), and the rename and failure
  integration tests; `watch/tests.rs`; `a_rebuild_alone_is_not_building`
  (it now also pins D5: a rename's layout notifies); the gpui random test.

Run `./scripts/test-names.sh` before slice 1 and after each slice. Every
name that leaves `app` has a successor above.

## Plan

Each slice is one commit and passes `./scripts/check.sh`. Test first, red,
then the change.

1. **The revision moves in.** Tests: the three revision tests in
   `live/tests.rs`, calling today's methods. Change: `LiveIndex::revision`
   and its three bumps (D4); `IndexSync::revision` delegates, plus the
   `#[cfg(test)]` offset behind `simulate_load`; `land` and `apply_build`
   stop bumping. Delete the two app revision tests. About 90 lines.
2. **The turn, in `inkra-index`.** Tests: `live/turn_tests.rs` but the
   build rows, and the executor in `Harness`, which only those tests use yet.
   Change: `LiveIndex::new(slots)`, `LiveIndex::turn`, `Turn`, `Effects` with
   `loads`, `merge`, `retired`, `layout`, `notify`, `indexed`, D2 steps 1, 2
   and 4, and T5's `Retired`. `Turn::built` returns `dirty` as a temporary
   `bool` (as built: not an `Effects.dirty` field, since a fourth `bool` on
   `Effects` trips clippy's pedantic `struct_excessive_bools`). The old steps stay public and
   `IndexSync` still calls them: no shim is written, the two entries coexist
   for one commit. About 140 lines.
3. **`IndexSync` runs turns.** Change: `IndexSync::turn` and `run` replace
   `timed_turn`, `end_turn`, `load_more`, `apply_merge` and `land`; it runs
   the `dirty` that `Turn::built` returns, as today. The old steps become `pub(super)`, so only
   `live`'s own tests still reach them. Delete the two `loading_tests`
   duplicates. About 90 lines.
4. **The rebuild is an effect.** Tests: the build rows of
   `turn_tests.rs`, and the pure dirty-settle test. Change: D2 step 3, D3;
   `Turn::built`'s `bool` becomes `Effects.build`; `reconcile` shrinks to
   `self.turn(cx, Turn::vault_changed)` (as built: a typed closure, since the
   path binds one `Turn` lifetime and fails the higher-ranked bound, and clippy
   rewrites an untyped closure into the path). Delete
   `a_build_landing_behind_the_vault_applies_then_rebuilds` and the two app
   dirty-settle tests. About 110 lines.
5. **The pure tests run effects.** Rewrite `live/tests.rs` and
   `live/startup_tests.rs` from the steps onto the executor; names and
   assertions stay. No production line changes, so the scheduler spec's
   "Faults injected" table is injected before and after this commit and must
   fail the same tests. About 150 lines. (As built: the steps keep
   `pub(super)`. `turn.rs` is a sibling of `builds.rs`, `loads.rs` and
   `startup.rs` and calls them, so a private step would not compile there.
   Three white-box tests call steps outside a turn on purpose, since no turn
   produces their scenario: `a_save_minted_with_a_free_slot_refills_it`
   builds without a turn's refill, `a_second_build_in_one_turn_is_refused`
   feeds two independent builds into one turn, and `model_tests.rs` starts
   loads by hand until slice 6.)
6. **The model runs effects.** Rewrite `live/model_tests.rs` onto the
   executor, add `RunMerge`, D5's `Seen` check after every turn, and the
   `indexed`-once drain check. Drop `.cargo/mutants.toml` exclusions that
   name functions this spec deleted (`load_more`; check `apply_landed`). Inject D2's three step faults, T5's, and a `notify` that
   ignores `progress`; record which tests fail in "As built". Update the
   scheduler spec's "Ordering" section to point here, add this spec to
   `CLAUDE.md`'s docs list. Run `cargo mutants` on `inkra-index`; zero
   missed. Take the performance figures. About 120 lines.

## Performance

A turn gains two `progress()` reads (D2 step 4), each O(loads in flight),
at most the core count. The refill and `end_turn` already run once per turn.
A save's mint becomes a turn: one `words_merge(Due)` policy check and one
refill check that returns at once when the slots are full. `Effects.retired`
is `None` on an idle turn, so no drop is spawned for nothing. Criterion, as
the scheduler spec's: on the 50,000-note vault of
`crates/app/examples/startup_vault.rs`, release, the launch line's `longest
turn` stays under 8 ms and within noise of `main`, three runs each; first
frame stays under 100 ms. Spawning happens inside the timed turn, as the
loads' spawns do today. As built, `longest turn` no longer includes gpui's
effect flush: `main` timed the whole `AsyncApp` update, so its figure also
counts the `notify` observers and the `NotesChanged` subscribers. Read the
comparison with `main` with that gap in mind.

Before the scan ends, D3 turns each vault notify into a full turn where
`reconcile` returned after one check. The vault notifies once per swap
(`vault/scans.rs:134`), so once per full-pass chunk before `is_scanned`.
Each such turn refills from an empty queue, checks the merge policy (arithmetic,
`words/policy.rs:9-19`) and diffs two snapshots. Second criterion, same
vault and runs: record the count and summed foreground time of the turns
that run before `is_scanned`, on this branch and on `main`. Each stays under
8 ms, and the sum stays within noise of `main`. If it does not, the fix is
in the turn's end, not a second gate in `IndexSync`: D3 keeps one place that
decides.

## Risks

- **Fewer notifies hide a reader.** D5's `Seen` check covers every getter
  of `LiveIndex`, and `IndexSync`'s getters only delegate. The residual
  risk is a new mutable `IndexSync` field with its own getter, against the
  rule written on that block. `a_rebuild_alone_is_not_building` and the
  views' refresh tests stay as the app-level oracle.
- **The harness rewrite goes vacuous.** About 560 lines of pure tests move
  from calling steps to the executor, alone in slice 5. Each fault in the
  scheduler spec's "Faults injected" table is injected before and after it
  and must fail the same tests.
- **The turn's order drifts.** D2 fixes it in one function; the model's
  `indexed`-once check and the six L7 tests pin it.
- **A borrow of `vault` across the closure.** `turn` borrows `vault.read(cx)`
  and `self.live` at once; they are distinct fields, and `run` takes `cx`
  only after `turn` returns. If the drain's closure needs `cx`, the design is
  wrong: it needs only the channel.
- **Two entries for one commit.** In slice 2 the old steps and `turn` are
  both public. It is one commit, `IndexSync` uses only the old steps, and
  slice 3 closes them; a `Turn` without its end was the other option, and it
  would publish a half Interface.

## Out of scope

- `IndexWatch`'s throttle and its key, `revision` and whether `status` is
  ready (index-status-once).
- The drain's `APPLY_BUDGET` loop and `yield_now`: they read a clock, so
  they stay in `app`.
- The last test hooks in one probe, `NoteSet::build(&[Entry])`, the diff in
  `inkra-vault`, and the load-read-checked-against-its-note race: the
  scheduler spec's Deferred rows keep their triggers.
- Moving `saved`'s parse into `inkra-index`: it runs off the foreground, and
  where it runs is the Adapter's choice.

## Review

Red/blue review, 2026-09-30. Red and blue agreed on all four findings; the
code was checked where they differed on weight.

| Finding | Outcome |
|---|---|
| R1 notify completeness unenforced | **Fixed.** D5 is now a checked property: the model's `Seen` snapshot over every `LiveIndex` getter must not change on a `notify`-false turn, and `IndexSync`'s getters are held to delegation by a rule on their block. Checked: `IndexSync::vault` is set once in `new` (`index.rs:60`), so the two getters that read it stay covered. |
| R2 Slice 2 not independently green | **Fixed.** Blue's shim commit rejected: it writes code only to delete it. The split instead lets the old steps and `turn` coexist for one commit (slice 2 in `inkra-index`, slice 3 in `IndexSync`), and moves the pure-harness rewrite to its own test-only slice 5, where re-injecting the scheduler spec's faults compares like with like. Six slices, each at or under about 150 lines. |
| R3 pre-scan vault changes run full turns | **Fixed.** Performance gains a second criterion: count and summed time of the turns before `is_scanned`, against `main`. Blue's point kept: no second gate in `IndexSync`, so D3 stays the one place. |
| R4 a save's mint becomes a turn | **Fixed.** Red and blue differed on weight: red called it an undisclosed behavior change, blue a disclosed and tested cost. Arbitration: the cost was disclosed (Performance), but the test only asserted "reports nothing". It is now `a_save_minted_in_its_own_turn_does_no_work`, asserting an empty `Effects`, with a sibling that shows the refill does fire when a slot and a note are free. |

Improvements:

- **Status-once pulled in: rejected.** It is its own spec with its own
  Interface change. `Seen` works over either shape; Out of scope notes how
  it shrinks if status-once lands first.
- **D5 as a checked property: adopted** (R1).
- **Slice 2 split by default: adopted**, in the form above (R2).
- **Pre-scan measurement: adopted** (R3).
- **Stronger mint test: adopted** (R4).
- **Why `loads` is a `Vec` and `build`/`merge` are `Option`s: adopted**, one
  paragraph under D1.

Verdict: proceed.

Cross-spec reconciliation (2026-09-30): new Merge order section:
index-status-once lands first; the getter block lists `status` in place of
`progress`, `awaits_first_build` and `is_building`; D5's argument and
`Seen` read `status`; the Out of scope "if it lands first" bullet is folded
into Merge order.
- Implementation (2026-09-30), slice 6: the model reads each load in its own turn (no `Read(at)`), and drops the `words_settled` drain check; both contradicted the deferred rename race and two startup tests. `RunMerge` takes no index.

## As built

**Slice 6.** The model runs every input in a turn through the harness and
reads each load in the turn that returned it, so `Land` and `Fail` pick among
reads already taken. `RunMerge` runs the one pending merge. `.cargo/mutants.toml`
drops the `load_more` exclusion: the `num_cpus * LOADS_PER_CORE` product now
sits in `IndexSync::new`, which cargo-mutants does not mutate, so no exclusion
replaces it. The `apply_landed` exclusion stays: its budget comparison is
unchanged.

**Faults injected** in `LiveIndex::turn` and `Turn::built`, each against
`live::` only:

| Fault | Failing tests |
|---|---|
| D2 step 1 skipped: no refill | 33, among them every test that lands a load and the model |
| D2 step 2 skipped: no `end_turn` | 13: all seven `startup_tests`, `a_stale_ticket_a_merge_and_a_mint_…`, four `turn_tests`, and the model (`indexed` reported 0 times) |
| D2 step 3 skipped: a dirty apply reruns nothing | `a_dirty_rebuild_ending_the_load_…`, `a_build_asked_while_one_runs_…`, `a_build_landing_behind_the_vault_applies`, `a_dirty_build_returns_its_rerun_…`, the model (answers differ from a fresh build) |
| T5: `retired` always `Some` | `a_save_minted_in_its_own_turn_…`, `an_idle_turn_retires_nothing`, `a_vault_change_before_the_scan_does_no_work` |
| `notify` ignores `progress` | `an_mtimes_apply_notifies_through_progress_alone`, the model (`Seen` changed on a turn that did not notify) |

**Mutants.** On `inkra-index`: the spec's diff (`--in-diff` from `7635f6f`),
31 mutants, 26 caught, 5 unviable, 0 missed; every mutant under
`crates/index/src/live`, 121, 104 caught, 17 unviable, 0 missed; the rest of
the crate, 959, 728 caught, 186 unviable, 0 missed, 45 timeouts. Every
timeout is in `find/`, `source.rs` or `words/`, which this spec does not
touch; they were not examined here. The machine ran other builds (load 20
to 40) against the 20 s auto timeout.

**Performance.** Release, the 50,000-note nested vault, runs alternated with
`bba9340` (this branch's `main`), six each, on a machine shared with other
builds (load 24 to 44). Both binaries carried a probe that timed each
`reconcile` before `is_scanned`. Longest turn: 2.6–3.1 ms after, 2.5–2.7 ms
before. Indexed: 1.18–1.57 s after, 1.25–1.33 s before. First frame:
114–169 ms after, 110–146 ms before, over 100 ms on both under this load; the
first run of each pair was the slower one in either order. Turns before
`is_scanned`: 2 in every run on both. Their sum: 8–41 µs after, 0.1–0.2 µs
before; the largest one 38 µs, under 8 ms. The sum grows by tens of
microseconds, far below the launch line's run-to-run noise of tens of
milliseconds.
