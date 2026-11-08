# Delete the unused fuzzy name matcher

Status: **built 2026-09-29.** Refactor: quick open, vault search and the
launch line behave as before.

## Why

The D2 name matcher (`2026-09-24-search-design.md`, D2) lost its last
production caller when quick open moved to word-start matching
(`2026-09-25-smart-quick-open-design.md`, S5). S5 kept it "for `[[` link
completion". That feature is in the search spec's Deferred table only. It is
not on `docs/roadmap.md` and has no trigger fired. By the house rule, a layer
for a future that is not on the roadmap does not stay.

The matcher still costs every build. `NamesSnapshot::build`
(`crates/index/src/names.rs:72`) runs `Candidate::new` per note on each
`NoteSet::build`, off the foreground. Per note it collects the path into a
`Vec<char>`, folds each char into a `PathChar`, computes a bonus per char,
and fills a `CharBag`. `find` reads three fields of a `Candidate`: `id`,
`path` and `chars.len()`. The rest is built and never read.

Callers of `match_names`, `merge_names`, `NameQuery` and `NameMatch`, checked
2026-09-29:

| Caller | Kind |
|---|---|
| `crates/index/benches/names.rs` (`bench_match`, `bench_realistic`, `bench_parallel`) | bench |
| `crates/index/src/names/{tests,model_tests}.rs` | the matcher's own tests |
| `crates/index/src/note_set/tests.rs:7,16,66` | test |
| `crates/index/src/index/tests.rs:7,297` (`names`) | test |
| `crates/app/src/test_support.rs:114` (`named`, 11 call sites) | test |
| `crates/app/src/index/diff_tests.rs:162` (`names`) | test |
| `crates/app/src/index/tests.rs:886` (`assert_pending_are_notes`) | test |

No production code calls them.

## What Zed does

Zed's file finder and completions share one matcher, `crates/fuzzy`
(`match_path_sets`, `match_strings`). Zed keeps it because both have live
callers. Our quick open matches by word start (smart quick open, S3), so
nothing is left to share. When `[[` completion is built, its spec picks a
match rule and reads `vendor/zed-reference/crates/fuzzy` first. The deleted
matcher stays in git history (see Plan, step 3, for the commit to cite).

## The module after the change

`NamesSnapshot` (`crates/index/src/names.rs`) stays the per-build table of
note names that `find` reads. It hides how a name is stored and folded.

```rust
/// Each note's name in vault order, as `find` reads it. Cloning is an `Arc` bump.
#[derive(Clone, Default)]
pub struct NamesSnapshot {
    names: Arc<[Name]>,
    words: Arc<NameWords>,
}

struct Name {
    id: EntryId,
    /// NFC, `/`-separated, `.md` stripped.
    path: Arc<str>,
    /// `path.chars().count()`: the path-length tie-break of `find`'s rank.
    chars: usize,
}

impl NamesSnapshot {
    pub(crate) fn build(notes: Vec<(EntryId, Arc<str>)>) -> Self;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    /// The name words' bytes and arrays; the names are not counted.
    pub fn heap_bytes(&self) -> usize;
    pub(crate) fn chunks(&self, n: usize) -> Vec<Range<usize>>;
    pub(crate) fn id(&self, order: usize) -> EntryId;
    pub(crate) fn path(&self, order: usize) -> &Arc<str>;
    pub(crate) fn chars(&self, order: usize) -> usize;
    pub(crate) fn words(&self) -> &NameWords;
}
```

Invariants, unchanged:

- Position `order` is the note's vault order, as in `NoteTable` and the
  corpus. `names` and `words` have one entry per note of the build.
- `path` is the `Arc` that `prepare` made, shared with `NoteTable`.
- `heap_bytes` returns the same value as before: it never counted the
  candidates, so the launch line's `names N bytes` does not move.

What goes: `Candidate`, `names/{bag,fold,matching,query,score}.rs` and their
test files, `names/model_tests.rs`, and the re-exports of `NameMatch`,
`NameQuery`, `match_names`, `merge_names` from `lib.rs`. `chunks` becomes
`pub(crate)`: its one caller is `IndexSnapshot::chunks`. `NoteSet::names()`
goes: after the rewrite below, no caller is left. Tests in the crate read the
`pub(crate)` field `NoteSet::names`.

`NameWords` stays where it is (`words/name_words.rs`); `find` reads it
through `NamesSnapshot::words`.

### The "one stats method" option: declined

