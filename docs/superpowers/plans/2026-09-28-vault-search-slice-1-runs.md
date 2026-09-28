# Vault search, slice 1: `Runs` — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md`, S1.
Search spec D7 (a refresh never cancels a scan in flight) is the rule being moved.

**Goal:** quick open's run logic moves into one `Runs` struct in
`crates/app/src/picker/runs.rs`, owned by the delegate. No behavior change.
Slice 3's vault-search delegate owns a second `Runs` and uses the same API.

**Branch:** `vault-search-1-runs` from `vault-search-spec` (`32fc445`).

## Success criterion

A pure move has no new failing test. The oracle is the existing quick-open
suite, run unchanged in its assertions:

- `crates/app/src/quick_open/tests.rs` and `tests/{background,refresh,excerpts,rows}.rs`
  pass. In particular `a_stale_result_is_dropped`,
  `a_new_query_cancels_the_scan_before_it`,
  `ten_notifies_during_a_scan_cause_no_cancel_and_one_rerun`, and the
  `Scan::Stale` assertions in `background.rs` pin the moved state machine.
- `./scripts/test-names.sh` is identical before and after (no test lost).
- `./scripts/check.sh` exits 0.

The only test edits are accessor paths: a field that moved is read through
`delegate.runs` (`cancel`, `id`, `scan`, the started counter) and `Scan` is
imported from `crate::picker::runs`. No assertion, fixture or step changes.

## Files

| File | Change |
|---|---|
| `crates/app/src/picker/runs.rs` | new: `Scan`, `Run`, `Runs`, `Started` |
| `crates/app/src/picker/mod.rs` | `pub(crate) mod runs;` |
| `crates/app/src/quick_open/mod.rs` | fields replaced by `runs: Runs`; `Drop` impl removed; `update_matches`, `apply`, `confirm`, `new` call `Runs` |
| `crates/app/src/quick_open/scan.rs` | deleted (contents moved) |
| `crates/app/src/quick_open/tests.rs`, `tests/background.rs`, `tests/refresh.rs` | accessor paths only |

## Data shapes

```rust
// picker/runs.rs
pub(crate) enum Scan { Idle, Running { user: bool }, Stale { user: bool } }   // moved as is
pub(crate) enum Run { Search, Refresh, Notify }                               // moved as is

pub(crate) struct Runs {
    cancel: Arc<AtomicBool>,   // was QuickOpenDelegate::cancel
    id: u64,                   // was search_id
    scan: Scan,
    next: Run,                 // was next_run
    watch: IndexWatch,
    #[cfg(test)] started: usize,   // was QuickOpenDelegate::runs (the field name `runs` now holds `Runs`)
}

/// What `start` hands the delegate for the run it begins.
pub(crate) struct Started { pub(crate) id: u64, pub(crate) cancel: Arc<AtomicBool>, pub(crate) run: Run, pub(crate) behind_user: bool }
```

`Runs` implements `Default` (fresh flag, id 0, `Idle`, `next = Search`,
default watch) and `Drop` (sets `cancel`, as `QuickOpenDelegate::drop` did).

