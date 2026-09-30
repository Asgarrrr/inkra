# The picker owns the matches-landed handshake

Status: **built 2026-09-30** (As built, below). Refactor: quick open, vault search and the
command palette behave as before; a query answered at once lands inside the
refresh that asked for it.

## Problem

`Picker` decides when `enter` may confirm, but a delegate decides when the
rows answer the current query. The two meet through a callback protocol that
no type states:

- **Only the delegate clears the picker's flag.** `refresh` sets
  `awaiting_matches` (`picker/mod.rs:117-120`); only `matches_updated` or
  `matches_refreshed` clears it (`picker/mod.rs:129-139`, `149-155`). A
  delegate that never calls back leaves `enter` in `confirm_on_update`
  forever (`picker/mod.rs:198-204`), and it compiles.
- **The rule lives in no signature.** "Call exactly once, after the rows are
  set, with the picker not borrowed" is part of the Interface. The trait
  (`picker/mod.rs:28-52`) says only that `update_matches` returns a
  `Task<()>` the picker holds; the field comment on `awaiting_matches`
  (`picker/mod.rs:62`) is the only trace.
- **Every caller escapes the borrow its own way.** `update_matches` runs with
  the picker borrowed, so a delegate that has its rows at once cannot land
  them. The command palette defers through `App::defer` on a weak handle
  (`command_palette/mod.rs:92-98`, palette spec D3). `Runs` spawns a task
  only to call back after the borrow ends, for runs that scan nothing
  (`picker/runs.rs:260-268`). The test delegate `Letters` calls back from
  its spawn (`picker/letters.rs:76-83`).
- **The ordering after the rows land is split across two modules.**
  `Runs::apply` reads the selection before `show`, picks
  `matches_refreshed(ix)` or `matches_updated(0)`, then calls `Runs::landed`
  last, which reads `Picker::confirmed` to skip a deferred re-run after a
  confirm (`picker/runs.rs:232-276`). The query-run spec named that
  ordering as the defect it removed from the delegates
  (`2026-09-29-query-run-design.md` lines 23-36); it moved into `Runs`, but
  still across the picker's boundary.
- **A stale answer is filtered twice.** Replacing `Picker::pending`
  (`picker/mod.rs:125`) drops the previous task, and `Runs::apply` also
  checks `is_current` (`picker/runs.rs:237`, `263`). The two
  `a_stale_result_is_dropped` tests (`quick_open/tests.rs:211-227`,
  `vault_search/tests.rs:162-178`) reach past the Interface to call
  `Runs::apply` with an old `RunId`.

Deletion test: delete `matches_updated`, `matches_refreshed` and
`matches_landed` from `Picker`, and the landing reappears in each of the three
paths above, each with its own borrow escape. The Module is shallow: its
Interface is a protocol, not a type. Three real paths (palette, `Runs` for two
delegates) and one test fake use it, so the Seam is real.

## What Zed does

Zed's delegate sets its matches and its own `selected_index` inside the task
`update_matches` returns. The picker wraps that task, awaits it, then calls
its own `matches_updated` and runs the queued confirm
(`crates/picker/src/picker.rs:1234-1270`). It also calls `matches_updated`
at once after `update_matches` returns (`picker.rs:1243`), so a delegate
that set its matches synchronously shows them in the same turn. No delegate
calls the picker back. `PendingUpdateMatches` keeps the delegate's task in a
slot so that a new query drops it synchronously before its first poll
(`picker.rs:1244-1258`). `finalize_update_matches` lets `enter` block up to
16 ms on a pending query (`picker.rs:237-246`, `1035-1047`).

We follow the principle: the picker lands, the delegate only answers.

## Decision

### The deepened Module: `Picker` lands every answer

A delegate's `update_matches` computes an answer; its `apply_matches` puts
that answer in its rows and says which row to select. The picker alone calls
both, in one foreground turn, then selects, scrolls, clears its flag, runs a
queued confirm, and starts a deferred re-run. A delegate holds no handle to
the picker and never calls it back.

