# Runs' ledger: supersede and re-run rules without a window

Status: **proposed** (2026-09-30). A refactor: behaviour stays identical.

## Problem

`Runs` (`crates/app/src/picker/runs.rs`, 335 lines) holds two things in one
module: the rules that decide which run is current and what follows a scan,
and the gpui driver that spawns the scan and lands its rows.

- **The rules need no gpui, but live beside code that does.** `start`
  (`runs.rs:139-153`), `begin_scan` (`:156-172`) and `is_current` (`:174-176`)
  touch only `cancel`, `id`, `scan` and `next`. Two more rules are written
  inside generic gpui functions that mutate those fields directly:
  `landed` takes `scan` and reads `rerun` (`:271-276`), and
  `refresh_or_defer` sets `scan.rerun` or `next` (`:279-295`). All six sit in
  one module, so field privacy protects nothing: any driver line can write
  `scan` or `next`.
- **Every test of the rules is a full picker over a vault.** Search spec D7
  (a refresh never cancels a scan; one re-run follows) and supersede are
  tested by `#[gpui::test]`s that build a `MemFs`, a `NoteStore`, an
  `IndexSync` and a `QuickOpenDelegate` or `VaultSearchDelegate`, hold the
  scan through a test future, trigger `IndexSync::simulate_load`, and read
  counters: `quick_open/tests.rs:230-255`, `quick_open/tests/background.rs:74-97,180-216`,
  `quick_open/tests/refresh.rs:55-97,167-188,239-313`,
  `vault_search/tests/refresh.rs:46-74`. The two `a_stale_result_is_dropped`
  (`quick_open/tests.rs:209-228`, `vault_search/tests.rs:163-179`) test one
  generic line twice.
- **The tests reach through five test-only accessors and a test-only
  method.** `hold`, `cancel_flag`, `id`, `scan`, `started`
  (`runs.rs:310-334`) and `RunId::previous` (`:94-99`), with two
  `#[cfg(test)]` fields (`:111-116`) and a test branch in `begin_scan`
  (`:161-171`). `scan()` exposes the private `Scan` so
  `background.rs:189-195` can assert `rerun: true`.
- **`cancel_flag` reads the wrong flag.** It returns `Runs::cancel`, the
  flag of the current run, not the flag the scan received. A driver that
  passed a fresh `AtomicBool` to `D::search` passes every cancel assertion.

The friction is real but bounded: the rules are about 60 lines, already
concentrated. The gain is locality (every write to the run state in one
file), unit tests without a window, and fewer production `cfg(test)` hooks.

### The two options

1. **An in-memory `IndexDelegate` fake**, as `Letters` is for
   `PickerDelegate`. Rejected: `IndexDelegate::sync` returns
   `&Entity<IndexSync>` and `search` takes an `&IndexSnapshot`
   (`runs.rs:28,34-39`). A fake still builds `MemFs`, `NoteStore` and
   `IndexSync`, still needs the test hold, and tests the same generic driver
   the two real delegates already exercise. It removes none of the friction.
2. **Extract the rules into a plain struct with its own tests.** Chosen. It
   follows the precedent of `Lifecycle` (note-session spec) and `Scans`
   (scan-session spec): the rules as data, the driver executes decisions.

## Decision

### The deepened Module: `Ledger`, in `picker/runs/ledger.rs`

No gpui, no `IndexSync`, no picker. It owns the four fields and every rule
that writes them.