The brief proposed replacing `NoteIndex::names()`, `corpus()` and `words()`
with one stats method for `mark_indexed` (`crates/app/src/index.rs:229-236`).
It is not a small change, and it narrows nothing. `corpus()` and `words()`
have about 40 other callers: index tests, app tests, and the `search` and
`words` benches, which are separate crates and need them `pub`.
`names().len()` also feeds `IndexSync::progress`. So `names()`, `corpus()`
and `words()` stay. `NamesSnapshot`'s public surface shrinks to `len`,
`is_empty` and `heap_bytes`.

## Tests

The rewritten tests assert the same property through `find_notes` or through
`NamesSnapshot` directly. A name-only helper keeps `Group::Name` rows only,
because `find_notes` also matches text, and a note whose text links `[[b]]`
would otherwise list under `b`. Word start is stricter than subsequence, but
every query in these tests is a whole name, so each expected list is the same.

| Test | Before | After |
|---|---|---|
| `note_set/tests.rs` `paths` (3 tests) | empty `NameQuery` with every note recent | `set.names.path(order)` for `order in 0..len` |
| `note_set/tests.rs` `a_candidate_index_is_its_note_order` | `match_names` one position at a time | `set.notes.order(set.names.id(at)) == at`; renamed `a_name_position_is_its_note_order` |
| `note_set/tests.rs` `a_snapshot_splits_into_ranges_over_its_notes` | `set.names()` | `set.names`; asserts unchanged |
| `index/tests.rs` `names` helper | `match_names`, query text | `find_notes` + `merge_notes`, `Group::Name` rows' paths |
| `words/name_words/tests.rs:50` | `note_set(&vault).names()` | `note_set(&vault).names` |
| `names/tests.rs` | 29 matcher tests | keeps `heap_bytes_counts_the_name_words` (reads `.names`) and a `NamesSnapshot`/`NoteSet` `Send + Sync` check; the rest go with the matcher |
| app `test_support.rs` `named` | `match_names` | `snapshot.lookup` → `find_notes` over `0..len` → `merge_notes`, `Group::Name` rows' ids, limit 10 |
| app `diff_tests.rs` `names` | `match_names` per position, every note recent | `set_notes` into a fresh `NoteIndex`, then `find_notes` per position, empty `NoteQuery` with every note recent, limit 1 |
| app `index/tests.rs` `assert_pending_are_notes` | `match_names`, empty query, pending recent | `find_notes` over `0..len`, empty `NoteQuery` with pending recent, limit `pending.len()` |

An empty `NoteQuery` lists exactly its recent notes in the range, less the one
showing (`find/rank.rs:36-40`), as an empty `NameQuery` did. So the two
position-reading helpers keep their meaning.

Executable success criteria:

1. `cargo test -p inkra-index -p inkra-app` passes, and
   `./scripts/test-names.sh` diffed before and after shows only the removed
   matcher tests, the renamed test, and no other loss.
2. The `find` model test (`find/model_tests.rs`, which models the path-length tie-break as
   `path.chars().count()`) passes: `chars` is still the char count.
3. Deleting a note from `NamesSnapshot::build`'s output (for example,
   `.skip(1)`) fails the `note_set` path tests. Swapping `id` and position
   fails `a_name_position_is_its_note_order`. Dropping the `Group::Name`
   filter from `named` fails
   `the_notes_are_built_off_the_foreground_and_follow_a_rescan`. This proves
   the rewritten tests still pin the table.
4. `grep -rn "match_names\|merge_names\|NameQuery\|NameMatch\|Candidate" crates`
   returns nothing, and `grep -c 'names/score' .cargo/mutants.toml` is 0.
5. `cargo bench -p inkra-index --bench names -- --quick "NoteSet::build"`
   before and after: the 50k build gets faster. Record both figures in "As
   built". This is a comparison, not a gate.
6. `./scripts/check.sh` is green. `cargo mutants` on `inkra-index` has zero
   missed in `names.rs` and `find/`.

## The bench

`benches/names.rs` keeps `bench_build` and `bench_set_notes`. They measure
`NoteSet::build` and `set_notes`, which this change makes cheaper.
`bench_cases`, `bench_match`, `bench_realistic`, `parallel` and
`bench_parallel` go, with their imports. The file keeps its name: `paths.rs`
under `benches/names/` is shared with the `find` bench, and a rename only
moves lines.

## Deviations from Zed

- No shared fuzzy matcher. Zed has two callers for `fuzzy`; we have none.

## Risks

- **A rewritten test goes vacuous.** A helper that returns nothing makes an
  `is_empty` assertion pass. Criterion 3 injects a fault to catch it.