```rust
// picker/mod.rs

/// A query's answer: at once, or from a task the picker holds.
pub(crate) enum Update<T> {
    /// Applied and landed before `refresh` returns.
    Ready(T),
    /// Applied and landed when it finishes with `Some`. `None`, or dropping it, lands nothing.
    Pending(Task<Option<T>>),
}

/// What the picker does once the rows are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Arrival {
    /// Selects the first selectable row from here, else the nearest before it.
    pub(crate) row: usize,
    /// The same answer refreshed: the list keeps its scroll.
    pub(crate) keep_scroll: bool,
    /// The data changed under this answer: re-run in the background, unless confirmed.
    pub(crate) rerun: bool,
}

impl Arrival {
    /// A new answer: its first selectable row, scrolled to.
    pub(crate) fn first() -> Self;
}

pub(crate) trait PickerDelegate: Sized + 'static {
    /// One answer, from `update_matches` to `apply_matches`.
    type Matches: 'static;
    // key_context, label, placeholder, empty_text, match_count, is_selectable,
    // confirm, render_row, row_label, render_footer, row_height, rows_shown: unchanged.

    /// The picker holds a `Pending` task, so the next query drops it.
    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Update<Self::Matches>;
    /// Replaces the rows with `matches`; `selected` is the row selected now, before they change.
    fn apply_matches(
        &mut self,
        matches: Self::Matches,
        selected: Option<usize>,
        cx: &mut Context<Picker<Self>>,
    ) -> Arrival;
}
```

`Picker`'s side, private:

```rust
fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
    match self.delegate.update_matches(&self.query, cx) {
        Update::Ready(matches) => {
            self.pending = None;
            self.apply(matches, cx);
        }
        Update::Pending(task) => {
            self.pending = Some(cx.spawn(async move |picker, cx| {
                let Some(matches) = task.await else {
                    return;
                };
                picker.update(cx, |picker, cx| picker.apply(matches, cx)).ok();
            }));
        }
    }
}

fn apply(&mut self, matches: D::Matches, cx: &mut Context<Self>) {
    let arrival = self.delegate.apply_matches(matches, self.selected, cx);
    self.land(arrival, cx);
}

/// Select, scroll, clear the flag, the queued confirm, then the re-run: this order is the Interface.
fn land(&mut self, arrival: Arrival, cx: &mut Context<Self>);
```

`refresh` still sets `awaiting_matches` and calls `refresh_in_background`.
Both stay `pub(crate)` for `Runs::refresh_or_defer` and quick open.
`matches_updated` and `matches_refreshed` go; `select_from` and
`reveal_selected` stay private.

The Interface a delegate author must know, in full:

- `update_matches` may mutate the delegate but must not assume its rows are
  shown until `apply_matches` runs.
- A `Pending` task finishes with `Some(answer)`, finishes with `None`, or
  its owner drops it. `None` means "superseded, no answer": the picker lands
  nothing and leaves `awaiting_matches` set, so a queued `enter` waits for
  the next answer. Return `None` only when a newer run exists or the picker
  is going away; the type states that an answer may be absent, and this
  rule states when.
- `apply_matches` runs on the foreground with the picker borrowed, once per
  answer that lands, and never for a dropped one.

### The implementers

- **Command palette.** `type Matches = Vec<Row>`. `update_matches` builds
  and sorts the rows as now and returns `Update::Ready(rows)`;
  `apply_matches` sets them and returns `Arrival::first()`. The `App::defer`
  block and the weak handle go (`command_palette/mod.rs:92-99`).
- **`Letters` (test).** `type Matches = (String, Vec<&'static str>)`. A new
  field `answer: Answer` picks the shape: `Later` (default) returns
  `Pending` of a task that answers `Some` after `DELAY`; `AtOnce` returns
  `Ready`; `Nothing` returns `Pending` of a task that answers `None` after
  `DELAY`. The picker tests cover all three. `apply_matches` pushes to
  `applied` and sets the rows.
- **`Runs`, for quick open and vault search.**
  - `type Matches = Ran<Self::Found>`, where
    `pub(crate) enum Ran<F> { Settled, Found(Landed<F>) }`. `Settled` means
    `scanning` or `query` already set the rows; the answer is only the
    landing. Each delegate declares the type and forwards `apply_matches` to
    `Runs::apply_matches`, the same forwarding it does for `update_matches`.
  - `Runs::update_matches` returns `Ready(Ran::Settled)` where it now calls
    `settle`. Otherwise it returns `Pending` of one `background_spawn`
    task: the wait, the test hold, then `D::search`. That task replaces
    today's foreground task and its inner background task.
  - When `search` returns `None`, the run was cancelled, and the task
    returns `None`, as today's `let Some(found) = found.await else { return; }`
    does. `cancel` is set only by `Runs::start` for a newer run or by the
    scan's drop, which is the `None` rule of the Interface.
  - `Runs::apply_matches(delegate, ran, selected, cx) -> Arrival` is today's
    `apply` without the `is_current` guard. It reads the kept key from
    `selected` on a non-user run, calls `show`, and finds the kept row. It
    calls `ledger.landed()`, which ends the scan, and passes the deferred run
    to `ledger.refresh`, which stores it as the next run; `rerun` is whether
    that returned `Some`. `Runs` writes no `Ledger` field by hand
    (runs-testable, Composition).
  - `settle`, `landed` and `Ledger::is_current` go.