```rust
/// Which run is current, which scan is in flight, and what follows it. No gpui.
#[derive(Default)]
pub(crate) struct Ledger {
    cancel: Arc<AtomicBool>,
    id: RunId,
    scan: Option<Scan>,
    next: Run,
}

/// The run `Ledger::start` begins.
#[must_use]
pub(crate) struct Started {
    pub(crate) id: RunId,
    pub(crate) run: Run,
    /// Before the scan: `SEARCH_DEBOUNCE` behind a user scan, `NOTES_CHANGED_REFRESH`, or none.
    pub(crate) wait: Option<Duration>,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Ledger {
    /// Supersedes the run before: sets its cancel flag, takes the run a refresh set.
    pub(crate) fn start(&mut self) -> Started;
    /// Refreshes defer behind the started run until it lands.
    pub(crate) fn begin_scan(&mut self, started: &Started);
    pub(crate) fn is_current(&self, id: RunId) -> bool;
    /// Search spec D7: `None` while a scan is in flight, which re-runs once it lands;
    /// else `run`, which the next `start` takes.
    pub(crate) fn refresh(&mut self, run: Run) -> Option<Run>;
    /// The current run's rows landed: ends its scan, returns the re-run a refresh deferred.
    pub(crate) fn landed(&mut self) -> Option<Run>;
}

impl Drop for Ledger { /* sets the current run's cancel flag */ }
```

`Run`, `RunId`, `Scan`, `SEARCH_DEBOUNCE` and `NOTES_CHANGED_REFRESH` move in
with it. `Scan` becomes private to `ledger.rs`. `runs.rs` re-exports `Run`,
`RunId` and the two durations, so no delegate import changes.

Interface, in prose:

- **Ordering.** `start`, then either `begin_scan` (a scan follows) or nothing
  (the run settles: no rows to scan). `landed` is called once, for the
  current run only, after its rows show. `refresh` may come at any time.
- **Supersede.** `start` sets the cancel flag of every run before it. Only
  the newest `Started::cancel` is unset, until the `Ledger` drops.
- **D7.** `refresh` never sets a cancel flag. During a scan it returns
  `None` and marks one re-run; any number of refreshes mark one. A `start`
  during the scan drops the mark: the new run reads the newest index.
- **The wait** is computed once, in `start`, from the run and whether a user
  scan was in flight: `Run::wait` stays private.
- **No picker state.** The `Ledger` reads no picker field. Whether a
  confirmed picker skips the re-run is the caller's rule: `landed` always
  ends the scan and hands back the deferred run.
- **Error modes.** None. Calling `landed` for a stale run is a caller bug;
  the driver guards it with `is_current`, as today (see Composition for its
  fate under picker-owns-landing).

### The driver: `Runs`, in `picker/runs.rs`

```rust
#[derive(Default)]
pub(crate) struct Runs {
    ledger: Ledger,
    /// Loads add text, so rows; a notify refreshes at most every `BUILDING_REFRESH` while they land.
    watch: IndexWatch,
    /// The cancel flag each scan received, in order, so a test reads the flag the scan holds.
    #[cfg(test)]
    begun: Vec<Arc<AtomicBool>>,
    #[cfg(test)]
    hold: Option<Hold>,
}
```

`update_matches`, `apply`, `settle`, `observe` keep their signatures and
order. The changed bodies:

- `Runs::start(&mut self, sync)` is `ledger.start()` then `watch.seen(sync)`.
- `refresh_or_defer`: `if let Some(run) = ledger.refresh(run)` then
  `picker.refresh` or `refresh_in_background` by `run.is_user()`.
- `landed`: `let rerun = ledger.landed()`; if it is `Some(run)` and
  `!picker.confirmed`, `refresh_or_defer(picker, run, cx)`. The scan ends
  either way, as today's `scan.take()` does.
- The spawned task waits `started.wait` instead of computing it.

`cancel_flag`, `id`, `scan`, `started` and `RunId::previous` go. `begun()`
replaces `started()` (its length) and `cancel_flag()` (its last element),
and returns the flag passed to `D::search`, which closes the wrong-flag gap.

## What moves where

| From | To |
|---|---|
| `runs.rs` `Run`, `RunId`, `Scan`, `Started`, the two durations | `picker/runs/ledger.rs` |
| `Runs::{start, begin_scan, is_current}` bodies, `impl Drop for Runs` | `Ledger` |
| `refresh_or_defer`'s `scan.rerun`/`next` writes (`runs.rs:284-289`) | `Ledger::refresh` |
| `landed`'s `scan.take()` and `rerun` read (`runs.rs:272-273`) | `Ledger::landed` |
| `landed`'s `!picker.confirmed` | stays in the driver (picker-owns-landing: `Picker::land`) |
| `Run::wait` call in `update_matches` (`runs.rs:195`) | `Ledger::start`, as `Started::wait` |
| `cancel_flag`, `id`, `scan`, `started`, `RunId::previous` | deleted; `begun()` |

