# One module owns running a query

Status: **built 2026-09-29** (As built, below). Refactor: quick open, vault search and the
backlinks panel behave as before, at the same latency.

## Why

Running one query over the index takes five steps across two crates, and
each caller repeats them. Checked against `arch/deepening`:

- **`inkra-index` exposes the steps, not the query.** `find_notes` and
  `search` take six arguments: `snapshot, lookup, range, query, cancel,
  limit` (`index/src/find/run.rs:14-40`, `search/run.rs:14-24`). The caller
  must pair them by hand: the `Lookup` from the same snapshot and query, the
  ranges from `IndexSnapshot::chunks` of that snapshot, the merge over every
  range in order (`merge_notes`, `Searched::merge`). `Lookup`, `Ranked`,
  `IndexSnapshot::lookup` and `IndexSnapshot::chunks` are public only for this
  (`index/src/index.rs:258-277`, `lib.rs:21,31`).
- **`app/src/index/fan_out.rs` does the pairing.** It spawns the lookup,
  one scan per `chunks(num_cpus)` range, the merge, then drops the result if
  `cancel` was set. It is generic over two `fn` pointers so both pickers
  share it; the index cannot test it, since it runs on gpui.
- **Each picker delegate repeats the run protocol.** `quick_open/mod.rs:203-257`
  and `vault_search/mod.rs:180-223` both do: `runs.start`, the
  `awaits_first_build` check and `Runs::settle`, the `Run` → wait choice,
  `runs.scanning`, the spawn and `fan_out`. Both `apply`s do: `is_current`,
  the kept key read before the rows change (non-user runs only),
  `matches_refreshed` vs `matches_updated(0)`, then `Runs::landed` last. Both
  `confirm`s call `runs.confirmed()` first. `Runs` reaches the delegate
  through `AsMut<Runs>` (`picker/runs.rs:137-206`).
- **`IndexWatch`'s throttle is written twice.** It decides `Skip`/`Now`/`Later`,
  and each owner spawns the `BUILDING_REFRESH` timer and hands the task back:
  `backlinks/mod.rs:80-90` and `picker/runs.rs:180-199`.

A new picker over the index would copy all of it; a step forgotten
(`landed` before the rows, `confirmed` skipped) compiles and misbehaves.

## What Zed does

`fuzzy::match_path_sets` (`vendor/zed-reference/crates/fuzzy/src/paths.rs:142-250`)
takes the candidates, the query, a `cancel_flag`, a limit and the
`BackgroundExecutor`. Inside, it splits the candidates into `num_cpus`
segments, runs them with `executor.scoped`, one result slot per segment, and
merges. The caller never sees a segment. We follow it: the index owns the
split, the scans and the merge.

Zed's `file_finder` keeps a `latest_search_id`
and checks it in its own delegate. There the protocol is per delegate. Ours
has D7 on top (a refresh never cancels a scan, one re-run follows), which is
too much to repeat; `Runs` owns it (Deviations).

## The deepened modules

### `inkra-index`: one entry per query, in `crates/index/src/query.rs`

```rust
impl IndexSnapshot {
    /// Quick open's rows: the best `limit` notes, best first. `None` once `cancel` is set.
    pub async fn find_notes(&self, query: &NoteQuery, limit: usize,
                            jobs: &dyn Jobs, cancel: &AtomicBool) -> Option<Vec<NoteMatch>>;

    /// Vault search's hits: the first `limit` word starts, grouped by note in vault order.
    pub async fn search(&self, query: &NoteQuery, limit: usize,
                        jobs: &dyn Jobs, cancel: &AtomicBool) -> Option<Searched>;
}
```

`Jobs` is `inkra_vault::Jobs`, the walk's trait (`vault/src/vault/walk.rs:37-61`):
`parallelism()` and `run(Vec<Job<'a>>) -> Job<'a>`. `inkra-index` already
depends on `inkra-vault`. Its doc changes from "a walk's jobs" to "jobs": a
walk's or a query's.

What it hides, in one private function `fan_out` in `query.rs`:

1. `lookup` of `query` in this snapshot's words; `None` → `None`.
2. `self.chunks(jobs.parallelism())`: disjoint, contiguous, in vault order.
3. One `Job` per range, each writing the per-range scan into its own slot
   (Zed's `segment_results`), borrowing the snapshot, the lookup and the query:
   no `Arc` clone per range.
4. `jobs.run(..).await`, then the merge over the slots in range order.
5. `None` when `cancel` is set after the merge, as `fan_out.rs:52-56` today:
   a cancelled range returns nothing, so the merge of the others is not the
   answer.

Invariants, each held by the function body, not by the caller:

- The lookup, the ranges and the merge read one snapshot and one query.
- Any `parallelism` gives the answer of one range over all notes.
- `Some` is the whole answer; a cancelled run is `None`, never a partial list.

Becomes crate-private: `find_notes`/`merge_notes` of one range (renamed
`find_in`/`merge_found` inside `find/run.rs`), `search` of one range
(`search_in`), `Searched::merge`, `Ranked`, `IndexSnapshot::lookup` and
`IndexSnapshot::chunks`. `WordsSnapshot::lookup`, `Lookup` and `TermHit` stay
public: they are the word index's own interface, read by the `words` bench and
by the app's word tests (`index/words_tests.rs`, `index/tests.rs:655-720`),
never a query's input any more.

Why `inkra-index`: the split and the merge are one invariant ("any split
equals one range"), and that invariant is the index's. There it is under
`proptest` and `cargo mutants`; in `app` it is not.

### `inkra-app`: `Runs` drives a picker over the index

Superseded in part by `2026-09-30-runs-testable-design.md`: the run state
and its rules (`start`, `begin_scan`, `refresh`, `landed`) live in `Ledger`,
`picker/runs/ledger.rs`; `Scan` is private to it, and `Runs::begun` replaces
the test counters and `cancel_flag`.

Superseded in part by `2026-09-30-picker-owns-landing-design.md`: the
landing moves into `Picker`. `Runs::update_matches` returns
`Update<Ran<D::Found>>`: `Ready(Ran::Settled)` where it settled, else one
task on the background executor (`cx.background_executor().spawn`) that
answers `None` on cancel. `Runs::apply_matches`
replaces `apply` and returns an `Arrival`, whose `rerun` comes from
`Ledger::landed` through `Ledger::refresh`. `settle`, `landed` and
`is_current` are gone; `Runs` no longer reads `Picker::confirmed`.

`picker/runs.rs` gains one trait, with two implementers (quick open and vault
search):

```rust
/// A picker delegate whose rows answer a query over the index; `Runs` drives its runs.
pub(crate) trait IndexDelegate: PickerDelegate {
    /// What one run found, applied on the foreground.
    type Found: Send + 'static;
    /// A row's identity across refreshes: what a refresh keeps selected.
    type Key: PartialEq;

    fn runs(&mut self) -> &mut Runs;
    fn sync(&self) -> &Entity<IndexSync>;
    /// Before the first build: empty rows that say so.
    fn scanning(&mut self);
    /// The query `text` runs; `None` once the delegate set rows that need no scan.
    fn query(&mut self, text: &str, run: Run) -> Option<NoteQuery>;
    /// Off the foreground: `find_notes` or `search`, with the delegate's limit.
    fn search<'a>(snapshot: &'a IndexSnapshot, query: &'a NoteQuery, jobs: &'a dyn Jobs,
                  cancel: &'a AtomicBool) -> impl Future<Output = Option<Self::Found>> + Send + 'a;
    /// Replaces the rows with what `landed` found.
    fn show(&mut self, landed: Landed<Self::Found>, cx: &mut Context<Picker<Self>>);
    fn key(&self, ix: usize) -> Option<Self::Key>;
}

pub(crate) struct Landed<F> {
    pub(crate) run: Run,
    pub(crate) begun: Instant,
    pub(crate) snapshot: IndexSnapshot,
    pub(crate) query: Arc<NoteQuery>,
    pub(crate) found: F,
}

impl Runs {
    /// The whole of an `IndexDelegate`'s `update_matches`.
    pub(crate) fn update_matches<D: IndexDelegate>(delegate: &mut D, text: &str,
                                                   cx: &mut Context<Picker<D>>) -> Task<()>;
    /// Lands run `id`'s result; `pub(crate)` for the stale-result tests.
    pub(crate) fn apply<D: IndexDelegate>(picker: &mut Picker<D>, id: RunId,
                                          landed: Landed<D::Found>, cx: &mut Context<Picker<D>>);
    pub(crate) fn refresh_or_defer<D: IndexDelegate>(..);  // quick open's `NotesChanged`
    pub(crate) fn observe<D: IndexDelegate>(..) -> Subscription;  // through `IndexWatch::observe`
}
```

`update_matches`, in this order (today's order, now in one place):

1. `runs.start(sync)`: cancels the run before, bumps the `RunId`, takes the
   `Run` a refresh set, reads `behind_user`, marks the revision seen.
2. `sync.awaits_first_build()` → `delegate.scanning()`, settle.
3. `delegate.query(text, run)`; `None` → settle. Quick open cancels its
   excerpts here on a user run; vault search answers blank or no term here.
4. The wait, from the `Run`: `Refresh` → `NOTES_CHANGED_REFRESH`, `Search` →
   `SEARCH_DEBOUNCE` when `behind_user`, `Notify` → none. Vault search never
   sets `Refresh` (it does not subscribe to `NotesChanged`), so one table
   serves both; `NOTES_CHANGED_REFRESH` moves to `picker/runs.rs`.
5. `runs.begin_scan(&started)`, the snapshot, then the spawned task: the wait,
   the test hold, `D::search` in a `background_spawn`ed task with the app's
   `Background` jobs (Risks: never in the foreground task), `None` → return,
   else `apply`.

`apply`: `is_current(id)` or return; the kept key, read from the selection
before the rows change, on a non-user run only; `delegate.show(landed)`; the
kept key's row by `key(ix)` over `0..match_count()`, `matches_refreshed(ix)`,
else `matches_updated(0)`; then `landed`, last, so an `enter` waiting on the
rows confirms first.

**`confirmed` is the picker's state, not a call.** `Picker` gains `confirmed:
bool`, set in `confirm_selected`. `Runs::landed` skips the deferred re-run
when the picker confirmed. `Runs::confirmed()` and the two calls to it go.
Today it clears the mark of the scan in flight only; the one difference is a
refresh arriving after a confirm, while the dismissed picker is still alive,
which no longer re-runs. Nothing is shown then.

`AsMut<Runs>` goes; `IndexDelegate::runs` replaces it. `Run`, `RunId`,
`Scan`, the test hold and the counters stay as they are.

**The app's `Jobs`.** `Background` moves from `vault/scans.rs` to
`app/src/background.rs` and gains its `Priority`: the walk keeps `Low`, a
query runs at `Priority::default()`, as `executor.spawn` in `fan_out.rs` does
today. `parallelism()` is `num_cpus()`, as today.

### `IndexWatch` owns its throttle

```rust
impl IndexWatch {
    /// `refresh` on `sync`'s notifies: at once when built, at most every
    /// `BUILDING_REFRESH` while loads land, never twice for one revision.
    pub(crate) fn observe<V: 'static>(sync: &Entity<IndexSync>, cx: &mut Context<V>,
                                      watch: fn(&mut V) -> &mut IndexWatch,
                                      refresh: fn(&mut V, &mut Context<V>)) -> Subscription;
    pub(crate) fn seen(&mut self, sync: &IndexSync);
}
```

`Refresh`, `on_notify` and `throttle` become private to `index/watch.rs`.
The backlinks panel passes `Self::rebuild`; `Runs::observe` passes
`refresh_or_defer(.., Run::Notify, ..)`.

## What moves where

| From | To |
|---|---|
| `app/src/index/fan_out.rs` | `index/src/query.rs` (`fan_out`, two entries); file deleted |
| `find_notes`, `merge_notes`, `search`, `Searched::merge`, `Ranked`, `IndexSnapshot::{lookup, chunks}` | crate-private |
| `quick_open/mod.rs` `update_matches`, `apply`, `Search` | `Runs::update_matches`/`apply`; the delegate keeps `query`, `show`, `key`, `scanning` |
| `vault_search/mod.rs` `update_matches`, `apply`, `answer` | same |
| `Runs::{settle, confirmed}`, `AsMut<Runs>` | private to the driver; `Picker::confirmed` |
| `NOTES_CHANGED_REFRESH` (`quick_open/mod.rs`) | `picker/runs.rs` |
| `Background` (`vault/scans.rs`) | `app/src/background.rs`, with a `Priority` |
| throttle in `backlinks/mod.rs`, `Runs::observe` | `IndexWatch::observe` |

`picker/runs.rs` is 226 lines; with the driver and without the throttle it
should stay under 400. Past that, the driver goes to `picker/runs/drive.rs`.

## Tests

The new interfaces are the test surface. Each behaviour has an executable
criterion; `./scripts/test-names.sh` is diffed before and after each step,
and every removed name is accounted for below.

`inkra-index`, `query/tests.rs`:

- **Any split equals one range.** `proptest` over the `find/model_cases.rs`
  vaults and `parallelism` in `1..=8`: `find_notes` through a test `Jobs`
  (`Ranges(n)`: `n` parallelism, jobs run inline) equals the crate-private
  one-range `find_in` + `merge_found`. Same for `search` against `search_in`.
  Replaces `merging_the_snapshot_s_chunks_equals_one_range`
  (`find/tests.rs:326`) and `merge_concatenates_in_range_order_and_truncates`
  (`search/tests.rs:240`), which move here.
- **A cancel set before the run is `None`**, for both entries.
- **A cancel set during the run is `None`, not a partial list.** A test
  `Jobs` whose `run` sets the flag before running the jobs. Mutating away the
  check after the merge fails it.
- **An empty vault is `Some(empty)`**, not `None`.
- Existing helpers that scan the whole vault (`find/tests.rs` `run`,
  `find/model_tests.rs`, `find/excerpt_tests.rs`, `index/tests.rs:298`,
  `search/tests.rs` `searched` on a full range) call the entry with
  `inkra_vault::Serial` under `futures::executor::block_on`. Tests of one
  range (`a_range_searches_only_its_notes`, `live/diff_tests.rs:153`, the
  merge-cap tests) keep the crate-private functions.
- `cargo mutants` on `index`: zero missed.

`inkra-app`:

- `any_core_count_lists_the_same_rows` (quick open and vault search, with
  `set_num_cpus`) is unchanged: it covers `Background` as `Jobs`.
- The two `a_stale_result_is_dropped` call `Runs::apply` with the previous
  `RunId` and a `Landed`; each stays with its delegate, since `show` differs.
- The D7 tests (`quick_open/tests/refresh.rs`, `background.rs`,
  `vault_search/tests/refresh.rs`) and the throttle tests (`backlinks/tests.rs:160`,
  `vault_search/tests/states.rs:124`) are unchanged: they are the oracle.
  `enter_during_a_dirty_scan_confirms_without_a_rerun` is the oracle for
  `Picker::confirmed`.
- `index/watch/tests.rs` keeps testing the decision (private, same module).
- App test helpers that ran the six-argument form (`test_support.rs:127`,
  `index/corpus_tests.rs:14`, `index/tests.rs:127`, `quick_open/render/tests.rs:20`)
  call the entry with `Serial`. `test_support::quick_open` loses its `range`
  parameter: every caller passes the whole vault.

## Performance

The work per query does not change: one lookup, one scan per core, one merge.
Two costs move. The scans are `scoped` jobs instead of `'static` tasks, so the
four `Arc` clones per range go. The lookup and the merge run in the task that
awaits the scans instead of two tasks of their own: one hop fewer.

Criterion: `cargo bench -p inkra-index --bench find -- --quick` and
`--bench search -- --quick` run on `arch/deepening` before step 1 and after
step 2, on the same machine state, interleaved if the load drifts. No case
may regress past noise; the figures go in "As built".

Both bench files call what becomes crate-private (`lookup`, `chunks`,
`find_notes` and `merge_notes` of one range, `search` of one range), so
step 2 rewrites them onto the entries:

- The one-core cases (`search.rs`'s `keystroke`, `find.rs`'s `one core`
  group and its adversarial rows) call `find_notes`/`search` under
  `futures::executor::block_on` with `inkra_vault::Serial`, as the test
  helpers do.
- `find.rs`'s thread cases (`THREADS threads, per_thread× ranges`,
  `keystroke to names`, `keystroke to excerpts`) run through a bench `Jobs`:
  `parallelism()` is `per_thread × THREADS`, and `run` drains the jobs on
  `THREADS` scoped threads, as `split` does today.

`block_on` adds a fixed cost per call that today's direct call does not
pay: one waker, one boxed future per job. Before trusting the comparison,
bench an empty-vault query both ways; that cost must stay under noise of
the cheapest case. If it does not, the baseline is re-run through the same
`block_on` wrapper around the old functions, so both sides pay it.

## Deviations from Zed

- **`Jobs`, not the executor.** Zed's `fuzzy` takes a `BackgroundExecutor`.
  `inkra-index` has no gpui (`check.sh`), so it takes the vault's `Jobs`,
  which has two real implementations and two test fakes.
- **The run protocol lives in `Runs`, not in each delegate.** Zed checks
  `latest_search_id` per delegate. Ours adds D7's deferred re-run, the kept
  selection and the `BUILDING_REFRESH` throttle; two delegates repeating that
  is the defect this slice removes.
- **`Picker::confirmed`.** Zed's picker has no such state; its delegates do
  not defer re-runs.

## Risks

- **A dropped scoped run blocks the thread that drops it.** `Scope`'s `Drop`
  blocks until every spawned job ends (`gpui/src/executor.rs:543-557`). If the
  picker's foreground task awaited the scoped future itself, replacing
  `Picker::pending` (`picker/mod.rs:120`) would drop it in place, on the
  foreground: the foreground executor is `!Send` (`executor.rs:29`), so no
  other thread can drop it. Hence the query never runs in the foreground
  task: `Runs` spawns it with `background_spawn`, and the foreground task
  awaits that `Task`. Dropping a `Task` of an idle future schedules it once
  more so that its executor drops the future
  (`async-task-4.7.1/src/task.rs:183-226`, wrapped by
  `scheduler/src/executor.rs:440-447`): the `Scope` drops on a background
  thread. The wait there is bounded by cancel latency, since `Runs::start`
  set `cancel` first and a scan reads it every 1,024 notes and 64 KB of text.
  `dropping_the_picker_cancels_its_scan` must stay green without a hang.
- **A missed pairing moves inside the index.** If `fan_out` read two
  snapshots, the proptest "any split equals one range" and the cancel tests
  catch it; before, only the app's `any_core_count` test did.
- **The `Refresh` wait table is shared.** A future delegate that sets
  `Run::Refresh` waits `NOTES_CHANGED_REFRESH`. That is the meaning of
  `Refresh`; the doc comment on `Run::Refresh` says so.
- **`Picker::confirmed`** differs from `Runs::confirmed` only for a refresh
  after a confirm (above).

## Plan

Each step merges alone with `./scripts/check.sh` green and
`./scripts/test-names.sh` diffed.

1. **Baseline.** Run the `find` and `search` benches on `arch/deepening`;
   keep the output for "As built".
2. **The index entry, and its callers.** `query.rs` with `fan_out`,
   `find_notes`, `search`; the index tests and benches onto the entries; the
   raw functions crate-private. In the same step the two delegates call
   `snapshot.find_notes`/`search` on the background executor with
   `Background { executor, Priority::default() }` (moved to
   `app/src/background.rs`), inside a `background_spawn`ed task (Risks),
   `app/src/index/fan_out.rs` is deleted, and the app test helpers switch.
   The benches move onto the entries as Performance says: `block_on` with
   `Serial` for one core, a `THREADS`-thread bench `Jobs` for the thread
   cases. The `Runs` protocol does not change yet. Bench after; `cargo
   mutants` on `index`.
3. **`IndexWatch::observe`.** The throttle moves in; the backlinks panel and
   `Runs::observe` call it; `Refresh` goes private. The backlinks and picker
   throttle tests are the oracle.
4. **`IndexDelegate`.** The trait, `Landed`, `Runs::update_matches`/`apply`,
   `Picker::confirmed`, `NOTES_CHANGED_REFRESH` moved. Quick open then vault
   search implement it; their `update_matches` becomes one call, their
   `apply` and `answer` go, `AsMut<Runs>` goes. The stale tests switch to
   `Runs::apply`. Render the `quick-open-*` and `vault-search-*` shots and
   look at them.
5. **Docs.** Amend the vault-search spec (S1 "As built", S3 "Shared with
   quick open") and the smart-quick-open spec (S5 "Built in slice 8") to
   point here; add this spec to CLAUDE.md's Docs list; fill "As built" with
   the bench figures.

## As built

Steps 2 to 5 are one commit each on `arch/query-run`. Deviations from the
design above:

- **Later, 2026-09-30:** `Runs::apply`, `settle`, `landed` and `is_current`
  are gone, and the query runs in one background task with no foreground
  task inside `Runs`. See `2026-09-30-picker-owns-landing-design.md`.

- **`Landed` carries the `RunId`.** Quick open's `List` keys its excerpts by
  run, so `show` needs it. `Runs::apply(picker, landed, cx)` reads it there
  instead of taking it apart.
- **`IndexDelegate::search`** is `fn search(snapshot, query, jobs, cancel) ->
  impl Future<Output = Option<Self::Found>> + Send`: edition 2024 captures the
  lifetimes, so no `'a`. Both delegates implement it as an `async fn`.
- **Quick open's `scanning`** calls a new `List::clear` (cancel the excerpts,
  drop the rows) instead of replacing the list with an empty one of the new run.
  Nothing is drawn from an empty list, so no behaviour changes.
- **Private in `Runs`:** `start`, `Started`, `begin_scan`, `is_current`,
  `settle` and `landed`. `Results::position` goes: `Runs::apply` finds the kept
  row through `key(ix)`.
- **`BUILDING_REFRESH`** is re-exported from `index` for tests only; no view
  reads it any more.
- **The query runs in its own background task** (Risks), in `Runs::update_matches`.
  The trace is `async-task`'s `set_canceled`, not a test: gpui's test
  dispatcher runs every task on one thread, so no test can tell which thread
  drops the `Scope`.
- **Tests.** `query/tests.rs` holds the proptest
  `any_parallelism_answers_as_one_range` (model cases, parallelism `1..=8`,
  limit `0..10`, both entries), the two moved tests under their old names, and
  `a_cancel_set_before_the_run_answers_nothing`,
  `a_cancel_set_during_the_run_answers_nothing_not_a_part` (a `Jobs` that sets
  the flag after the first range), `an_empty_vault_answers_empty_not_nothing`.
  Whole-vault index helpers go through `test_support::find_all` (`Serial`,
  `block_on`). `cargo mutants --in-diff` on `inkra-index`: 8 mutants, 6
  caught, 2 unviable, 0 missed. No test name vanished.

### Benches

`--quick`, `arch/deepening` (a scratch worktree, its own target) against this
branch, same machine, other worktrees building meanwhile.

- **`block_on` cost.** `find_notes` on an empty vault through `block_on` and
  `Serial`: 186 ns, lookup and split included. The cheapest real cases
  (`search` of a word no note holds, 0.2 µs) pay it: +40 to 60 ns with the
  baseline re-run through `block_on` too. The app never ran a query for less:
  it spawned three tasks per query before, one now.
- **`find`, thread and pipeline cases** (`10 threads`, `keystroke to names`,
  `keystroke to excerpts`): 0.81× to 1.14×, both ways, no pattern by corpus.
  `find_notes, 40 words, one core/empty` read 209 → 405 µs in the first pass;
  interleaved re-runs gave 231 to 340 µs on the baseline and 208 to 337 µs
  here: bimodal noise, not the change.

  | case | before | after |
  |---|---|---|
  | keystroke to names, zipf, word 1,000 | 441.5 µs | 371.2 µs |
  | keystroke to names, zipf, 1 letter "e" | 2,390 µs | 2,455 µs |
  | keystroke to names, 40 words, two words | 1,145.8 µs | 1,135.3 µs |
  | keystroke to names, 40 words, empty, 20 recent | 108.8 µs | 115.7 µs |

- **`search`, one core.** Within noise except the 1-char cases with 1,000
  hits (`"e"`, `"é"`, `"e e"`): three interleaved runs gave 537 to 558 µs on
  the baseline and 556 to 594 µs here, +3 to 5 %. Adding `Searched::merge` to
  the baseline's call does not close it; `#[inline(never)]` on `search_in`
  closes about half, so it is probably the code generated for `search_in`
  inlined into `fan_out`. Not fixed: 20 µs on one core, and the app runs one
  range per core. Flagged for a profile before any change.