### Invariants now held by types

- **Exactly one landing per answer, by the picker.** A delegate holds no
  handle to the picker and cannot call back to land its own answer:
  `apply_matches` returns an `Arrival` instead of invoking a picker method,
  so it cannot forget to land, land twice, or land while the picker is
  borrowed elsewhere. `matches_updated`, `matches_refreshed` and
  `matches_landed` stop being callable from outside the picker.
- **A superseded answer never applies.** Every `update_matches` replaces
  `pending`, and `Ready` clears it, so the only task that can reach `apply`
  is the current one. A cancelled computation has a value, `None`, which
  the picker's wrapper drops by pattern, not by a parked future. `RunId` stays for quick open's excerpts
  (`quick_open/list.rs:46`, `115`); it no longer guards `apply`.
- **The re-run follows the queued confirm and never follows a confirm.**
  Both are steps of `land`, in that order. `Picker::confirmed` is read only
  there; `Runs` no longer reads a picker field.
- **The kept selection is read at landing.** The picker passes `selected`
  into `apply_matches` in the same turn as the rows change, as `Runs::apply`
  did (`picker/runs.rs:240`).
- **A query answered at once never queues `enter`.** `Ready` lands before
  `refresh` returns, so `awaiting_matches` is never observed `true` for it.

## What moves where

| Now | After |
|---|---|
| `Picker::{matches_updated, matches_refreshed, matches_landed}` | `Picker::land(Arrival)`, private |
| Palette `App::defer` + weak handle | `Update::Ready`; the picker applies it |
| `Runs::settle` | `Update::Ready(Ran::Settled)` |
| `Runs::apply` | `Runs::apply_matches`, returns `Arrival` |
| `Runs::landed` + `picker.confirmed` read | `Arrival::rerun`, from `Ledger::landed` through `Ledger::refresh`; `land` checks `confirmed` |
| `Ledger::is_current` and its guards in `apply` and `settle` | gone: `pending` ownership drops stale answers |
| Foreground task + inner background task in `Runs::update_matches` | one background task; the picker's foreground wrapper |
| Cancelled run: foreground task returns early | background task returns `None`; the wrapper lands nothing |

## Merge order

**runs-testable (`2026-09-30-runs-testable-design.md`) lands first.** This
spec assumes its `Ledger` (`picker/runs/ledger.rs`): `Runs` holds a
`Ledger`, `Ledger::landed` takes no argument and returns the deferred
re-run, and vault search's `a_stale_result_is_dropped` and `RunId::previous`
are already deleted. Line citations in Problem are to the code before
runs-testable. This spec carries the one slice runs-testable hands over, not
a rebase:

- delete `Ledger::is_current`, the `is_current` assertion in
  `a_start_cancels_the_run_before_and_supersedes_it`, and the driver guards
  in `apply` and `settle` (slice 3);
- delete quick open's `a_stale_result_is_dropped`: `pending` ownership,
  tested on `Letters`, replaces it (slice 3);
- call `ledger.landed()` from `Runs::apply`, later `Runs::apply_matches`,
  for `Arrival::rerun` (slice 1).

Every write of `next` goes through `Ledger::refresh`. `Runs` never sets
`next` or `scan` by hand; only `ledger.rs` writes the run state.

`Runs::scanning` (index-status-once) is a field of the driver `Runs`, not
of the `Ledger`. If index-status-once has landed, the `Settled` answer on
`Status::Scanning` sets it where the `scanning` hook ran, and
`Runs::apply_matches` clears it on a `Found` answer, where `apply` cleared
it.

## Deviations from Zed

- **The delegate returns its answer; it does not set it in its task.** Zed's
  delegate owns `selected_index`, so a gap between "rows set" in its task and
  `matches_updated` in the picker's wrapper is harmless. Our picker owns
  `selected`: in that gap a frame could draw the new rows under the old
  selection. Returning the answer puts rows, selection and landing in one
  turn. It also lets `Runs` read the kept selection at landing.
