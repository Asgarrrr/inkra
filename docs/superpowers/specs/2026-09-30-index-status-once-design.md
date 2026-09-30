# The index status, answered once

Status: **proposed 2026-09-30.** Refactor: quick open, vault search and the
backlinks panel say the same words at the same moments as before.

## Problem

The index is in one of three states: no build has applied (scanning), the
last build's notes are still loading (indexing N of M), or every note is
loaded (ready). No type names these states. `IndexSync` exposes four
accessors, and every reader combines two of them again.

- `IndexSync` (`crates/app/src/index.rs:102-128`) exposes `progress`,
  `is_building`, `awaits_first_build` and `indexing_footer`. The
  `self.vault.is_some() &&` guard is repeated in two of them (`:109`, `:127`).
  `indexing_footer` is itself `is_building() && !awaits_first_build()`, the
  "indexing" state spelled as two booleans (`:114`).
- `LiveIndex` (`crates/index/src/live.rs:57-83`) is the source. Its
  `is_building` is `awaits_first_build() || loaded < total`. The fact
  "`Indexing` implies a first build and `loaded < total`" lives in that
  expression, not in a type.
- Vault search re-derives the indexing state in its empty text: it calls
  `is_building()`, then `progress()`, then formats both counts with
  `thousands` (`crates/app/src/vault_search/mod.rs:163-182`). `index.rs:113-123`
  already does the same reads and the same formatting for the footer.
- Each picker keeps a private copy of "the index was scanning when my last run
  started": `Answered::Scanning` in quick open (`quick_open/mod.rs:50-61`,
  set at `:116-119`, read at `:71` and `:189`) and in vault search
  (`vault_search/mod.rs:52-63`, set at `:111-113`, read at `:167`). Both get
  it through the same hook, `IndexDelegate::scanning`
  (`picker/runs.rs:29-30`), which `Runs::update_matches` calls
  (`runs.rs:187-190`). The hook also clears rows, but rows can only come from
  `show`, and no run shows before the first build: the clear is dead.
- The backlinks panel keeps `building: bool` (`backlinks/mod.rs:25`, `:86`,
  `:115-119`) and `IndexWatch` keys its throttle on `is_building`
  (`index/watch.rs:54`, `:68`).

The friction is real but modest. No reader has a bug today. The cost is that
every reader must know which two booleans make which state, and that the
guard for "no vault" is duplicated.

## What Zed does

Zed's worktree holds one scanning flag, `is_scanning`, a `watch` channel
(`vendor/zed-reference/crates/worktree/src/worktree.rs:144`, `:1614`
`scan_complete`). Project search copies it into its own run state,
`SearchActivity::WaitingForScan`
(`vendor/zed-reference/crates/search/src/project_search.rs:317-320`), and its
empty view matches on that copy (`:1048-1061`, "Loading project…"). So Zed has
one source for the index state and one run-level copy per search view. We
follow that shape: one `Status` at the source, and the run-level copy in
`Runs`, the one module that drives every picker's runs.

## Decision

### `inkra-index`: `live::Status`

```rust
/// What the index can answer now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No build has applied: there are no names to list.
    Scanning,
    /// The last build's notes less those queued or loading; `loaded < total`.
    Indexing { loaded: usize, total: usize },
    /// Every note of the last build is loaded; a rebuild in flight does not count.
    Ready,
}

impl Status {
    #[must_use]
    pub fn is_ready(self) -> bool;
}

impl LiveIndex {
    #[must_use]
    pub fn status(&self) -> Status;
}
```

Interface, for a caller:

- `Scanning` comes first and never comes back: `last` is only ever replaced
  (`live/builds.rs:119`), never taken.
- `Indexing` is only built by `status()`, with `loaded < total`. A reader
  never checks the counts for sense.
- `Ready` with zero notes is an empty vault after its first build.
- Cost: one `progress()` call, a walk of the in-flight loads (bounded by the
  load concurrency). Vault search's empty text does one walk instead of two.

`progress` stays `pub(crate)`: `status` is built on it, and the crate's
white-box tests (`live/tests.rs`, `live/model_tests.rs`) keep reading it.
`is_building` and `awaits_first_build` become private; `startup.rs:52,71`
keeps using `is_building`.

### `inkra-app`: `IndexSync::status`