`runs.rs` shrinks by about 60 lines; `ledger.rs` is about 90.

## Invariants now held by types

- **Only `ledger.rs` writes the run state.** The four fields are private to a
  child module, so the generic driver cannot set `scan` or `next` by hand.
- **Refreshes decide through data.** The driver starts a run only when
  `refresh` or `landed` returns `Some(run)`. "A refresh never cancels a scan"
  is one function body, not a convention across two.
- **The wait cannot drift from the run.** `Started::wait` is fixed when the
  run starts; the driver has no `behind_user` to misread.
- **A test reads the flag the scan holds.** `begun` records
  `Started::cancel` as `update_matches` hands it to `D::search`.

## Composition with picker-owns-landing

The picker-owns-landing spec (`2026-09-30-picker-owns-landing-design.md`)
touches the same rules, and two of its invariants bear on the `Ledger`'s
Interface: "`Picker::confirmed` is read only in `Picker::land`; `Runs` no
longer reads a picker field", and "`is_current` goes: `pending` ownership
drops stale answers".

- **`confirmed` never enters the `Ledger`.** `landed` takes no argument and
  returns the deferred re-run. Today the driver checks `picker.confirmed`;
  under picker-owns-landing, `Runs::apply_matches` maps `ledger.landed()` to
  `Arrival::rerun` and `Picker::land` checks `confirmed`. The `Ledger` and
  its tests do not change for it.
- **`is_current` is the one method picker-owns-landing deletes.** It is a
  driver guard, not a rule the `Ledger` needs: supersede is tested on the
  cancel flags, and the proptest reads only flags and `landed`.
- **The `next` write stays in `ledger.rs`.** On a re-run,
  `Runs::apply_matches` passes the run `ledger.landed()` returns to
  `Ledger::refresh`. The scan has just ended, so `refresh` stores it as the
  next run and returns `Some`, which is `Arrival::rerun`. The
  picker-owns-landing spec never writes `next` by hand.
- **Kept selection is picker-owns-landing's.** Its rule reads
  `Picker::selected` and moves to `Picker`, where `Letters` (with keys)
  tests it through `PickerDelegate`. This spec adds no kept-selection
  module. If picker-owns-landing is dropped, the rule stays in `Runs::apply`
  (three lines), covered by the kept-selection tests listed below; a
  one-caller pure function would be indirection.

### Merge order

**This spec lands first.** It is a behaviour-identical refactor, and it gives
picker-owns-landing a `Ledger` whose `landed` already has the shape
`Arrival::rerun` needs. The picker-owns-landing spec then carries one stated
slice, not a rebase:

- delete `Ledger::is_current`, the `is_current` assertion in
  `a_start_cancels_the_run_before_and_supersedes_it`, and the driver guards
  in `apply` and `settle`;
- delete quick open's `a_stale_result_is_dropped`: `pending` ownership, which
  picker-owns-landing tests on `Letters`, replaces it;
- call `ledger.landed()` from `Runs::apply_matches`, through
  `Ledger::refresh`, for `Arrival::rerun`.

The order is fixed: the picker-owns-landing spec assumes the `Ledger` and
mirrors this section in its own Merge order. This spec deletes
`RunId::previous` and vault search's `a_stale_result_is_dropped` (slice 3);
picker-owns-landing does not delete them again.

### `Runs::scanning` stays in the driver

index-status-once adds `Runs::scanning`, the sticky "the last settled run
found no build" answer. It is a field of the driver `Runs`, not of the
`Ledger`: it answers from `IndexSync::status`, and the `Ledger` reads no
index and no gpui. Its writers are the driver's, as index-status-once states:
`update_matches` sets it when it settles a run on `Status::Scanning` and
clears it when `query` returns `None`; `apply` clears it when the current
run lands. Whichever of the two specs lands first, the other adds the field
beside `ledger` in `Runs` without touching `ledger.rs`.

## Deviations from Zed