- **No synchronous `matches_updated` after every query.** Zed calls it at
  once for every query (`picker.rs:1243`). We land at once only for
  `Ready`; a `Pending` query keeps the older rows shown until it lands, as
  today.
- **No `PendingUpdateMatches` slot.** Zed keeps the delegate's task outside
  its wrapper so a new query drops it synchronously. Ours needs no slot: the
  heavy work is a background task, and `Runs::start` sets `cancel`
  synchronously, which stops a superseded scan within its cancel latency.
  Today's code has the same shape (a foreground task owning the scan's
  task).
- **No `finalize_update_matches`.** `Ready` covers the palette's case, which
  is why Zed has it. Blocking `enter` on a scan would break the 8 ms
  foreground budget.

## Tests

Tests assert through the Interface. `./scripts/test-names.sh` is diffed
before and after each slice; every removed name is listed here.

Executable success criteria, written first, failing before the change:

- `picker/tests.rs` **`an_answer_at_once_lands_inside_the_refresh`.**
  `Letters` with `answer: Answer::AtOnce`; in one `picker.update`, set the query to `b`,
  `refresh`, `confirm(Open::Here)`, and read `confirmed` inside the same
  closure: `[("beet", Open::Here)]`. Before the change this cannot be
  written without the palette's `defer`, under which `confirmed` is empty
  in the closure. Mutation: making `Ready` spawn fails it.
- `picker/tests.rs` **`a_landing_that_asks_for_a_rerun_runs_it_unless_confirmed`.**
  Two halves over `Letters`. First, `land` with `rerun: true` and nothing
  queued: `applied` gains one query. Second, the same with a queued
  `enter` on a selectable row: the row is confirmed and `applied` gains
  nothing. Mutation: swapping the confirm and the re-run in `land` fails it.
- `picker/tests.rs` **`an_answer_of_none_lands_nothing_and_keeps_enter_queued`.**
  `Letters` with `answer: Answer::Nothing`; type `b`, press `enter`, park:
  `applied` and `confirmed` are empty. Set `answer: Answer::Later`,
  `refresh`, park: `confirmed` is `[("beet", Open::Here)]`. Mutation:
  applying or landing on `None` (clearing `awaiting_matches`, or confirming
  the older rows) fails it. It cannot be written before the change, since
  `Update` does not exist.
- `quick_open/tests/refresh.rs` **`a_superseded_scan_never_lands`**, which
  replaces quick open's `a_stale_result_is_dropped`, the one runs-testable
  keeps. Hold scans, type `p`, type
  `l`, release, and park. `logged` rose by one and the rows answer `pl`. It
  drives keys, not `Runs::apply`. It passes before the change too; it is
  the regression oracle for dropping `is_current`.

Existing tests:

- **Moved.** `an_update_past_the_last_selectable_row_selects_the_one_before`
  (`picker/tests.rs:260`) calls `land(Arrival { row: 1, .. })` instead of
  `matches_updated(1)`.
- **Die.** Quick open's `a_stale_result_is_dropped`: the path it calls no
  longer exists. Vault search's copy and `RunId::previous` are already gone
  with runs-testable's slice 2. The `is_current` assertion in
  `a_start_cancels_the_run_before_and_supersedes_it` (`ledger/tests.rs`)
  goes with `Ledger::is_current`; the test keeps its cancel-flag asserts.
- **Oracles, unchanged.** `enter_while_a_query_is_pending_confirms_its_first_row`,
  `cmd_enter_while_a_query_is_pending_...`, `a_new_query_drops_the_one_before`
  (picker); `enter_during_a_dirty_scan_confirms_without_a_rerun`,
  `notes_changed_during_a_scan_reruns_once`,
  `ten_notifies_during_a_scan_cause_no_cancel_and_one_rerun` (quick open
  refresh); every `quick_open/tests/background.rs` test; vault search's
  `refresh.rs` and `states.rs`; every `shell/tests/command_palette.rs` test,
  which types and presses `enter` in one `simulate_keystrokes`;
  `dropping_the_picker_cancels_its_scan`, which must stay green without a
  hang. `a_new_query_drops_the_one_before` and
  `dropping_the_picker_cancels_its_scan` already drop `Picker::pending`
  mid-flight, the first for a `Letters` task and the second for a scan; with
  `a_superseded_scan_never_lands` they are the executable criterion for the
  drop-cancellation chain.
- `cargo mutants` does not cover `app`; the two mutations named above are
  checked by hand.

## Plan

Each slice merges alone with `./scripts/check.sh` green.