```rust
impl IndexSync {
    /// `Ready` without a vault: nothing will ever load.
    pub(crate) fn status(&self) -> Status;
}

/// "Scanning the vault…"
pub(crate) const SCANNING: &str;

/// "Indexing 1,204 of 50,000 notes" while `Indexing`, else `None`.
pub(crate) fn indexing_footer(status: Status) -> Option<SharedString>;
```

`status` replaces `progress`, `is_building`, `awaits_first_build` and the
method `indexing_footer`. The no-vault guard is written once, as a `match` on
`self.vault`. That arm is temporary: `2026-09-30-no-vault-mode-design.md`
slice 2 makes `IndexSync.vault` an `Entity<Vault>`, and then `status` is
`self.live.status()` (see Merge order). `indexing_footer` becomes a free function of a `Status`, so the
counts it prints are the ones the caller matched on.

Readers after the change:

| Reader | Before | After |
|---|---|---|
| `Runs::update_matches` (`runs.rs:187`) | `awaits_first_build()` | `status() == Status::Scanning` |
| `IndexWatch::{on_notify, seen}` (`watch.rs:54,68`) | `is_building()` | `!status().is_ready()` |
| `BacklinksPanel::rebuild` (`backlinks/mod.rs:86`) | `is_building()` | `!status().is_ready()` |
| quick open footer (`quick_open/mod.rs:100`) | `indexing_footer()` | `indexing_footer(status())` |
| vault search footer (`vault_search/mod.rs:95`) | `indexing_footer()` | `indexing_footer(status())` |
| vault search empty text (`vault_search/mod.rs:169-179`) | `is_building()` + `progress()` | `match status() { Status::Indexing { loaded, total } => … }` |

The backlinks panel keeps a `bool`, not a `Status`. A `Status` changes with
every load's count, and comparing it in `rebuild` would redraw the editor on
every load (`backlinks/mod.rs:114`). The message it drives only depends on
ready or not.

### `Runs` holds the scanning answer

`Runs` gets one field and one query; `IndexDelegate` loses `scanning`.

```rust
impl Runs {
    /// The last settled run found no build, and no run has landed since.
    pub(crate) fn scanning(&self) -> bool;
}
```

- `update_matches` sets it when it settles a run on `Status::Scanning`.
- `apply` clears it when the current run lands. `update_matches` clears it
  when `query` returns `None`. These are exactly the points where the
  delegates replace `Answered::Scanning` today (`show`, `answer`).
- `scanning` is a field of the driver `Runs` (`picker/runs.rs`), never of
  runs-testable's gpui-free `Ledger`: it answers from `IndexSync::status`,
  which the `Ledger` does not read. The three writes above are the driver's
  in either order of the two specs; runs-testable states the same.
- The answer is sticky until the next rows land, as today. Reading the live
  `status()` in the empty text instead would show a blank between the first
  build and the first rows: up to `BUILDING_REFRESH` (100 ms) in vault search.

Each delegate's `empty_text` starts with
`if self.runs.scanning() { return Some(SCANNING.into()); }` and its
`Answered` keeps only query-specific arms: quick open `Nothing`, `Recent`,
`Query`; vault search `Blank`, `NoTerm`, `Query`. Quick open's `NotesChanged`
handler reads `self.runs.scanning()` instead of `answered == Scanning`
(`quick_open/mod.rs:71`).

## What moves where

| From | To |
|---|---|
| `LiveIndex::{is_building, awaits_first_build}` pub | private; `Status` + `status()` pub |
| `IndexSync::{progress, is_building, awaits_first_build}` | `IndexSync::status` |
| `IndexSync::indexing_footer` (method) | `crate::index::indexing_footer(Status)` |
| `Answered::Scanning` in quick open and vault search | `Runs::scanning` |
| `IndexDelegate::scanning` and its two impls | deleted |
| `List::clear` call in `QuickOpenDelegate::scanning` | deleted (dead: no rows before the first build); `List::clear` (`quick_open/list.rs:67-70`) has no other caller and goes |
| `results` reset in `VaultSearchDelegate::scanning` (`vault_search/mod.rs:111-113` via `answer`, `:83-86`) | deleted, dead for the same reason: `Runs::update_matches` calls the hook only while `Scanning`, and `results` is only filled by `show` off a landed run, which cannot exist before the first build. `answer` keeps its reset for its other callers |

## Invariants now held by types

- "Indexing" means a first build applied and `loaded < total`: one arm of
  `Status`, built in one place. Today it is `is_building() &&
  !awaits_first_build()` at each reader.