- Zed's `FileFinderDelegate` keeps `search_count`, `latest_search_id` and
  `cancel_flag` inline (`vendor/zed-reference/crates/file_finder/src/file_finder.rs:362-371,1069-1106`)
  and tests them only through the whole finder over a `FakeFs`. We extract
  them, because we add D7's deferred re-run and the notify throttle on top,
  which Zed has no equivalent of; the query-run spec already moved that
  protocol out of the delegates.

## Tests

### New, the executable success criterion: `picker/runs/ledger/tests.rs`

Plain `#[test]`, written first against the Interface above. They fail to
compile before slice 1: this is a refactor, so the red is the missing Module,
and the green oracle for "no behaviour change" is the integration tests that
stay. Each names the plausible bug it catches.

- `a_start_cancels_the_run_before_and_supersedes_it`: two starts; the first
  flag is set, the second is not, `is_current` of the first is false.
  Catches: `start` not replacing the flag, or storing into the new one.
- `dropping_the_ledger_cancels_its_run`. Catches: `Drop` removed.
- `refreshes_during_a_scan_defer_to_one_rerun_and_never_cancel`: ten
  `refresh(Notify)` return `None`, the flag stays unset, `landed()` is
  `Some(Notify)` once, a second `landed` is `None`. Catches: a refresh that
  cancels, a rerun per refresh, a mark not cleared.
- `a_refresh_with_no_scan_starts_at_once_as_the_next_run`: `refresh(Refresh)`
  is `Some(Refresh)`; the next `start` has `run == Refresh` and
  `wait == Some(NOTES_CHANGED_REFRESH)`; the one after is `Search` again.
  Catches: `next` not taken.
- `a_start_during_a_scan_drops_its_rerun`: scan, refresh (deferred), start,
  scan, `landed()` is `None`. Catches: the mark surviving a supersede.
- `a_keystroke_waits_only_behind_a_user_scan`: `Search` with no scan waits
  nothing; during a user scan, `SEARCH_DEBOUNCE`; during a `Notify` scan,
  nothing; after a settle (start, no scan), nothing. Catches: a wrong
  `behind_user`.
- `landing_ends_the_scan`: scan, no refresh, `landed()` is `None`, and a
  `refresh` after it is `Some`. Catches: the scan left in flight, so every
  later refresh defers forever. The confirm rule is the driver's; its oracle
  is the kept `enter_during_a_dirty_scan_confirms_without_a_rerun`.
- `the_rerun_waits_nothing`: the run `landed` returns starts with no wait.
- Proptest `only_the_newest_run_is_uncancelled`: any sequence of `start`
  (with or without `begin_scan`), `refresh(run)` and, while a scan is in
  flight, `landed()`; after each step every flag handed out but the
  newest is set, `refresh` never changes a flag, and `landed` returns `Some`
  at most once per scan and only if a `refresh` came during it. Commit the
  `.proptest-regressions` file.

Mutation check: `cargo mutants -p inkra-app --test-package inkra-app -f crates/app/src/picker/runs/ledger.rs -- picker::runs::ledger`,
zero missed. Without `--test-package inkra-app`, `.cargo/mutants.toml`'s
`test_package` list runs five crates that never reach this file: a vacuous
green.

### Integration tests

Die, with the unit test that replaces each:

| Test | Replaced by |
|---|---|
| `quick_open/tests.rs` `a_new_query_cancels_the_scan_before_it` | `a_start_cancels_the_run_before_and_supersedes_it` |
| `quick_open/tests/background.rs` `a_keystroke_cancels_a_background_refresh_and_waits_no_debounce` | `a_keystroke_waits_only_behind_a_user_scan`; the timer wiring stays in `an_idle_picker_answers_at_once_and_a_busy_one_waits` |
| `quick_open/tests/background.rs` `a_keystroke_cancels_a_stale_background_refresh_and_no_rerun_follows` | `a_start_during_a_scan_drops_its_rerun` |
| `vault_search/tests/refresh.rs` `a_refresh_during_a_scan_waits_for_it` | `refreshes_during_a_scan_defer_to_one_rerun_and_never_cancel`; the generic driver's notify path stays under quick open's `ten_notifies_…` |
| `vault_search/tests.rs` `a_stale_result_is_dropped` | quick open's copy, which tests the same generic line |