1. **`Arrival` and `Picker::land`** (~100 lines). Data: `Arrival`. Behaviour:
   `land` replaces `matches_updated`, `matches_refreshed` and
   `matches_landed`, and runs the re-run after the queued confirm.
   `Runs::apply` returns its `Arrival` into `land`, with `rerun` from
   `ledger.landed()` through `ledger.refresh`; `Runs::landed` goes.
   The palette's `defer`, `Letters` and `Runs::settle` still call back,
   now as `picker.land(Arrival::first(), cx)`, which stays `pub(crate)` for
   this slice only. Test first:
   `a_landing_that_asks_for_a_rerun_runs_it_unless_confirmed`; move
   `an_update_past_the_last_selectable_row_selects_the_one_before`.
2. **`Update<T>` and `apply_matches`, for the palette and `Letters`**
   (~90 lines). Data: `Update`, `type Matches`, `Answer` on `Letters`.
   Behaviour: the picker applies and lands `Ready` and `Pending`; the
   palette's `defer` and weak handle go. `Runs`' two delegates declare
   `type Matches = std::convert::Infallible`: their `update_matches` returns
   `Update::Pending` of today's task, which still calls `picker.land` and
   then answers `None`, and their `apply_matches` is `match matches {}`. The
   type proves that path never applies, so no dead `apply_matches` body
   exists. This bends the `None` rule for one slice: `Runs` answers `None`
   after landing itself, and Slice 3 removes it. `land` stays `pub(crate)`
   for `Runs`.
   Tests first: `an_answer_at_once_lands_inside_the_refresh`,
   `an_answer_of_none_lands_nothing_and_keeps_enter_queued`.
3. **`Runs` answers instead of landing** (~110 lines). Data: `Ran`,
   `type Matches = Ran<Self::Found>`. Behaviour: `Runs::update_matches`
   spawns one background task that returns `None` on cancel;
   `Runs::apply_matches` returns the `Arrival`; `land` goes private;
   `Runs::settle` goes. This slice carries the one runs-testable hands over
   (see Merge order): `Ledger::is_current`, its assertion and its guards go.
   Tests first: `a_superseded_scan_never_lands`; delete quick open's
   `a_stale_result_is_dropped` in the same commit. This slice holds all the
   cancellation change, so it reverts alone.
4. **Docs** (~40 lines). Amend palette spec D3 (the `App::defer` paragraph),
   the query-run spec's `Runs` section and As built, and search spec lines
   686-718 (`matches_updated`/`matches_refreshed`) to point here. Add this
   spec to CLAUDE.md's Docs list. Fill "As built".

## Performance

No new work per keystroke or per frame. The palette loses one `defer` per
keystroke. A run that scans nothing (scanning, blank or no-term vault search)
loses one spawned task and lands a turn earlier. A scanning run keeps one
foreground task and one background task, as today. First frame is untouched:
the palette's first answer lands inside `Picker::new`, where today it lands
in the same effect flush.

This paragraph describes the state after Slice 3. In Slice 2 the picker wraps
the `Runs` task in its own task, so every `Runs` query spawns two foreground
tasks. Slice 3 removes the inner task (plan item 2).

## Risks

- **Landing inside `Picker::new`.** The palette's `Ready` lands while the
  picker entity is under construction: `cx.notify()` and a scroll request
  on an entity not yet inserted. gpui probably accepts this, since
  `Context` for a reserved entity queues effects; the palette tests open it
  first thing and would fail loudly. Fallback, if not: `new` applies a
  `Ready` answer but skips `cx.notify()`, as nothing has drawn yet.
- **A confirm inside the field's `Edited` handler.** A `Ready` answer can
  run a queued confirm synchronously from the keystroke that edited the
  query. `Dismissed` is an effect, so the modal handles it after the
  handler returns. It is reachable only when `enter` was queued behind a
  `Pending` query and the next key's answer is `Ready` (vault search going
  blank); today that confirm finds no row, and it still finds none.
- **A `Pending` task that answers `None` while it is current** leaves
  `enter` queued until the next answer. The type admits it; the Interface
  rule forbids it. `Runs` answers `None` only after `cancel`, which only a
  newer run (whose `Pending` replaces this one in the same call) or the
  scan's drop sets, so the picker never polls a current `None` from `Runs`.
  `an_answer_of_none_lands_nothing_and_keeps_enter_queued` pins what the
  picker does if one arrives.
- **The scan's `Scope` must drop on a background thread**
  (query-run spec, Risks). It still does: the picker's foreground wrapper
  owns a background `Task`, whose drop the background executor completes.