The generic parts reach a delegate's `Runs` through `D: PickerDelegate +
AsMut<Runs>`: a std trait, so no new trait with one implementer. Quick open
implements `AsMut<Runs>` in one line; vault search will too.

## API and where each old line goes

| Old (quick open) | New |
|---|---|
| `update_matches`: cancel old flag, new flag, `search_id += 1`, take `next_run`, `behind_user`, `scan = Idle`, `watch.seen` | `runs.start(links.read(cx)) -> Started`, same order |
| `update_matches`: `scan = Running { user }`, test `runs += 1` | `runs.scanning(run)` |
| scanning-state task: `search_id == id` | `runs.is_current(id)` |
| `apply`: `id != search_id` return | `!runs.is_current(id)` |
| `apply` end: take `scan`, re-run on `Stale` | `Runs::landed(picker, cx)` |
| `confirm`: `Stale → Running` | `runs.confirmed()` |
| `scan.rs` `refresh_or_defer` | `Runs::refresh_or_defer(picker, run, cx)` generic over `D` |
| `new`: the `cx.observe(links, …)` closure (watch, throttle) | `Runs::observe(links, cx) -> Subscription` generic over `D` |

Stays in quick open: the `Run` → wait mapping (`NOTES_CHANGED_REFRESH`,
`SEARCH_DEBOUNCE`), the `NotesChanged` subscription and its `Answered::Scanning`
choice of `Run::Search`, `list.cancel()` on `Run::Search`, `Search`, `hold`,
`hold_excerpts`, `logged`, the excerpt machinery.

Behavior preserved by construction: every moved statement keeps its order
relative to the ones around it. In `update_matches`, `start` runs all
statements before the `awaits_first_build` check (as today); `scanning`
runs after it, only for a real scan (as today, so the scanning state's empty
runs are not counted).

Two moves are reordered, both inert (blue team checked): `list.cancel()`
now runs after `start` instead of between the `next` take and the
`behind_user` read, and `Runs`'s drop sets its flag in field order instead of
before every field drops. `List`'s flag and `Runs`'s flag never interact
(`quick_open/list.rs`).

The generic `refresh_or_defer` and `landed` never hold
`picker.delegate.as_mut()` across `picker.refresh*`: `Scan` is `Copy`, so each
statement borrows anew, as `scan.rs` does.

Test accessors on `Runs`, `#[cfg(test)] pub(crate)`: `cancel()`, `id()`,
`scan()`, `started()`. Fields stay private.

## Plan review

Red and blue teams (Sonnet) ran on this plan. Confirmed and folded in:
`Started`'s fields need `pub(crate)`; the borrow note above; the two inert
reorders above; the `Scan` import path in `tests/background.rs`. Confirmed
already covered: test accessors gated by `#[cfg(test)]`. `AsMut<Runs>` and a
generic `cx.observe` were checked against gpui's `Context::observe`: no
obstruction.

## Steps

1. Record `./scripts/test-names.sh > /tmp/runs-names-before.txt`.
2. Write `picker/runs.rs`; register it in `picker/mod.rs`.
3. Rewire `quick_open/mod.rs`; delete `quick_open/scan.rs`.
4. Update test accessor paths.
5. `cargo fmt -p inkra-app`; `cargo test -p inkra-app quick_open`.
6. `./scripts/test-names.sh > /tmp/runs-names-after.txt`; `diff` both: empty.
7. `./scripts/check.sh`: exit 0.
8. No shot: nothing renders differently.

## Deviation from Zed

Zed's `picker` has no run state; `file_finder` keeps its own
(`search_count`, `latest_search_id`, `cancel_flag`). Inkra shares it because
two delegates need D7's rule, and the spec (S1) takes that seam.

`Runs::is_current` is strict equality. Zed's `file_finder` applies any result
with `search_id >= latest_search_id` (`file_finder.rs:1105`), so an older search
that lands after a newer one is dropped, but a newer one always applies. Inkra
starts a run only from `update_matches`, which cancels the run before, so no
result newer than the current id can exist: equality says the same thing and
also drops a cancelled run's late result.
Zed's `>=` also accepts repeated batches for one id; an Inkra run applies
once. A run that ever reports in batches must revisit this check.

## Follow-up (review)

The quality review and the reviewer asked for a shape that makes wrong uses
impossible before slice 3 adds a second caller. Same behavior throughout:

- `Scan` is flat: `scan: Option<Scan { user, rerun }>`. `confirmed` clears
  `rerun` instead of turning a stale scan back into a running one.
- `scanning` takes the `&Started`, so the scan's run is the one started.
- `RunId` is a newtype through `Search`, `List::replace`/`land` and `is_current`.
- `Run::is_user` is the one predicate for "the user waits on it".
- `Runs` and `Run` derive `Default`; `start` takes the flag, the next run and
  the scan with `std::mem::take`.
- The test accessor `cancel` is `cancel_flag`; the `IndexSync` parameter is
  `sync` everywhere in `runs.rs`.

## Diff review

Red and blue teams (Sonnet) ran on the diff. No blocker or major finding. One
nit refuted: `start` names its `&IndexSync` `sync`, as `IndexWatch` does.

Red and blue (Sonnet) on the follow-up diff: the flat `Scan` maps one to one
onto the old enum (`None`, `rerun: false`, `rerun: true`) in every
transition; no divergence. One style nit refuted as not warranted:
`started.cancel` moves into the task by field capture, since `scanning` needs
the whole `Started` first.