- The counts a view prints are the counts of the state it matched: they come
  out of the `Indexing` arm, not from a second call.
- "No vault" is `Ready`, decided in `IndexSync::status` alone, until
  no-vault-mode removes `IndexSync` from the no-vault launch.
- A picker cannot forget the scanning state: `Runs` sets and clears it; no
  delegate implements a hook for it.

## Deviations from Zed

None in shape. Zed stores its run-level copy in each search view's state; we
store it in `Runs`, which every picker over the index shares. Zed has two
views and no shared run driver; we have `Runs` already
(`2026-09-29-query-run-design.md`).

## Tests

Executable success criteria, written first:

1. `crates/index/src/live/tests.rs`: `status_goes_scanning_indexing_ready`.
   It drives the harness through the first build, two held loads and their
   landing, and asserts `Scanning`, `Indexing { loaded: 0, total: 2 }`,
   `Indexing { loaded: 1, total: 2 }`, `Ready`. It fails to compile before
   slice 1.
2. `crates/index/src/live/model_tests.rs`: after each step,
   `status()` agrees with `progress()` and the first-build flag, and never
   yields `Indexing` with `loaded >= total`. It fails to compile before
   slice 1.
3. `crates/app/src/index/tests.rs`: `a_store_without_a_vault_is_ready`. An
   `IndexSync` over `NoteStore::new(fs, None, cx)` answers `Status::Ready`.
   It fails to compile before slice 1. No-vault-mode slice 2 deletes it with
   the arm it tests.
4. `crates/app/src/vault_search/tests/states.rs`:
   `the_scanning_text_holds_until_the_first_rows_land`. Type during a held
   scan, release it with one load still blocked, `run_until_parked`: the empty
   text is still "Scanning the vault…"; after `BUILDING_REFRESH` the rows
   list. It passes before slice 3, and it is written before slice 3 to catch
   the plausible bug of clearing the flag at run start or reading the live
   status.

Tests that change:

- `crates/app/src/index/{tests,loading_tests,corpus_tests,startup_tests,scanning_tests,words_tests}.rs`:
  about 25 `is_building()`, `progress()` and `awaits_first_build()` asserts
  become `status()` asserts. A mid-load `(1, 3)` becomes
  `Indexing { loaded: 1, total: 3 }`. A final `(n, n)` becomes `Ready`, plus
  `index().names().len() == n` where the total matters
  (`corpus_tests.rs:93`, `loading_tests.rs:94,231,255,370`, `tests.rs:802`).
- `crates/app/src/backlinks/tests.rs`: unchanged; `building` stays a field.
- The scanning and indexing text tests stay unchanged and must stay green:
  `quick_open/tests.rs:337`, `quick_open/tests/rows.rs:145`,
  `quick_open/tests/refresh.rs:198-208`, `vault_search/tests/states.rs:55-129`,
  `shell/tests/open_first.rs:120-175`. They are the refactor's guard.
- `crates/index/src/live/{tests,startup_tests,model_tests}.rs`: unchanged,
  except where they call `is_building` or `awaits_first_build` from outside
  `live` (none found; they are submodules of `live`).

Look: render `quick_open` and `vault_search` scenes `scanning` and `indexing`
(`shot/scenes.rs:223-273`) after slice 3 and compare with the scenes before.

## Plan

Each slice is green under `./scripts/check.sh` on its own.

1. **`Status`, answered to the footers.** Data: `live::Status`,
   `LiveIndex::status`, `IndexSync::status`, free `indexing_footer(Status)`
   (replaces the method). Behaviour: the two picker footers and vault
   search's empty text read `status()`; the other readers keep the old
   accessors for one slice. Tests: criteria 1, 2 and 3; the footer and
   indexing text tests listed above stay green; `cargo mutants` on
   `inkra-index` for `status()` (the `loaded < total` boundary and the
   first-build arm) before the slice closes, zero missed. About 120 lines.
2. **The other readers, and the accessors go.** Behaviour:
   `Runs::update_matches`, `IndexWatch` and `BacklinksPanel::rebuild` read
   `status()`. Data: delete `IndexSync::{progress, is_building,
   awaits_first_build}`; make `LiveIndex::{is_building, awaits_first_build}`
   private. Tests: the app index tests migrate. About 130 lines, most of them
   mechanical test edits. `inkra-index` logic changes: `Status::is_ready` is
   added and `LiveIndex::is_building` reads it. So `cargo mutants -p
   inkra-index --file crates/index/src/live.rs -F "is_ready|is_building"`
   runs before the slice closes, zero missed. Run on 2026-09-30: 7 tested,
   7 caught, 0 missed, 0 timeouts.