## Out of scope

- Zed's `finalize_update_matches` (blocking `enter` on a pending answer).
- Moving `selected` into the delegate, as Zed does.
- `Runs`' debounce, D7 deferral, and `IndexWatch` throttle: unchanged.
- Any change to what `enter` confirms when it was queued before a later key.

## Review

Red/blue review, 2026-09-30.

- **F1-runid-previous-dead-code: rejected, legibility fixed.** Red read
  Slice 2 as leaving `RunId::previous` without callers. The plan already
  deleted it with its only two callers (`quick_open/tests.rs:217`,
  `vault_search/tests.rs:168`; checked by grep), and "What moves where"
  listed it. Blue is right: no clippy failure. The deletion is now named in
  the Tests section's "Die" bullet and in Slice 3's test list, since
  `test-names.sh` does not see a dead test-only method.
- **F2-pending-task-cancellation-hack: fixed.** Confirmed: `Ran` had no
  value for a cancelled run, so the draft parked on
  `std::future::pending()` and rested on the wrapper being dropped in
  lockstep. `Update::Pending` now holds a `Task<Option<T>>`; `None` lands
  nothing, and `Runs` returns it as today's code returns early. The rule
  for when `None` may be returned stays in the Interface, but the absent
  answer is a value the wrapper matches, not a future that never ends.
  `Ran` gets no `Cancelled` variant: `apply_matches` would then need a
  "no landing" result, which moves the same absence one call later.
- **F3-invariant-overclaim: fixed.** Confirmed: both fields are private
  today (`picker/mod.rs:63`, `65`). The bullet now names the new guarantee
  only: a delegate cannot call back to land.
- **Improvement, split Slice 2: adopted.** Slice 2 now moves the palette and
  `Letters`; `Runs` joins in Slice 3 through `type Matches = Infallible`,
  which the type proves never applies. Each slice is under ~110 lines, and
  the cancellation change reverts alone.
- **Improvement, name `RunId::previous` in "Die": adopted** (see F1).
- **Improvement, `Task<Option<T>>`: adopted** (see F2).
- **Improvement, reword F3: adopted** (see F3).
- **Improvement, a test that drops `pending` mid-flight: adopted in part.**
  `a_new_query_drops_the_one_before` and
  `dropping_the_picker_cancels_its_scan` already drop `pending` mid-flight
  through the Interface, and `a_superseded_scan_never_lands` covers a scan
  superseded mid-flight; a fourth would assert the same state. The new
  path, a `None` answer, gets its own failing-first test:
  `an_answer_of_none_lands_nothing_and_keeps_enter_queued`.

Outcome: the candidate stands. No finding removes the Seam or the Depth it
gains.

Cross-spec reconciliation (2026-09-30): runs-testable lands first (new Merge
order section); `is_current` deletion is the slice it hands over; the re-run
goes through `Ledger::refresh`, not a hand write of `next`; `RunId::previous`
and vault search's stale test are no longer deleted here; `Landing` renamed
`Arrival`, since restore-tabs adds a `Landing` state to `inkra-app`;
`Runs::scanning` stays in the driver.

## As built

Slices 1 to 4 are one commit each on `arch/picker-runs`; slices 1 to 3 each
have one review-fix commit. Deviations from the design above:

- **`Picker::selected` is `#[cfg(test)]`.** `Runs` reads the selection from
  `apply_matches`' `selected` argument, so no other code reads it.
- **The palette's `Row` is `pub(crate)`.** `type Matches = Vec<Row>` names
  it in a `pub(crate)` trait.
- **Landing inside `Picker::new`** (Risks) needed no fallback: `new` calls
  `refresh`, the palette's `Ready` answer lands there, and every palette test
  stays green.
- **`a_superseded_scan_never_lands`** advances the clock by `SEARCH_DEBOUNCE`
  after the release, not only parks: the `l` run waits the debounce behind a
  user search. `logged` in `quick_open/tests/background.rs` is `pub(super)`
  so `refresh.rs` reads it.
- **Tests.** Added: `a_landing_that_asks_for_a_rerun_runs_it_unless_confirmed`,
  `an_answer_at_once_lands_inside_the_refresh`,
  `an_answer_of_none_lands_nothing_and_keeps_enter_queued` (picker) and
  `a_superseded_scan_never_lands` (quick open refresh). Removed: quick open's
  `a_stale_result_is_dropped`, and the two `is_current` asserts in
  `a_start_cancels_the_run_before_and_supersedes_it`.