- **`find_notes` panics on a note without a corpus slot**
  (`find/rank.rs:47`). `set_notes` reorders the corpus with the names, so a
  test that reads through `NoteIndex::snapshot` always has both. The
  `diff_tests` helper must apply its `NoteSet` with `set_notes` before it
  queries.
- **Text matches leak into `named`.** Filtering on `Group::Name` removes
  them. No current `named` fixture puts the queried name in another note's
  text, so no test pins the filter today. Step 1 gives note `a` of
  `the_notes_are_built_off_the_foreground_and_follow_a_rescan`
  (`app/src/index/tests.rs`) the text `see [[b]]`: without the filter,
  `named(.., "b")` would then list `a`. Criterion 3 drops the filter to
  prove it.
- **Stale docs.** Search spec D2 and the Deferred row for `[[` completion, and
  smart quick open S5, still say `match_names` stays. Step 3 amends them.

## Plan

Each step builds, passes its tests, and is one commit.

1. **Tests off the matcher.** Rewrite the helpers and tests in the table
   above to use `find_notes` or `NamesSnapshot` directly; the matcher still
   exists. Run `./scripts/test-names.sh` before this step and keep the output.
   `cargo test -p inkra-index -p inkra-app` passes. Run the fault injection
   of criterion 3, then revert it.
2. **Delete the matcher and `Candidate`.** Replace `candidates` with
   `names: Arc<[Name]>`. Delete `names/{bag,fold,matching,query,score}.rs`,
   `bag/`, `fold/`, `model_tests.rs`, the matcher tests in `names/tests.rs`,
   the `lib.rs` re-exports, `NoteSet::names()`, and the match benches.
   Remove the two `names/score\.rs` `exclude_re` entries from
   `.cargo/mutants.toml` (`fits`/`score` and `reachable`): with the file gone
   they can never match. Make `chunks` `pub(crate)`. Take the bench figure of criterion 5 before and
   after. Criteria 1, 2, 4 and 6 pass.
3. **Docs.** Amend search spec D2 and its Deferred row, and smart quick open
   S5: "deleted 2026-09-29, see `2026-09-29-names-delete-design.md`; restore
   from `<commit before step 2>` or start from Zed's `fuzzy`". Fill "As built"
   here with the bench figures and any deviation from this plan.

## As built

- **Bench, criterion 5.** `cargo bench -p inkra-index --bench names -- --quick
  "NoteSet::build"`: 117.92 ms before, 98.34 ms after, 16.6% faster.
- **One app helper.** `named`, the `diff_tests` `names` helper and
  `assert_pending_are_notes` share `test_support::quick_open`: `lookup`, then
  `find_notes` over a range, then `merge_notes`. The index crate's `names`
  helper stays inline, beside the crate's other test helpers.
- **The `Group::Name` pin.** Note `a` of
  `the_notes_are_built_off_the_foreground_and_follow_a_rescan` has the text
  `see [[b]]`. With the filter dropped, `named(.., "b")` returns `[b, a]` and
  the test fails.
- **Criterion 3, run.** `.skip(1)` in `build` fails the five `note_set`
  tests. Reading `id` in reverse fails `a_name_position_is_its_note_order`.
- **Three more call sites.** `index/tests.rs` read `NoteSet::names()` on a
  `set_notes` result three times; they read the field now.
- **The thread-crossing test is renamed** to
  `the_name_table_and_note_set_cross_threads`: the matcher types it named are
  gone.
- **`long_stem` moves into `benches/find.rs`,** its one user now. Left in
  `benches/names/paths.rs`, it is dead code in the `names` bench and fails
  clippy. The `names` bench builds its vault with `paths::folders`, which
  draws the same words in the same order, so the benched vault does not
  change.
- **`.cargo/mutants.toml`** also loses the two comment paragraphs that
  justified the `names/score.rs` exclusions.
- **Mutants.** `cargo mutants --in-diff` on `inkra-index`: 18 mutants, 8
  caught, 10 unviable, 0 missed.
- **test-names diff.** 44 names go and 3 come. 41 are the deleted matcher
  tests (`names/{tests,model_tests}.rs`, `bag/tests.rs`, `fold/tests.rs`).
  The other three are renames:
  `notes_are_candidates_in_vault_order_and_folders_are_not` →
  `names_are_in_vault_order_and_folders_are_not`, `a_candidate_index_is_its_note_order` →
  `a_name_position_is_its_note_order`, `the_matcher_types_cross_threads` →
  `the_name_table_and_note_set_cross_threads`.
- **Restore point.** `9772a24`, the base on `main`, still has the matcher.