3. **`Runs` holds the scanning answer.** Data: `Runs::scanning`. Behaviour:
   `IndexDelegate::scanning` and both `Answered::Scanning` arms are deleted;
   quick open's `NotesChanged` reads `runs.scanning()`. Tests: criterion 4,
   written first; the scanning tests listed above stay green. About 70 lines.

Every slice carries data, a visible reader and a test. Slice 1 has a caller
from the start, so the type's Interface is proven by the footers, not only by
`inkra-index`'s own tests.

## Merge order

**This spec lands before index-turn-effects and no-vault-mode.** Both
specs state the same order.

- index-turn-effects (`2026-09-30-index-turn-effects-design.md`) reshapes
  `IndexSync`'s write side. This spec touches only the read side (`status`)
  and does not depend on how loads land. index-turn-effects then rebases
  onto `LiveIndex::status`: its getter block lists `status`, not
  `is_building` and `awaits_first_build`, and its `Seen` check reads
  `status`.
- no-vault-mode (`2026-09-30-no-vault-mode-design.md`) removes the no-vault
  `IndexSync`. Both are read-side, and this one is smaller. No-vault-mode's
  slice 2 then deletes one `match` arm in `IndexSync::status` and
  criterion 3, instead of the two `vault.is_some() &&` guards in
  `is_building` and `awaits_first_build`.

## Risks

- Sticky scanning answer (slice 3). Setting or clearing `Runs::scanning` at a
  different point than today brings back a blank frame or a stale "Scanning"
  text. Criterion 4 and `quick_open/tests.rs:337` guard both directions.
- Quick open's `NotesChanged` handler picks `Run::Search` on the first build.
  It must read the flag before any run clears it. The existing test
  "lists the first names without the refresh wait" guards it.
- The gain is modest: about 40 lines net, one trait method, two enum arms and
  four accessors fewer. It lands first (Merge order), so no other spec has
  rewritten these readers before it.
- Performance: `status()` costs one walk of in-flight loads, as
  `is_building()` does today. No foreground turn does more work than before.

## Out of scope

- The wording of each message. "No matches in N of M notes indexed so far"
  stays in vault search; it is query-specific.
- Removing the no-vault arm. No-vault-mode does it by building no `IndexSync`
  without a vault.
- Backlinks' change detection and `IndexWatch`'s throttle policy.
- A status line or progress bar outside the pickers.

## Review

Red/blue on 2026-09-30. All four findings confirmed; no disagreement to
arbitrate.

| Finding | Outcome |
|---|---|
| `red-1-no-vault-mode-overlap` | Fixed. Risks names no-vault-mode and picks the order (this spec first); the `IndexSync::status` section marks the no-vault arm as temporary; criterion 3 and the invariant say who deletes them. Checked `index.rs:107-128`: both guards are the ones no-vault-mode's slice 2 lists. |
| `red-2-mutants-gate-deferred-past-slice-1` | Fixed. `cargo mutants` on `inkra-index` moves into slice 1, where `status()` is written. Slice 2 also changes `index` logic (`Status::is_ready`, `is_building`'s body), so it runs its own targeted mutants; slice 3 changes none. |
| `red-3-vault-search-results-reset-unaddressed` | Fixed. "What moves where" has a row for the `results` reset, argued dead by the same invariant as `List::clear`. |
| `red-4-slice-1-is-horizontal` | Fixed by restructure, not by an argued exception. Slice 1 now wires `IndexSync::status` into the footers and vault search's empty text, with criterion 3; slice 2 takes the remaining readers and deletes the accessors. |

Improvements adopted: the Risks bullet (ordering and rebase cost in one
place), mutants in slice 1, the symmetric table row, the restructured slice 1.
Rejected: editing `no-vault-mode-design.md` to point back here. Another agent
owns that spec; the reciprocal line is requested in Risks instead.

Verdict: proceed. The gain stays modest (Risks, last bullets); the review
found ordering and process gaps, not a flaw in the shape.

Cross-spec reconciliation (2026-09-30): new Merge order section: this spec
lands before index-turn-effects and no-vault-mode, and the "either order"
fallback is gone; candidate numbers replaced by spec slugs; `Runs::scanning`
is stated to stay in the driver `Runs`, not in runs-testable's `Ledger`.
