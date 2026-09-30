# `Jobs` gets its own module in `inkra-vault`

Date: 2026-09-30. Status: **proposed**.

A refactor from the architecture review. Behaviour stays identical. The public
Interface, `inkra_vault::{Job, Jobs, Serial}`, stays identical too: no caller
outside `inkra-vault` changes a line.

## Problem

`crates/vault/src/vault/walk.rs` holds two subjects.

- `walk.rs:15-35` and `:63-268`: the directory walk. `Walk`, `Reach`, the
  shared `Queue`, `Shared::job`, `Vault::walk`, `walk_seeded`, `walk_changed`
  and the private `run`.
- `walk.rs:37-61`: the parallelism Seam. `Job<'a>`, the trait `Jobs`
  (`parallelism`, `run`), and its inline Adapter `Serial`.

The second subject is not the walk's. The query-run spec widened it to
"a walk's or a query's" (`2026-09-29-query-run-design.md`, "`Jobs` is
`inkra_vault::Jobs`, the walk's trait") and left it where it was. Today its
callers are:

- the walk: `walk.rs:154`, `:174`, `:192`, `:246-263`; `scans/run.rs:5` and
  `:81` reach it through `crate::vault::Jobs`;
- `Vault::scan`, `vault.rs:191`, with `Serial`;
- the query: `crates/index/src/query.rs:6`, `fan_out` at `:43-73`;
- the app: `crates/app/src/background.rs:4-24` (`Background`), and
  `picker/runs.rs:9`, `quick_open/mod.rs:12`, `vault_search/mod.rs:12`, which
  pass `&dyn Jobs` through.

Adapters of `Jobs`: `Serial`, `Background` (app), and test or bench fakes —
`Threads` in `vault/parallel_tests.rs:21` and `vault/benches/vault.rs:39`,
`Ranges` and `CancelsAfterOne` in `index/src/query/tests.rs:16-44`, `Threads`
in `index/benches/find.rs:105`. The Seam is real; only its file is wrong.

The friction is Locality. A reader who follows `inkra_vault::Jobs` from
`query.rs` or `background.rs` lands in a 268-line file about folder queues and
`Reach`. The project rule "one module per concern" (`CLAUDE.md`, How we build)
names this case: a file that mixes two subjects.

### Does it belong in `vault`?

The explorer's premise was that the `index → vault` edge partly exists for
`Jobs`. It does not. `inkra-index` references `EntryId` in 37 files (18
non-test files import it directly), and `Mtime`, `Entry`, `Kind` and `Vault`
beside it; `query.rs` is the one non-test file that imports `Job`/`Jobs`. Moving `Jobs` into `index` would invert the edge: the walk, below
`index`, needs it. A new crate for 25 lines of trait fails the deletion test:
it would hold one trait and one Adapter, and it adds a workspace member to
`check.sh` and the fmt list for no Leverage. So `Jobs` stays in `inkra-vault`,
at its root, beside `fs` — the crate's other executor-free Seam.

## Decision

A new private Module `crates/vault/src/jobs.rs`, declared `mod jobs;` in
`lib.rs`, re-exported at the crate root. Its Interface, unchanged in every
character:

```rust
pub type Job<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Runs jobs to completion, a walk's or a query's: the app on its background threads, a test inline.
pub trait Jobs: Sync {
    /// How many jobs to start.
    fn parallelism(&self) -> usize;
    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a>;
}

/// One job, run inline: `scan`, `rescan` and tests.
pub struct Serial;
```

What a caller must know, stated once in the module doc:

- `run` resolves once every job it was given has run to completion. `fan_out`
  relies on it (`query.rs:67-69` panics otherwise); the walk relies on it to
  take the queue back (`walk.rs:263-267`).
- `parallelism` is a hint for how many jobs to start. Either caller may start
  another number: the walk starts `parallelism().max(1)`, the query one per
  non-empty range.
- Jobs borrow from the caller (`'a`): an Adapter must scope them, never spawn
  them `'static`.

The Module is shallow on purpose: it is a Seam, and a Seam's Interface is the
whole of it. The gain is Locality, not Depth.

## What moves where

| From | To |
|---|---|
| `vault/walk.rs:37-61` (`Job`, `Jobs`, `Serial`, `impl Jobs for Serial`) | `jobs.rs`, verbatim |
| `use std::pin::Pin` in `walk.rs:7` | `jobs.rs`; `walk.rs` no longer needs it |
| `vault.rs:15` `pub use walk::{Job, Jobs, Reach, Serial, Walk}` | `pub use walk::{Reach, Walk}` |
| `lib.rs:25` `pub use vault::{.., Job, Jobs, .., Serial, ..}` | `pub use jobs::{Job, Jobs, Serial};` on its own line |
| `walk.rs` uses `Jobs`, `Job` from its own scope | `use crate::jobs::{Job, Jobs};` |
| `vault.rs:191` `Serial` through `walk` | `use crate::jobs::Serial;` |
| `scans/run.rs:5` `crate::vault::{Jobs, ..}` | `crate::jobs::Jobs` |
| `vault/*_tests.rs` get `Serial` through `use super::*` | still do, from `vault.rs`'s `use crate::jobs::Serial` |
| `vault/parallel_tests.rs` gets `Job`, `Jobs` through `use super::*` | `use crate::{Job, Jobs};`: `vault.rs` imports only what it uses, so the glob no longer carries them |

The `walk.rs` module doc stays "Reading a vault's folders from disk". The
query-run spec's pointer `vault/src/vault/walk.rs:37-61` becomes stale; this
spec supersedes it, and the query-run spec is not edited (specs record their
day).

## Invariants now held by types

None new. The move keeps every type. One invariant becomes local: nothing
inside `walk.rs` can reach a private item of `Jobs`, because there is none —
the module boundary makes it visible that `Jobs` owes the walk nothing but its
public Interface.

## Deviations from Zed

None added. Zed's `fuzzy` and `worktree` take a `BackgroundExecutor`
directly; `inkra-vault` and `inkra-index` have no gpui, so they take `Jobs`.
That deviation is recorded in the query-run spec ("`Jobs`, not the executor")
and is unchanged.

## Tests

No test moves, shrinks or dies. `Serial` has no test of its own and gets none:
every walk and query test runs through it, and `parallel_tests.rs` and
`query/tests.rs` pin the Seam's contract through its Interface.

The executable success criterion, written before the change:

1. `./scripts/test-names.sh > /tmp/before.txt` on the base commit, then
   `> /tmp/after.txt` after the change; `diff` is empty. A dropped `mod` line
   compiles green and fails this.
2. `grep -n 'trait Jobs' crates/vault/src/vault/walk.rs` exits 1, and
   `grep -n 'trait Jobs' crates/vault/src/jobs.rs` exits 0. On the base commit
   the first grep exits 0, so the criterion fails before the change.
3. `grep -rnwE 'Jobs?|Serial' crates/index crates/app` output (43 lines today)
   is byte-identical before and after: no caller outside `inkra-vault` changed a
   line that names the Seam. Whole-word names, not the `inkra_vault::` prefix,
   so an import alias cannot hide a changed use.
4. `./scripts/check.sh` green.

No `cargo mutants` run: no logic changes.

## Plan

One vertical slice. It is a file move; splitting it leaves a slice with a
dangling re-export.

**J1. `jobs.rs`.** Create `crates/vault/src/jobs.rs` with the module doc and
the 25 moved lines. Declare `mod jobs;` in `lib.rs`, re-export
`{Job, Jobs, Serial}`. Drop them from `walk.rs` and `vault.rs:15`. Fix the four
internal imports (`walk.rs`, `vault.rs`, `scans/run.rs`,
`vault/parallel_tests.rs`). Run the four criteria above. About 60 lines
of diff, most of it moved lines.

## Risks

- **Coordination with `feat/restore-tabs`.** That branch edits
  `crates/vault/src/{fs,mem_fs,real_fs}.rs` and, at the time of writing,
  `crates/app/src/shell/tests/restore.rs`. J1 touches none of them. It touches
  `lib.rs`, which that branch may also touch if it adds a public name: the
  conflict would be one line in the `pub use` block, trivial to resolve. Land
  J1 after the restore-tabs merge, or rebase it.
- **The test globs.** Six `vault/*_tests.rs` files and three `scans/*_tests.rs`
  files `use super::*`. Today `vault.rs:15`'s `pub use` hands them `Job`,
  `Jobs` and `Serial`. After J1, `vault.rs` imports only `Serial`, so a test
  naming `Jobs` fails to build under `cargo test` while `cargo build` stays
  green. `check.sh` runs the tests, so it catches the miss; criterion 1 then
  proves no test was lost to it.
- **Performance.** None: no code path changes. First frame and foreground
  turns are unaffected.

## Out of scope

- **`parallelism` as `NonZeroUsize`.** It would delete the guard
  `parallelism().max(1)` in `walk.rs:260` and the lower clamp in
  `index/src/chunks.rs:5` by data shape. Every Adapter returns at least 1 today
  (`num_cpus` is at least 1; `Serial` returns 1; the proptest range is
  `1..=8`), so the guard costs nothing and hides no bug. The change would touch
  seven Adapters across three crates for no behaviour. Revisit if an Adapter
  can compute 0.
- **Moving `Jobs` to `inkra-index` or a new crate.** Rejected above: the walk,
  below `index`, is a caller, and a one-trait crate fails the deletion test.
- **Renaming `Jobs` or `Serial`.** The names are used in 25 files and read
  well; a rename is a drive-by.

## Review

Red/blue pass, 2026-09-30.

- **RED-1** (minor, evidence count). Fixed. "Ten files" was wrong: `EntryId`
  appears in 37 files of `crates/index/src`, 18 of them non-test direct
  imports. Reproduced both greps. The conclusion is unchanged: only `query.rs`
  and its tests name `Job`/`Jobs`.
- **Blue: "20 files" for `Jobs`/`Serial`.** Fixed. The whole-word grep over
  `crates` counts 25. The no-rename conclusion is unchanged.
- **Blue: criterion 3 by whole-word names.** Adopted. No caller aliases the
  names today (`grep` for `as` on those imports is empty), but the whole-word
  grep is strictly stronger at no cost.
- **Blue: Interface block, slicing, test-glob risk.** Confirmations; no change.

No finding argues for dropping the candidate. It proceeds.

J1 review, 2026-09-30.

- **Q1** (major, contract placement). Fixed; deviates from "stated once in
  the module doc". `jobs` is a private module re-exported at the crate root,
  so rustdoc and hover on `inkra_vault::Jobs` never show its `//!` doc. Each
  clause now sits on its item: the `'a` scope on `Job`, the hint on
  `parallelism`, completion on `run`. The module doc keeps one line.