Stay, as the wiring oracle, each on `begun()` instead of `cancel_flag`/`started`:

- Notify path: `ten_notifies_during_a_scan_cause_no_cancel_and_one_rerun`.
- `NotesChanged` path: `notes_changed_during_a_scan_reruns_once`.
- Keystroke supersede: `a_keystroke_during_the_scan_cancels_it`, now
  asserting the flag the first scan received.
- First build: `a_build_during_the_first_scan_does_not_cancel_it`.
- Confirm: `enter_during_a_dirty_scan_confirms_without_a_rerun`,
  `enter_during_a_background_refresh_confirms_the_row_shown_at_once`.
- Drop: `dropping_the_picker_cancels_its_scan` (query-run spec's guard),
  asserting `begun().last()`.
- Stale: quick open's `a_stale_result_is_dropped`, with `RunId::default()`:
  `Picker::new` starts run 1, so run 0 is never current.
- Kept selection (picker-owns-landing's area, unchanged here):
  `the_selected_note_is_kept_across_a_load_refresh`,
  `a_background_refresh_keeps_the_selection_and_the_scroll`,
  `a_refresh_that_drops_the_kept_note_selects_and_reveals_the_first_row`,
  `the_selection_survives_the_footer_leaving`,
  `vault_search` `a_refresh_during_loads_keeps_the_selected_hit`.
- Counting tests on `begun().len()`: `one_build_runs_one_scan`,
  `the_footer_counts_the_notes_indexed_then_leaves`,
  `only_a_keystroke_s_run_is_logged`, `vault_search` `states.rs` `scans`.

`./scripts/test-names.sh` is diffed before and after each slice; every
removed name appears in the table above.

## Plan

Each slice merges alone with `./scripts/check.sh` green.

1. **`Ledger` owns supersede.** Write `ledger/tests.rs` red: the start,
   drop, wait and stale-id cases. Add `ledger.rs` with `Run`, `RunId`,
   `Scan`, `Started`, the durations, `start`, `begin_scan`, `is_current`,
   `Drop`. `Runs` holds a `Ledger`; `update_matches` reads `Started::wait`.
   The re-exports keep every delegate and test import; `Scan` stays
   `pub(crate)`. `Runs`' test accessors keep their names and callers: the
   fields they read now live in a child module, which its parent cannot
   see, so `Ledger` gets three scaffold accessors,
   `#[cfg(test)] pub(super) fn {cancel_flag, id, scan}`, the
   `sidebar/edit.rs` pattern. The fields stay private and only `ledger.rs`
   writes them. `started` stays a `Runs` field. No integration test
   changes: all green is the oracle. About 160 lines.
2. **`Ledger` owns D7.** Write the refresh, rerun, landing tests and the
   proptest red. Add `refresh` and `landed`; `refresh_or_defer` and
   `Runs::landed` call them and no longer touch the fields. About 130 lines.
3. **The test hooks shrink.** `begun` replaces `started`; the remaining
   integration tests move onto it; the five tests in the table die;
   `Runs::{cancel_flag, id, scan}`, the three `Ledger` scaffold accessors,
   `RunId::previous` and the `cancel_flag` helper in `quick_open/tests.rs`
   go; `Scan` becomes private to `ledger.rs`. Run the mutation check. Add
   this spec to CLAUDE.md's Docs list and point the query-run spec's `Runs`
   section here. About 150 lines, mostly deletions.

The picker-owns-landing spec, landing after, carries the `is_current`
slice listed under Merge order.

## Risks

- **Unit tests green, wiring broken.** A driver that skipped `begin_scan` or
  called `landed` before `show` passes every ledger test. The integration
  tests kept above cover each entry path (notify, `NotesChanged`, keystroke,
  first build, confirm, drop); do not prune them further.
- **An Interface conflict with picker-owns-landing.** Bounded by Merge order:
  `landed` has the same shape in both orders, and only `is_current` and one
  stale test depend on the order.
- **The scaffold accessors outlive slice 3.** They are `cfg(test)` and
  listed by name in slice 3's deletions; `test-names.sh` does not catch
  them, so the reviewer greps `ledger.rs` for `cfg(test)` at slice 3.
- **Coverage loss from the deleted tests.** Each deleted test's rule has a
  unit test, and each deleted wiring path has a kept test. Vault search keeps
  no D7 test of its own: it reaches D7 only through the generic driver and
  `Runs::observe`, whose throttle `states.rs` still covers.
- **Performance.** None: `start` does the same work (one `Arc` swap, one id
  bump). `begun` exists only under `cfg(test)`.

## Out of scope

- Kept selection and the landing order: picker-owns-landing.
- The other production `cfg(test)` hooks: `QuickOpenDelegate::{hold_excerpts,
  logged}` (`quick_open/mod.rs:40-45`), `BUILDS`/`DRAWS` (`quick_open/render.rs:11-17`),
  and those in `sidebar` and `backlinks`. They are not `Runs`'.
- `hold` itself: a scan in flight is a gpui task; only the driver can hold
  it. It stays one `cfg(test)` field.
- `IndexWatch`'s throttle: already its own module with its own tests.
- Moving the ledger into a gpui-free crate: its vocabulary (`Search`,
  `Refresh`, `Notify`, the debounce) is the picker's, not the index's.

## Review

Red/blue on 2026-09-30. Red and blue agreed on all three findings; each was
checked against the code and picker-owns-landing's spec.

- **RED-1 (blocker): the Interface conflicts with picker-owns-landing.** Fixed.
  The picker-owns-landing spec bans `Runs` from reading `Picker::confirmed` and deletes
  `is_current`. `Ledger::landed` now takes no argument, so the `Ledger` never
  sees `confirmed` in either order. `is_current` is the only piece that
  depends on the order; Merge order fixes that this spec lands first and
  names the slice picker-owns-landing carries. Also found: picker-owns-landing's
  `apply_matches` writes `next` by hand, which would break "only `ledger.rs`
  writes the run state"; Composition routes that write through
  `Ledger::refresh`. The picker-owns-landing spec needed a matching note
  (added in the cross-spec reconciliation below).
- **RED-2 (major): slice 1 could not compile.** Fixed. Confirmed: a parent
  module cannot read a child module's private fields. Slice 1 now adds three
  `#[cfg(test)] pub(super)` scaffold accessors on `Ledger` (the
  `sidebar/edit.rs` pattern), and slice 3 deletes them by name. The fields
  stay private.
- **RED-3 (minor): the mutation command omitted `--test-package inkra-app`.**
  Fixed, with the reason inline.

Blue's improvements:

- Explicit merge order: adopted (Merge order). Only this spec's section is
  written; picker-owns-landing's author mirrors it.
- `landed` without `confirmed`: adopted. It shrinks the Interface and makes
  the `Ledger` identical in both merge orders.
- Merge slice 3's accessor work into slice 1: rejected. Slice 3's deletions
  depend on slice 2's unit tests (`a_start_during_a_scan_drops_its_rerun`
  replaces the only `scan()` caller, `background.rs:25`), so the merge would
  make one slice of about 440 lines. The scaffold accessors close the
  compile gap at 10 lines.
- A compile-level acceptance check for slice 1: rejected as redundant.
  `check.sh` builds every test target; the gap was in the paper, fixed by
  naming the accessors.
- A `check.sh` grep of specs for `cargo mutants` lines: rejected. One
  omission in prose is not a pattern, and `check.sh` checks code, not
  specs. The `.cargo/mutants.toml` comment already documents the rule.

Verdict: proceed.

Cross-spec reconciliation (2026-09-30): candidate numbers replaced by spec
slugs; `Landing::rerun` is now `Arrival::rerun`; Composition routes the
re-run through `Ledger::refresh` explicitly; Merge order drops the "picker
first" fallback, since picker-owns-landing now assumes the `Ledger`; new
section: `Runs::scanning` (index-status-once) stays in the driver, not the
`Ledger`.
