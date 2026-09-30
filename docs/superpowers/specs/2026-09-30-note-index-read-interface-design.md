# NoteIndex read-only Interface: dropped

Status: **dropped 2026-09-30.** This architecture candidate fails the
deletion test. No code changes.

## Problem as proposed

The candidate said `NoteIndex` exposes a writer Interface that only
`LiveIndex` should hold:

- `set_notes`, `insert`, `remove`, `words_merge` and `words_apply` are `pub`
  (`crates/index/src/index.rs:50,121,133,146,168`). Their only production
  caller is `LiveIndex` (`live/builds.rs:146`, `live/loads.rs:132,160`,
  `live/startup.rs:87`).
- A direct `words_merge` would bypass the `Startup` state machine
  (`live/startup.rs`).
- `lib.rs:20-33` re-exports types that `app` never names.
- `app` reads the corpus, words and names only for three `heap_bytes`
  figures (`crates/app/src/index.rs:176-186`).
- One `app` test builds a `NoteIndex` by hand
  (`crates/app/src/quick_open/render/tests.rs:16-23`), because
  `test_support` in `inkra-index` is `cfg(test)` (`lib.rs:15`).

The proposed Decision: make every mutator `pub(crate)`, collapse the
diagnostics into one read, and expose a fixture behind a `test-support`
feature, like `MemFs` in `inkra-vault`.

## Why it does not earn its keep

### The invariant already holds by ownership

`LiveIndex` owns its `NoteIndex` and hands out `&NoteIndex` only
(`live.rs:54`). `IndexSync::index` forwards that shared reference
(`app/src/index.rs:98`). No production path in `app` holds a
`&mut NoteIndex`. The production reads are `snapshot`
(`picker/runs.rs:200`, `quick_open/mod.rs:81`), `resolve_link`
(`shell/activation.rs:131`), `backlinks` (`backlinks/mod.rs:96`) and the
three snapshots in `mark_indexed`. So `app` cannot call `words_merge` and
cannot bypass `Startup`. The borrow checker already makes `LiveIndex` the only
writer. Hiding the mutators restates that invariant a second time.

### Deletion test

Delete `pub` from the five mutators. No production caller changes. Two
groups of callers break:

| Caller | What it needs |
|---|---|
| `crates/index/benches/{index,names,find,search}.rs`, `benches/words/build.rs` | `set_notes`, `insert`, `words_merge`, `words_apply`, `MergeWhen`, `CorpusSnapshot`, `WordsSnapshot`, `Lookup`, `Source` |
| `crates/app/src/quick_open/render/tests.rs:16-23` | `set_notes`, `insert` on a one-note index |

The benches are separate crates, so they see only the public Interface. They
measure each mutator by itself: `set_notes` at 10k and 50k notes,
`words_apply` per merge, the foreground cost of each insert
(`benches/words/build.rs:19-63`). The 8 ms foreground-turn budget rests on
these numbers. Moving the benches onto `LiveIndex` would add tickets, the
load queue and the `Startup` policy to every sample. The numbers would then
measure the scheduler as well as the index.

The remaining option is a feature that re-exposes the mutators to the
benches. That keeps the same surface under a different gate. The Interface
does not get smaller and no caller learns less. This is relocation, not
depth.

### The friction is small

- One `app` test builds an index by hand, in eight lines. A `test-support`
  fixture would replace those eight lines with a new feature, a new Cargo
  edge and a new public module. That is more Interface, not less.
- `mark_indexed` reads three snapshots in one function, once per launch
  (`app/src/index.rs:176-186`). One `footprint()` read would save two lines.
  It would also add a struct that exists for one caller.
- `insert` before `set_notes` is a no-op (`index.rs:147`). This is a
  documented invariant, not a trap: a load that lands after its note was
  trashed must leave nothing behind. `LiveIndex` depends on it.

### Leverage and Locality

The change gives no caller more behaviour per unit of Interface. It moves no
knowledge closer to where it is used. Every rule that the candidate wanted to
protect already lives in `live/`, next to its tests
(`live/startup_tests.rs`, `live/model_tests.rs`).

## Deviations from Zed

None. The candidate did not touch a subject that Zed solves differently.

## Tests

None added, none moved.

## Plan

None.

## Risks of dropping

- A future production caller could construct its own `NoteIndex` and write
  to it outside `LiveIndex`. Today no roadmap item needs a second index. If a
  change adds one, it is visible in review: it is a new `NoteIndex::default()`
  outside `inkra-index`.

## Out of scope: hygiene, not a deepening

These items are small and independent. Any change that touches
`crates/index/src/lib.rs` for its own reason can take them. They do not
justify a change of their own.

- `NamesSnapshot`, `Target`, `HITS_PER_NOTE`, `MergedWords`, `RetiredWords`
  and `WordsMerge` are re-exported, but no bench and no `app` file names
  them. The last three appear in public signatures that benches call. Remove
  a re-export only after `cargo doc` and clippy accept the unnameable type.
- `NoteIndex::resolve(&Target)` (`index.rs:199`) has no caller outside the
  crate. It can become `pub(crate)`, which leaves `Target` without an
  external use.

## Review

Cross-spec reconciliation (2026-09-30): the candidate number in Status is
replaced by a plain reference to this spec.
