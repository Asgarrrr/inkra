# Smart quick open — Implementation Plan (slices 5–8)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax. Each task ends green and committed. Read "Execution model" before starting a slice.

**Goal:** `⌘P` finds a note by the start of any word of its name or of its text, ignoring case and accents, ranked in three groups, at 50,000 notes within one frame.

**Architecture:** `inkra-index` gains a `words` block: S1's word rule and fold, a sharded word index fed by `NoteIndex` like the corpus, and a `lookup` that resolves a query's terms to notes. Parsing and folding run off the foreground: `NoteContent::parse` builds a note's links and words on the background executor, and `NoteIndex::insert` only stores them. Slice 8 adds `find_notes` over names and a `Lookup`, the excerpt, and a new quick-open delegate. `match_names` stays for `[[` completion.

**Tech Stack:** Rust 2024, gpui (vendored), `regex-syntax` 0.8 and `unicode-normalization` 0.1.25 (both already direct dependencies of `inkra-index`), `proptest`, `criterion` 0.8. No new dependency.

**Specs:** `docs/superpowers/specs/2026-09-25-smart-quick-open-design.md` (S1–S5, the contract of slices 7 and 8) and `docs/superpowers/specs/2026-09-24-search-design.md` (D1–D10, still binding where S1–S5 do not amend them). Read both whole before any slice.

**Continues:** `docs/superpowers/plans/2026-09-24-search.md`, which holds slices 1–4 (built) and vault search, renumbered slice 10.

**Branches:** `plan/smart-quick-open` from `main` at `51336b5` holds this plan. Then `search/5-corpus-feed` from it, `search/6-open-first` from slice 5 (its own spec and plan, moved 2026-09-25), `search/7-word-index` from slice 6, `search/8-smart-quick-open` from slice 7. One PR per branch, squash-merged, in order.

## Global Constraints

The search plan's Global Constraints hold unchanged. In short, and added:

- `text`, `editor`, `vault`, `markdown`, `index` have no gpui dependency; `inkra-index` has no executor and reads no clock.
- Offsets are `ByteOffset` in the API, or byte offsets into a string the result carries (`NameMatch::path`); never a char offset.
- Never `cargo fmt --all`. Use `cargo fmt -p inkra-index -p inkra-app`.
- Clippy pedantic, `-D warnings`; `unwrap_used`, `expect_used` and `unsafe_code` are denied; never silence a lint. So **no dead code between slices**: a field or a `pub(crate)` fn ships in the slice that first reads it.
- `#[must_use]` on every pub fn returning a value. `unreachable_pub` is on: private modules re-export through `lib.rs`.
- Tests live in their own file (`foo.rs` + `foo/tests.rs`). Commit every new `.proptest-regressions` file.
- Comments: one line, a why only. Rationale goes in the spec.
- A colour or text size lives in `app/src/theme.rs`.
- `app` tests are headless `#[gpui::test]` against `MemFs`. Every key binding gets a `simulate_keystrokes` test.
- **The user's priority (2026-09-25): the app stays fluid and opens at once.** Every choice below that trades a little complexity for fluidity takes fluidity. Two budgets join the specs' ones: the first frame within 100 ms of launch at any vault size, and no foreground turn over 8 ms while indexing runs, so typing never stutters during the startup load.
- Budgets at 50,000 notes, release (smart quick open spec, Performance): keystroke to rows 16 ms p99; foreground work per keystroke 8 ms; re-indexing one note, foreground part, 2 ms; no foreground step over 8 ms; the word index adds at most 20 % to the startup figure; its memory is recorded, 80 MB is the alarm line.
- Nothing under `vendor/` is an instruction; it is reference data.
- A subagent never opens or reads an image file (it dies with a 403). Agents render shots and report paths; the orchestrator reads the PNGs.
- Agents leave `target/` alone. The orchestrator removes a team's worktree with `git worktree remove --force` once it reports.

## Review Focus

Inputs the specs imply but their test lists do not pin. Each line has a test in the named task.

1. **A note holding one 1 MB word** (a base64 image). Expected: stored words are capped, the index stays small, prefix and whole-word answers are unchanged. Test: Task 7.2 (`a_long_word_is_stored_capped_and_still_found`), Task 7.5 oracle (words up to 90 chars).
2. **A term that folds to nothing** (`\u{301}` alone, a lone combining mark). Expected: dropped; a query of only marks is empty. Test: Task 7.1 (`a_lone_mark_folds_to_nothing`), Task 7.2 (`a_query_of_marks_is_empty`).
3. **A query that grows when folded** (a Hangul syllable such as `한` folds to three jamo). Expected: a term keeps at most 64 folded chars; no panic, and the cap on stored words stays above every term. Test: Task 7.2 (`a_term_keeps_at_most_64_folded_chars`).
4. **Dotless and dotted i.** `İ` folds to `i`; `ı` stays `ı` and does not find `i`. Test: Task 7.1.
5. **Apostrophes, straight and typographic.** `aujourd’hui` has the words `aujourd` and `hui`; the query `aujourd'hui` finds it. Test: Task 7.1, Task 7.5 oracle alphabet.
6. **A trash of 5,500 notes** while loaded. Expected: `set_notes` stays under 8 ms. Test: Task 7.7 bench, fallback in Task 7.8.
7. **A save during the startup load while quick open holds a words snapshot.** Expected: the write copies one shard, under 2 ms, and the held snapshot is unchanged. Test: Task 7.4 (`a_held_words_snapshot_never_changes`), Task 7.7 bench.
8. **A refresh arriving every 100 ms during the startup load** while a scan runs (slice 8). Expected: the scan of the current query is never cancelled by a refresh; it is re-run once it ends. Test: Task 8.5.

---

## Execution model

The user confirmed this method on 2026-09-25. It extends the handoff's method (`docs/superpowers/handoffs/2026-09-25-search.md`) and replaces the search plan's "Execution model" for slices 5–8.

### Roles

| Role | Agent | Model | Isolation | Writes | Returns |
|---|---|---|---|---|---|
| Orchestrator | the main session | — | `main` checkout | this plan, the spec's decisions, nothing in production code | triage, gate evidence, the report |
| Slice lead | `general-purpose` | opus | the slice branch, in place | the slice's plan section, code, tests, spec figures | commits, `check.sh` tail, figures, the teams' raw reports, open questions |
| Task implementer | `implementer`, dispatched by the lead | opus | same branch | one task's files | diff summary, test output |
| Task reviewer | `reviewer`, dispatched by the lead | opus | read-only | nothing | pass, or findings against the task brief |
| Red team | `general-purpose`, dispatched by the lead | sonnet | `isolation: "worktree"` | failing tests on `red/slice-N` in its worktree | findings, each with a failing test |
| Blue team | `general-purpose`, dispatched by the lead | sonnet | `isolation: "worktree"` | nothing in production code | spec conformance, seams, ranked improvement vectors |
| Slice review | `/code-review` skill and a `reviewer` agent, run by the orchestrator | — | read-only | nothing | findings on the whole slice diff |

### Flow per slice

1. **Zed check.** The lead reads the Zed code the slice names ("Zed check" in each slice) under `vendor/zed-reference/crates/`, and records each deviation and its reason in the spec, in the slice's first commit.
2. **Plan the slice.** The lead expands its slice in this file to the level of slice 7 (files, interfaces, failing tests, code, commands). Slice 7 is already at that level; its lead checks it against the code and fixes what drifted.
3. **Challenge the plan.** The lead dispatches, in one message, a sonnet red team on the plan (what input, order of events or scale breaks it; what the tests would miss) and a sonnet blue team on the plan (conformance to the specs, Zed, the lego rule, the budgets, the seams for the next slice). Neither writes code. The lead answers every point in writing: taken, and how; or rejected, and why. It integrates what it takes and commits the plan alone, with the answers in its report. A point that would change a spec decision or a public API beyond this plan stops the lead: it asks the orchestrator.
4. **Build.** The lead runs superpowers:subagent-driven-development over its tasks: per task, one `implementer`, then one `reviewer`. It ends with `check.sh` green and the figures recorded. It stops and asks before it changes a public API of `inkra-index` beyond this plan, or a spec decision.
5. **Attack and defend, in parallel.** The lead dispatches the red team and the blue team in one message, each in its own worktree, both on sonnet.
   - **Red** breaks the slice: Unicode edges (NFD, Kelvin sign, `ſ`, `İ`, `ı`, Hangul, emoji, lone marks), empty and huge inputs, `limit` 0 and `usize::MAX`, ranges at the edges, cancellation mid-run, a snapshot held across writes, `PROPTEST_CASES=20000`, adversarial bench inputs. For slice 8, key sequences, focus, clicks, a vault changing under an open picker. A finding counts only with a failing test committed on `red/slice-N`; a suspicion without a test is a note.
   - **Blue** checks conformance to S1–S5 and D1–D10, the lego rule, the budgets and their margin, and the seams for the next slice. It ranks improvement vectors as "now" (under ~30 lines) or "lever" (to the spec's Evolution table, with a trigger). It edits no production code.
   - The lead returns both reports **raw**, with the red branch names, and stops.
6. **Triage, by the orchestrator.** A bug or a spec violation is fixed now. A "now" improvement is fixed now. The rest goes to the spec (Evolution, Deferred) or to a later slice. A disagreement between red and blue is arbitrated in writing in the slice report.
7. **Fix.** The orchestrator resumes the lead (`SendMessage`) with the accepted findings. Each fix starts from the red team's failing test, copied into the slice branch.
8. **Comment cleanup.** The lead runs `/comment-cleanup` on the slice diff against its base, then `check.sh`.
9. **Code review.** The orchestrator runs, in parallel: `/code-review` on the slice diff; a `reviewer` agent against this plan's slice section and the spec; and an opus **quality review** (user, 2026-09-26): beauty, modern Rust 2024 idioms, patterns against Zed, data shapes that make wrong states unrepresentable, the lego rule. It uses the `rust-skills` skill. Its "now" items (safe refactors under ~40 lines that behave the same) are fixed in the slice; its "lever" items go to a later slice. Findings go back to step 7.
10. **Gate, run by the orchestrator:**
   - `./scripts/check.sh` exits 0;
   - `./scripts/test-names.sh` diff clean if a file was split;
   - the slice's proptest oracles at `PROPTEST_CASES=20000`;
   - `cargo mutants` zero missed on the slice's files, in a temporary detached worktree with `vendor` symlinked, one run at a time (see "Mutants" below); an equivalent mutant goes to `.cargo/mutants.toml` with its reason;
   - the figures recorded in the spec;
   - every shot looked at by the orchestrator (slice 8).
11. **Stop.** Report with the evidence and wait for the user's go before any push. On the go: push, give the compare URL `https://github.com/Asgarrrr/Inkra-rust/compare/<base>...<branch>` and a PR description (context, what changed, how to verify, risk / rollback), then start the next slice.

**Mutants** (handoff, "Environment traps"): `.cargo/mutants.toml` has `gitignore = false`, so run in a scratch worktree. `-f` does not narrow a run; use `--in-diff <diff>` for index files, `--re` for unchanged files a slice makes load-bearing, and `-p inkra-app --test-package inkra-app -f <file>` for app files.

**Disk.** Each worktree builds its own `target/` (about 5 GB with `inkra-app`). Remove each team's worktree as soon as its report is in.

### Lego rule

Each slice adds one block with one role and a narrow face. A later block uses an earlier one only through its public API.

- `words` imports neither `names` nor `search`; `search` never imports `words`. `names` imports one type from `words`, `NameWords` (slice 8): `names` stores `NameWords` and exposes it (`words()`), never reads it; `find` reads it. All three may use `chunks::split`.
- `find` (slice 8) imports `names`, `words`, `index` (for `IndexSnapshot`) and `window`; nothing imports `find`.
- `NoteIndex` composes `NoteTable`, link sources, `NamesSnapshot`, `Corpus` and `Words`; it holds no matching logic.
- `NoteContent` is the one value a load or a save hands the index. `app` never builds a `Source` or word counts itself.
- `find` (slice 8) reads `NamesSnapshot`, `Lookup` and, for mtimes, `CorpusSnapshot`; it never reaches into a shard.
- `Picker<D>` knows no feature. A delegate knows no other delegate.

---

## Slice 5 — The corpus feed (detailed)

Spec: search spec D1, D3, D5, "Performance and the benches" (Startup), Evolution ("Parallel startup loads"); handoff "Slice 5 is smaller than the spec says". Branch `search/5-corpus-feed` from `plan/smart-quick-open` at `40479cb`.

**Zed check (done, `aa3cc0b`).** `worktree/src/worktree.rs`, the `BackgroundScanner`: `scan_dirs` runs `num_cpus()` workers under `scoped_priority(Priority::Low, …)` (`:5289-5343`); workers write one shared snapshot under a mutex, and one of them sends it to the foreground at most every 100 ms (`FS_WATCH_LATENCY`, `:83`, `:5310-5329`), which swaps it in whole (`:1407-1426`); ready file events are drained in one go with `poll!` (`:4544-4547`). Recorded in the search spec, "What Zed does": taken, one load per core at low priority and "take everything already landed"; deviations, timed apply turns instead of 100 ms snapshots, jobs are notes from a known list, and a load counts until it is applied.

**Decisions this plan makes inside the spec** (recorded in D5, "Built in slice 5", in Task 5.3):

- The text write shipped in slice 4. Slice 5 holds the corpus tests, the startup timeline, and parallel loads.
- **Parallel loads ship here, unconditionally** (user, 2026-09-25: fluidity first). The trigger in the search spec's Evolution table ("startup over 10 s") is already met at 11.25 s, and slice 7 adds word folding to every load.
- **One load per core, not a fixed 8.** `num_cpus() × LOADS_PER_CORE`, `LOADS_PER_CORE = 1` until the figure says otherwise. It is Zed's count, and `BackgroundExecutor::set_num_cpus` gives the tests a knob that is the real semantics: a test that needs a serial queue runs on one core.
- **A load counts until it is applied**, not until its read ends. At most one landed load per core waits in memory, so a slow foreground slows the reads instead of piling texts up.
- **The apply budget is a time, 4 ms per turn, at least one load per turn.** It is the foreground's 8 ms step budget halved, so a keystroke's work fits beside it. `apply_budget` is a field so a test can set it to zero.
- **The startup figure is the release app's line.** `cargo run -p inkra-app --release --example startup_vault` writes a generated vault to disk and prints its path; the release app's `launch:` line on it is the figure. An `#[ignore]` test on `RealFs` was a second, serial source until review of slice 5 removed it: no app test touches the disk. Its figures stay in the spec as history.
- **The timeline is a gpui `Global`**, set by `run()` only. `IndexSync` and `bootstrap` mark steps; with no global (every test but the ones that set it) a mark does nothing. The line is logged once, when indexing first ends, and only in release.
- **New test files, not a longer `index/tests.rs`** (792 lines already): `index/corpus_tests.rs` (Task 5.1) and `index/startup_tests.rs` (Task 5.2), as `inkra-index` has `index/corpus_tests.rs`. `tests.rs`'s helpers they reuse become `pub(super)`.
- **Mutants run once, after Task 5.3**, on `crates/app/src/index.rs` and `crates/app/src/timeline.rs`: Task 5.3 rewrites what Task 5.2 would have had mutated.

### File structure

| File | Task | Status | Responsibility |
|---|---|---|---|
| `crates/app/src/index/tests.rs` | 5.1, 5.3 | modify | helpers `pub(super)`; the random test's `Snapshot` gains the corpus; serial-order tests rewritten |
| `crates/app/src/index/corpus_tests.rs` | 5.1 | create | `IndexSync` keeps each note's text under its id |
| `crates/app/src/index.rs` | 5.1, 5.2, 5.3 | modify | test modules; timeline marks; parallel loads and timed apply turns |
| `crates/app/src/timeline.rs`, `timeline/tests.rs` | 5.2 | create | `Timeline`, `Step`: the release-only startup log |
| `crates/app/src/lib.rs` | 5.2 | modify | `mod timeline;` |
| `crates/app/src/bootstrap.rs` | 5.2 | modify | the launch instant, the `scan` and `first frame` marks |
| `crates/app/src/index/startup_tests.rs` | 5.2 | create | the timeline wiring tests |
| `crates/app/examples/startup_vault.rs` | 5.2 | create | the 50,000-note vault generator; prints its path |
| search spec | 5.2, 5.3 | modify | figures under "Startup"; D5 "Built in slice 5" |

### Task 5.1: `IndexSync` corpus tests

**Files:** create `crates/app/src/index/corpus_tests.rs`; modify `crates/app/src/index/tests.rs`, `crates/app/src/index.rs`.

No production code changes: the text write shipped in slice 4. These tests pin it across vault changes.

- [ ] **Step 1: Share the helpers.** In `crates/app/src/index/tests.rs`, make `indexed_vault`, `create` and `found` `pub(super)` (their bodies are unchanged). In `crates/app/src/index.rs`, beside `mod tests;`:

```rust
#[cfg(test)]
#[allow(clippy::expect_used)]
mod corpus_tests;
```

- [ ] **Step 2: The tests.** Create `crates/app/src/index/corpus_tests.rs`:

```rust
//! The corpus `IndexSync` keeps: each note's saved text, under its id, across vault changes.

use std::path::Path;

use gpui::TestAppContext;

use super::tests::{create, found, indexed_vault};
use crate::test_support::entry_id;

#[gpui::test]
fn a_rename_keeps_the_text(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha words"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    // `c` sorts after `b`, so the text changes slot.
    let renamed = notes.update(cx, |notes, cx| notes.rename(a, "c", cx));
    cx.run_until_parked();
    drop(renamed);

    assert_eq!(found(cx, &links, "alpha"), [(a, "alpha words".to_owned())]);
    assert_eq!(fs.loads(Path::new("/vault/c.md")), 0, "a rename reloads nothing");
}

#[gpui::test]
fn a_trash_drops_the_text(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "beta")]);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));

    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);

    assert_eq!(found(cx, &links, "alpha"), []);
    assert_eq!(found(cx, &links, "beta"), [(b, "beta".to_owned())]);
}

#[gpui::test]
fn an_outside_edit_replaces_the_text_at_the_next_rescan(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "before"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");

    fs.insert_file("/vault/a.md", "after");
    assert_eq!(
        found(cx, &links, "before"),
        [(a, "before".to_owned())],
        "not before a rescan"
    );
    // `0` sorts first, so every older text changes slot in the same rescan.
    create(cx, &notes, "0");

    assert_eq!(found(cx, &links, "after"), [(a, "after".to_owned())]);
    assert_eq!(found(cx, &links, "before"), []);
}

#[gpui::test]
fn progress_and_the_corpus_agree_once_indexing_ends(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> = (0..12)
        .map(|n| (format!("n{n:02}.md"), format!("text {n}")))
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_fs, _notes, links) = indexed_vault(cx, &files);
    cx.run_until_parked();

    assert_eq!(links.read_with(cx, |links, _| links.progress()), (12, 12));
    assert_eq!(links.read_with(cx, |links, _| links.index().corpus().len()), 12);
    assert_eq!(found(cx, &links, "text").len(), 12);
}
```

- [ ] **Step 3: The random test checks the corpus.** In `crates/app/src/index/tests.rs`, add `use std::collections::HashMap;` and replace `Snapshot` and `snapshot`:

```rust
/// Per note: its path, whether it is indexed, its backlinks as (source, occurrences), and its
/// corpus text as a search for `[[` reads it (`None` when not indexed); then the corpus size,
/// which tells an empty text from none.
type Snapshot = (
    Vec<(String, bool, Vec<(EntryId, usize)>, Option<String>)>,
    usize,
);

fn snapshot(
    cx: &mut TestAppContext,
    notes: &Entity<NoteStore>,
    links: &Entity<IndexSync>,
) -> Snapshot {
    let ids: Vec<(EntryId, String)> = /* unchanged */;
    links.read_with(cx, |links, _| {
        let index = links.index();
        let corpus = index.corpus();
        let query = TextQuery::new("[[", false, false).expect("a query");
        // Every text here is one line of links under 60 bytes, so a hit's context is the text.
        let mut texts: HashMap<EntryId, String> = HashMap::new();
        for hit in search(&corpus, 0..corpus.len(), &query, &AtomicBool::new(false), 1_000).hits {
            texts.entry(hit.note).or_insert(hit.context);
        }
        let rows = ids
            .into_iter()
            .map(|(id, path)| {
                let backlinks = index
                    .backlinks(id)
                    .into_iter()
                    .map(|backlink| (backlink.source, backlink.links.len()))
                    .collect();
                let indexed = index.is_indexed(id);
                let text = indexed.then(|| texts.remove(&id).unwrap_or_default());
                (path, indexed, backlinks, text)
            })
            .collect();
        (rows, corpus.heap_bytes())
    })
}
```

The assertion `snapshot(links) == snapshot(fresh)` is unchanged.

- [ ] **Step 4: Run.**

Run: `cargo test -p inkra-app index::`
Expected: every test passes; 4 new in `index::corpus_tests`.

- [ ] **Step 5: Prove each test bites** (local reverts, never committed). Revert A: in `IndexSync::apply`, pass `Arc::from("")` to `insert` in place of `text` (an `apply` that stores no text). Revert B: in `Corpus::reorder` (`crates/index/src/corpus.rs`), take each text from the slot of the same position, `ids.iter().enumerate().map(|(at, _)| previous.get_mut(at).and_then(Option::take))`, in place of `order(id)` (a `set_notes` that does not reorder). Each of the four new tests and the random test must fail under at least one revert; the report lists which fail under which. Restore with `git checkout -- crates` and re-run.

- [ ] **Step 6: Commit.**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/app/src/index.rs crates/app/src/index
git commit -m "IndexSync: corpus tests across rename, trash, outside edit and a random history"
```

### Task 5.2: The startup timeline and the startup figure

**Files:** create `crates/app/src/timeline.rs`, `crates/app/src/timeline/tests.rs`, `crates/app/src/index/startup_tests.rs`; modify `crates/app/src/lib.rs`, `crates/app/src/bootstrap.rs`, `crates/app/src/index.rs`; the search spec.

**Interfaces:**
- Produces: `pub(crate) struct Timeline` (`gpui::Global`) with `pub(crate) fn new(started: Instant) -> Self`, `pub(crate) fn mark(step: Step, cx: &mut App)`, `pub(crate) fn line(&self) -> String`; `pub(crate) enum Step { Scan, FirstFrame, NamesReady, Indexed { notes, corpus_bytes, longest_turn } }`.
- `IndexSync` gains `longest_turn: Duration`, `fn turn_ended(&mut self, turn: Duration, cx)` and `fn mark_indexed(&self, cx)`.

The log line, once, in release: `launch: scan 1.2s, first frame 1.3s, names ready 1.4s, indexed 9.8s (50000 notes, corpus 156000000 bytes, longest turn 1.2ms)`. Each time is since the `Instant` taken first thing in `run()`. `bootstrap::launch` scans the vault with `block_on` **before** the window opens (`crates/app/src/bootstrap.rs:64`), so "first frame" includes the whole scan; the figure says how much. "longest turn" is the longest foreground turn `IndexSync` took until indexing ended: the build's `apply_notes` and every load's apply. It is timed around the whole `update`, which flushes the notify to every observer, so it is what a frame pays. Task 5.3's figure compares with this one.

- [ ] **Step 1: Write the failing tests.** Create `crates/app/src/timeline/tests.rs`:

```rust
use std::time::{Duration, Instant};

use super::{Step, Timeline};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn indexed(notes: usize) -> Step {
    Step::Indexed {
        notes,
        corpus_bytes: 1_234,
        longest_turn: Duration::from_micros(1_500),
    }
}

#[test]
fn each_step_keeps_its_first_time() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    assert!(!timeline.record(Step::Scan, start + ms(5)));
    assert!(!timeline.record(Step::Scan, start + ms(9)));
    assert!(!timeline.record(Step::NamesReady, start + ms(20)));
    assert_eq!(
        timeline.line(),
        "launch: scan 5.0ms, first frame -, names ready 20.0ms, indexed -"
    );
}

#[test]
fn the_line_is_due_once_when_indexing_first_ends() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    assert!(!timeline.record(Step::FirstFrame, start + ms(40)));
    assert!(timeline.record(indexed(3), start + ms(900)));
    assert!(!timeline.record(indexed(4), start + ms(950)), "a later end logs nothing");
    assert_eq!(
        timeline.line(),
        "launch: scan -, first frame 40.0ms, names ready -, \
         indexed 900.0ms (3 notes, corpus 1234 bytes, longest turn 1.5ms)"
    );
}
```

Create `crates/app/src/index/startup_tests.rs` with the wiring test (the figure test follows in Step 5):

```rust
//! The startup timeline's marks, and the startup figure (search spec, Performance, Startup).

use std::time::Instant;

use gpui::TestAppContext;

use super::tests::indexed_vault;
use crate::timeline::Timeline;

#[gpui::test]
fn indexing_marks_names_ready_then_indexed(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let (_fs, _notes, links) =
        indexed_vault(cx, &[("a.md", "alpha"), ("b.md", "[[a]]"), ("c.md", "")]);
    cx.run_until_parked();

    let bytes = links.read_with(cx, |links, _| links.index().corpus().heap_bytes());
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(!line.contains("names ready -"), "{line}");
    // The corpus of the end, not of the first build: marked when nothing is left to load.
    assert!(line.contains(&format!("(3 notes, corpus {bytes} bytes")), "{line}");
}
```

Make `indexed_vault` in `tests.rs` `pub(super)` if Task 5.1 did not.

- [ ] **Step 2: Run them to see them fail.**

Run: `cargo test -p inkra-app timeline`
Expected: compile error, `crate::timeline` does not exist.

- [ ] **Step 3: `Timeline`.** Create `crates/app/src/timeline.rs`:

```rust
//! A release-only log of when each step of a launch happened, since the process started.

use std::time::{Duration, Instant};

use gpui::{App, Global};

pub(crate) enum Step {
    /// The vault read before the window opens.
    Scan,
    FirstFrame,
    /// The first build applied: quick open lists every note.
    NamesReady,
    /// Nothing left to load: search sees every note.
    Indexed {
        notes: usize,
        corpus_bytes: usize,
        longest_turn: Duration,
    },
}

struct Indexed {
    at: Duration,
    notes: usize,
    corpus_bytes: usize,
    longest_turn: Duration,
}

pub(crate) struct Timeline {
    started: Instant,
    scan: Option<Duration>,
    first_frame: Option<Duration>,
    names_ready: Option<Duration>,
    indexed: Option<Indexed>,
}

impl Global for Timeline {}

impl Timeline {
    pub(crate) fn new(started: Instant) -> Self {
        Self {
            started,
            scan: None,
            first_frame: None,
            names_ready: None,
            indexed: None,
        }
    }

    /// Records `step` the first time it happens, and logs the line when indexing first ends.
    /// Does nothing when no launch set a timeline, as in tests.
    pub(crate) fn mark(step: Step, cx: &mut App) {
        if !cx.has_global::<Self>() {
            return;
        }
        let timeline = cx.global_mut::<Self>();
        if timeline.record(step, Instant::now()) && cfg!(not(debug_assertions)) {
            eprintln!("{}", timeline.line());
        }
    }

    /// True for the first `Indexed`: the line is due.
    fn record(&mut self, step: Step, now: Instant) -> bool {
        let at = now.saturating_duration_since(self.started);
        let slot = match step {
            Step::Scan => &mut self.scan,
            Step::FirstFrame => &mut self.first_frame,
            Step::NamesReady => &mut self.names_ready,
            Step::Indexed {
                notes,
                corpus_bytes,
                longest_turn,
            } => {
                if self.indexed.is_some() {
                    return false;
                }
                self.indexed = Some(Indexed {
                    at,
                    notes,
                    corpus_bytes,
                    longest_turn,
                });
                return true;
            }
        };
        slot.get_or_insert(at);
        false
    }

    pub(crate) fn line(&self) -> String {
        let at = |step: Option<Duration>| step.map_or_else(|| "-".to_owned(), |at| format!("{at:.1?}"));
        let indexed = self.indexed.as_ref().map_or_else(
            || "-".to_owned(),
            |indexed| {
                format!(
                    "{:.1?} ({} notes, corpus {} bytes, longest turn {:.1?})",
                    indexed.at, indexed.notes, indexed.corpus_bytes, indexed.longest_turn
                )
            },
        );
        format!(
            "launch: scan {}, first frame {}, names ready {}, indexed {indexed}",
            at(self.scan),
            at(self.first_frame),
            at(self.names_ready)
        )
    }
}

#[cfg(test)]
mod tests;
```

In `crates/app/src/lib.rs`, add `mod timeline;` after `mod theme;`.

- [ ] **Step 4: The marks.** In `crates/app/src/index.rs`:

Imports: `use std::time::{Duration, Instant};` and `use crate::timeline::{Step, Timeline};`.

Field, after `loading`:

```rust
    /// The longest foreground turn until indexing ended, for the startup timeline.
    longest_turn: Duration,
```

initialised to `Duration::ZERO` in `new`.

In `apply_notes`, right after the `self.revision += 1;` that follows `set_notes`:

```rust
        Timeline::mark(Step::NamesReady, cx);
```

In `reconcile`'s spawned task, the apply is timed around the whole `update`, since the update flushes the notify to every observer:

```rust
        cx.spawn(async move |this, cx| {
            let notes = build.await;
            let started = Instant::now();
            let _ = this.update(cx, |this, cx| this.apply_notes(generation, notes, cx));
            let turn = started.elapsed();
            let _ = this.update(cx, |this, cx| this.turn_ended(turn, cx));
        })
        .detach();
```

In `work`, the apply update is timed the same way:

```rust
            let started = Instant::now();
            let applied = this.update(cx, |this, cx| { /* unchanged */ });
            if applied.is_err() {
                return;
            }
            let turn = started.elapsed();
            if this.update(cx, |this, cx| this.turn_ended(turn, cx)).is_err() {
                return;
            }
```

New methods:

```rust
    /// Called after each foreground turn of the startup load, with how long it took.
    fn turn_ended(&mut self, turn: Duration, cx: &mut Context<Self>) {
        self.longest_turn = self.longest_turn.max(turn);
        self.mark_indexed(cx);
    }

    /// The first time nothing is left to load, for the startup timeline.
    fn mark_indexed(&self, cx: &mut Context<Self>) {
        if self.is_building() {
            return;
        }
        let corpus = self.index.corpus();
        let step = Step::Indexed {
            notes: corpus.len(),
            corpus_bytes: corpus.heap_bytes(),
            longest_turn: self.longest_turn,
        };
        Timeline::mark(step, cx);
    }
```

The last apply's `turn_ended` sees nothing left to load; so does the build's when it queues nothing (an empty vault, or nothing changed). Superseded queue entries are not in `queued`, so they never delay the mark.

At the end of the file: `#[cfg(test)] #[allow(clippy::expect_used)] mod startup_tests;`, as `tests`.

In `crates/app/src/bootstrap.rs`: imports `std::time::Instant` and `crate::timeline::{Step, Timeline}`. `run` becomes:

```rust
pub fn run() {
    let started = Instant::now();
    gpui_platform::application().run(move |cx: &mut App| {
        cx.set_global(Timeline::new(started));
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let launched = match launch() { /* unchanged */ };
        Timeline::mark(Step::Scan, cx);
        /* bounds, open_window: unchanged */
        match window {
            Ok(window) => {
                let focused = window.update(cx, |view, window, cx| {
                    window.focus(&view.focus_handle(cx), cx);
                    cx.activate(true);
                    window.on_next_frame(|_, cx| Timeline::mark(Step::FirstFrame, cx));
                });
                /* unchanged */
            }
            /* unchanged */
        }
    });
}
```

`Window::on_next_frame` runs its callback at the top of the next frame request, before that frame's draw and present (`gpui/src/window.rs:1780-1812`); `open_window` draws once without presenting (`app.rs:1363-1368`). So the mark is chained: the outer callback runs at the first frame request and registers the inner one, which runs at the second, after the first frame is presented. "first frame" is the time until the first present, plus at most one frame interval.

- [ ] **Step 5: The vault generator.** Create `crates/app/examples/startup_vault.rs`: plain Rust, no gpui, so it needs no `[[example]]` entry. The generator is `crates/index/benches/search.rs`'s (same LCG seeded 13, same `WORDS`, same `note_text`, 100 folders `f{n % 100}/n{n}.md`), copied verbatim, so the figures compare; texts are drawn in note-number order. It writes 50,000 notes to `std::env::temp_dir().join("inkra-startup-50k")` and prints the path. The hidden `.written` marker, written last, holds a `DefaultHasher` hash of the parameters (format version, count, folders, seed, words, rare word). When the marker is missing or differs, the example removes its own directory and writes the vault again, so an older layout leaves no notes behind. A test would touch the real disk, which no app test does, so the figure comes from the release app alone.

- [ ] **Step 6: Run.**

Run: `cargo test -p inkra-app -- timeline index::startup_tests`
Expected: all pass, none ignored.

Run: `cargo run -p inkra-app --release --example startup_vault`
Expected: the vault's path. The first run writes the vault (about 150 MB); a second run reuses it.

- [ ] **Step 7: Record, commit.** In the search spec, "Performance and the benches", after the **Startup** paragraph, add "Measured, slice 5, before parallel loads": machine (`sysctl -n machdep.cpu.brand_string hw.ncpu`), the generator's command, and a line "release app: taken by the orchestrator" that the orchestrator fills in.

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/app docs/superpowers/specs/2026-09-24-search-design.md
git commit -m "App: a release-only startup timeline, and the startup figure on RealFs"
```

- [ ] **Step 8: Hand the vault to the orchestrator.** The path is the one `startup_vault` prints (on macOS, `$TMPDIR/inkra-startup-50k`). The orchestrator runs `cargo run -p inkra-app --release -- "$TMPDIR/inkra-startup-50k"`, reads the `launch:` line on stderr, quits the app, and records it ("before").

### Task 5.3: Parallel startup loads

**Files:** modify `crates/app/src/index.rs`, `crates/app/src/index/tests.rs`, `crates/app/src/index/startup_tests.rs`; the search spec.

**Interfaces:** none public. `IndexSync` loses `working` and `work`; `loading: Option<EntryId>` becomes `loading: Vec<Loading>`; it gains `landed`, `apply_budget` and, under `cfg(test)`, `turns`. `next_load` returns `(EntryId, u64, PathBuf)` and holds back an entry whose note is loading. `turn_ended` and `mark_indexed` (Task 5.2) are unchanged; `drain` calls `turn_ended` after each apply turn, as `work` did.

- [ ] **Step 1: Take the test names.** Run: `./scripts/test-names.sh > /tmp/names-before.txt`.

- [ ] **Step 2: Write the failing tests.** In `crates/app/src/index/tests.rs`, replace `one_note_loads_at_a_time` with:

```rust
#[gpui::test]
fn at_most_one_load_per_core_runs_at_once(cx: &mut TestAppContext) {
    cx.executor().set_num_cpus(2);
    let fs = Arc::new(MemFs::new());
    let paths: Vec<String> = (0..5).map(|n| format!("/vault/n{n}.md")).collect();
    let mut gates = Vec::new();
    for path in &paths {
        fs.insert_file(path.as_str(), "");
        gates.push(fs.block_load(path.as_str()));
    }
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    let started = |fs: &MemFs| -> usize { paths.iter().map(|path| fs.loads(Path::new(path))).sum() };
    let progress = |cx: &mut TestAppContext| links.read_with(cx, |links, _| links.progress());

    cx.run_until_parked();
    assert_eq!(started(&fs), 2, "two cores, two loads");
    assert_eq!(progress(cx), (0, 5));

    // The queue follows vault order: n0 and n1 hold the slots.
    gates.remove(0).release();
    cx.run_until_parked();
    assert_eq!(started(&fs), 3, "n0's slot went to n2");
    assert_eq!(progress(cx), (1, 5));

    gates.clear();
    cx.run_until_parked();
    assert_eq!(started(&fs), 5, "each note loaded once");
    assert_eq!(progress(cx), (5, 5));
}

#[gpui::test]
fn a_turn_over_its_budget_yields_and_the_rest_applies_later(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> =
        (0..6).map(|n| (format!("n{n}.md"), format!("[[n{n}]]"))).collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let (_fs, _notes, links) = indexed_vault(cx, &files);
    links.update(cx, |links, _| links.apply_budget = Duration::ZERO);

    cx.run_until_parked();

    assert_eq!(links.read_with(cx, |links, _| links.turns), 6, "one load per turn");
    assert_eq!(links.read_with(cx, |links, _| links.progress()), (6, 6));
    assert!(!links.read_with(cx, |links, _| links.is_building()));
}

#[gpui::test]
fn a_note_queued_while_it_loads_waits_for_its_load(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", ""), ("b.md", "")]);
    cx.run_until_parked();
    let progress = |cx: &mut TestAppContext| links.read_with(cx, |links, _| links.progress());
    let a = Path::new("/vault/a.md");

    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "[[c]]");
    create(cx, &notes, "c");
    assert_eq!(progress(cx), (2, 3), "a is loading, c is loaded");
    let reads = fs.loads(a);

    // Free slots remain, yet a's new entry waits: one read per note at a time.
    fs.insert_file("/vault/a.md", "[[b]]");
    create(cx, &notes, "d");
    assert_eq!(fs.loads(a), reads, "no second read of a while one is in flight");
    assert_eq!(progress(cx), (3, 4), "a is loading and queued, and counts once");

    gate.release();
    cx.run_until_parked();
    assert_eq!(fs.loads(a), reads + 1, "the queued entry loads once the first lands");
    assert_eq!(progress(cx), (4, 4));
    assert_eq!(backlinks(cx, &notes, &links, "b"), ["a"]);
}
```

Add `use std::time::Duration;` beside the `Instant` import.

Tests whose expectations encode one load at a time, rewritten (their names stay; `test-names.sh` checks it):

| Test | Change |
|---|---|
| `progress_counts_the_notes_loaded` | after the first `run_until_parked`: `(2, 3)`, "a is loading, b and c are loaded" |
| `a_note_queued_twice_is_loaded_once` | first line `cx.executor().set_num_cpus(1);` — one core, so `a`'s parked load holds the queue and `b`'s first entry is superseded before it starts; the comment says so |
| `a_note_reloading_during_its_load_counts_once` | first line `cx.executor().set_num_cpus(1);`, same reason: its point is a note queued again while it loads |
| `a_failed_load_changes_the_revision` | `d` loads beside the parked `a`, so it applied before `before`: `before + 1`, "a's forget"; drop the "one at a time" comment |

In `assert_pending_are_notes`, the pending set becomes `links.queued.keys().copied().chain(links.loading.iter().filter(|load| load.live).map(|load| load.id))`, and it gains a bound read from the executor, not a literal: `assert!(links.loading.len() <= cx.background_executor().num_cpus(), "one load per core");` (the hook's `cx` is the `&mut App` of `cx.observe`). A test that calls `set_num_cpus` does so before it builds `IndexSync`.

- [ ] **Step 3: Run them to see them fail.**

Run: `cargo test -p inkra-app index::`
Expected: compile errors (`apply_budget`, `turns`, `Loading`), then, once Step 4 compiles, green.

- [ ] **Step 4: Implement.** In `crates/app/src/index.rs`:

Imports: `use std::future::poll_fn;`, `use std::task::Poll;`, `use futures::StreamExt as _;`, `use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};`, `gpui::Priority`.

Constants, after the imports:

```rust
/// Loads in flight per core; set by the startup figure (search spec, Performance).
const LOADS_PER_CORE: usize = 1;
/// How long one foreground turn applies landed loads before it yields: half the 8 ms step budget.
const APPLY_BUDGET: Duration = Duration::from_millis(4);
```

Types, after `NotesChanged`:

```rust
/// A load started and not applied yet.
struct Loading {
    id: EntryId,
    seq: u64,
    /// False once its note left the vault: not pending, though it keeps its slot until it lands.
    live: bool,
}

/// A load read and parsed off the foreground.
struct Landed {
    id: EntryId,
    seq: u64,
    loaded: inkra_vault::Result<(Source, Arc<str>)>,
}
```

Fields: drop `working` and `loading: Option<EntryId>`; add

```rust
    /// At most one per core, each counted until the foreground applies it.
    loading: Vec<Loading>,
    /// Where a finished load goes; `drain` applies it on the foreground.
    landed: UnboundedSender<Landed>,
    /// A test sets it to zero to make each turn apply one load.
    apply_budget: Duration,
    #[cfg(test)]
    turns: usize,
```

In `new`, before building `sync`:

```rust
        let (landed, arrivals) = unbounded();
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| Self::drain(this, arrivals, cx).await)
            .detach();
```

and initialise `loading: Vec::new()`, `landed`, `apply_budget: APPLY_BUDGET`, `#[cfg(test)] turns: 0`.

`progress`:

```rust
    pub(crate) fn progress(&self) -> (usize, usize) {
        let total = self.index.names().len();
        // A note loading twice, or loading and queued again, is one note pending.
        let loading: HashSet<EntryId> = self
            .loading
            .iter()
            .filter(|load| load.live && !self.queued.contains_key(&load.id))
            .map(|load| load.id)
            .collect();
        let pending = self.queued.len() + loading.len();
        (total.saturating_sub(pending), total)
    }
```

In `apply_notes`, replace the `self.loading.is_some_and(…)` block with

```rust
        // A load in flight for a note gone is not pending; it keeps its slot until it lands.
        for load in &mut self.loading {
            load.live &= live.contains(&load.id);
        }
```

and replace the `if !self.working && !self.queue.is_empty() { … }` block with `self.load_more(cx);`.

Replace `work` with `load_more`, `drain` and `apply_landed`:

```rust
    /// Starts loads until one per core is in flight, each read and parsed off the foreground.
    fn load_more(&mut self, cx: &mut Context<Self>) {
        let slots = cx.background_executor().num_cpus() * LOADS_PER_CORE;
        while self.loading.len() < slots {
            let Some((id, seq, path)) = self.next_load(cx) else {
                return;
            };
            self.loading.push(Loading { id, seq, live: true });
            let fs = Arc::clone(&self.fs);
            let landed = self.landed.clone();
            // Low, as Zed's scanner: a query's scan goes first.
            cx.background_executor()
                .spawn_with_priority(Priority::Low, async move {
                    let loaded = fs.load(&path).await.map(|loaded| {
                        let source = Source::parse(&loaded.text);
                        (source, Arc::<str>::from(loaded.text))
                    });
                    // Closed only once this entity is gone, and then nobody waits for it.
                    let _ = landed.unbounded_send(Landed { id, seq, loaded });
                })
                .detach();
        }
    }

    async fn drain(this: WeakEntity<Self>, mut arrivals: UnboundedReceiver<Landed>, cx: &mut AsyncApp) {
        while let Some(first) = arrivals.next().await {
            let started = Instant::now();
            let applied =
                this.update(cx, |this, cx| this.apply_landed(first, &mut arrivals, cx));
            if applied.is_err() {
                return;
            }
            let turn = started.elapsed();
            if this.update(cx, |this, cx| this.turn_ended(turn, cx)).is_err() {
                return;
            }
            // Loads that landed meanwhile would otherwise keep this turn going past its budget.
            yield_now().await;
        }
    }

    /// Applies landed loads until the budget is spent, at least one, then refills the slots. One
    /// note's apply is never split, so the heaviest note bounds a turn; `longest_turn` shows it.
    fn apply_landed(
        &mut self,
        first: Landed,
        arrivals: &mut UnboundedReceiver<Landed>,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        let mut next = Some(first);
        while let Some(Landed { id, seq, loaded }) = next {
            if let Some(at) = self.loading.iter().position(|load| load.id == id && load.seq == seq) {
                self.loading.swap_remove(at);
            }
            match loaded {
                Ok((source, text)) => self.apply(id, seq, source, text, cx),
                // Retried by the next reconcile; a GUI error has nowhere to go yet.
                Err(_) => self.forget(id, seq, cx),
            }
            next = if started.elapsed() < self.apply_budget {
                arrivals.try_recv().ok()
            } else {
                None
            };
        }
        #[cfg(test)]
        {
            self.turns += 1;
        }
        self.load_more(cx);
        cx.notify();
    }
```

`next_load` holds a queue entry back while its note has a load in flight, so one note never takes two slots and one read per note runs at a time (see below).

`try_recv`, not `try_next`: `try_next` is deprecated in `futures-channel` 0.3.34 (`Cargo.lock`), and clippy runs with `-D warnings`.

`next_load` drops `self.loading = Some(id);` and the `Fs` from its tuple:

```rust
    fn next_load(&mut self, cx: &Context<Self>) -> Option<(EntryId, u64, PathBuf)> {
        let vault = self.vault.as_ref()?.read(cx);
        let mut busy = Vec::new();
        let next = loop {
            let Some((id, seq)) = self.queue.pop_front() else {
                break None;
            };
            if self.queued.get(&id) != Some(&seq) {
                continue;
            }
            // One read per note at a time, so a note rewritten in a loop cannot take every slot.
            if self.loading.iter().any(|load| load.id == id) {
                busy.push((id, seq));
                continue;
            }
            self.queued.remove(&id);
            if let Some(path) = vault.absolute(id) {
                break Some((id, seq, path));
            }
        };
        for entry in busy.into_iter().rev() {
            self.queue.push_front(entry);
        }
        next
    }
```

A free function at the end of the file, before the test modules:

```rust
/// Pending once, so the task goes to the back of the foreground queue.
async fn yield_now() {
    let mut yielded = false;
    poll_fn(|task| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        task.waker().wake_by_ref();
        Poll::Pending
    })
    .await;
}
```

The struct's doc comment on the removed `working` goes; `IndexSync`'s other methods (`saved`, `forget`, `apply`) are unchanged.

- [ ] **Step 5: Run.**

Run: `cargo test -p inkra-app index::`
Expected: all pass.

Run: `./scripts/test-names.sh > /tmp/names-after.txt; diff /tmp/names-before.txt /tmp/names-after.txt`
Expected: `one_note_loads_at_a_time` gone; `at_most_one_load_per_core_runs_at_once`, `a_turn_over_its_budget_yields_and_the_rest_applies_later`, `a_note_queued_while_it_loads_waits_for_its_load` added; nothing else.

Run: `cargo test -p inkra-app random_operations` five times, then once with `ITERATIONS=1000` (read by `gpui/src/test.rs:148`).
Expected: pass.

- [ ] **Step 6: The figures, both ways.** The same vault as Task 5.2.

Run: `cargo run -p inkra-app --release --example startup_vault`
Expected: the vault's path; the vault is reused.

Release app: `cargo run -p inkra-app --release -- "$TMPDIR/inkra-startup-50k" 2>&1 | tee /tmp/launch.log` run in the background; once `launch:` appears, stop the process. Run it three times each for `LOADS_PER_CORE` 1 and 2, the second and third runs warm. Record every `launch:` line. Keep the value with the lower "indexed" time unless its "longest turn" goes over 8 ms.

- [ ] **Step 7: Record, mutants, commit.** In the search spec: the "after" rows beside Task 5.2's "before" (each release `launch:` line, `LOADS_PER_CORE` chosen and why); D5 gets a "**Built in slice 5.**" paragraph with this plan's decisions list; the Evolution row "Parallel startup loads" says "Shipped in slice 5" with the figure, and its "What changes" cell becomes "`IndexSync` keeps one load per core in flight, one read per note, and applies landed loads until 4 ms pass, at least one per turn". The "Built in slice 5" paragraph names the accepted limit: a turn applies at least one load and never splits one, so the heaviest single note's apply bounds a turn; `longest turn` in the timeline is its figure, and a turn over 8 ms on a real vault is the trigger to split `insert`.

Mutants, in a temporary detached worktree with `vendor` symlinked (handoff, "Environment traps"):

```bash
git worktree add --detach /tmp/inkra-mutants HEAD
ln -s "$PWD/vendor" /tmp/inkra-mutants/vendor
(cd /tmp/inkra-mutants && cargo mutants -p inkra-app --test-package inkra-app -f crates/app/src/index.rs -f crates/app/src/timeline.rs)
git worktree remove --force /tmp/inkra-mutants
```

Expected: zero missed. A timing mutant that no test can observe (`<` to `<=` on `started.elapsed() < self.apply_budget`) goes to `.cargo/mutants.toml` with its reason, matched by function.

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/app docs/superpowers/specs/2026-09-24-search-design.md .cargo/mutants.toml
git commit -m "IndexSync: one load per core, applied in 4 ms turns"
```

### Task 5.4: What the timeline decides

The orchestrator reads the release timeline on the generated vault, before and after Task 5.3, and records both in the search spec. If the first frame takes more than 100 ms, the "Open first, scan behind" slice moves right after slice 5, since a window that waits on the scan is the first thing a user feels.

**Decided 2026-09-25 (orchestrator).** The release app's first frame took 489–514 ms before Task 5.3 (search spec, "Startup"), so the trigger fired. "Open first, scan behind" is now slice 6 (see "Slice 6" below); the word index is slice 7, smart quick open slice 8, the persisted index slice 9, vault search slice 10.

---

## Slice 6 — Open first, scan behind (detailed)

Spec: [`../specs/2026-09-25-open-first-design.md`](../specs/2026-09-25-open-first-design.md), O1–O9, Performance, Testing. Reviewed by the user on 2026-09-25, after a sonnet red team, blue team and researcher. Branch `search/6-open-first` from `search/5-corpus-feed`.

Moved here from "After slice 8" on 2026-09-25: the release app's first frame took 489–514 ms at 50,000 notes, against the 100 ms budget. The user also asked for the rescan's cost (5.4–6.4 ms, aim 4 ms) to be fixed in this slice.

**Zed check (done, in the plan commit).** `worktree/src/worktree.rs`: `ScanState::Updated` is handled by `set_snapshot` plus an `is_scanning` flag (`:1411-1422`); `Snapshot` holds two `SumTree`s (`:184-206`); `scan_dirs` runs `num_cpus()` workers under `scoped_priority(Priority::Low, …)`, each pulling a `ScanJob` from an `async_channel` and queueing subfolder jobs (`:5276-5345`, `:5661`); `scan_dir` skips one entry whose metadata fails and logs, it does not fail the folder (`:5400-5413`); `reload_entries_for_paths` first removes a path's descendants, then re-inserts what it reads (`:5671-5760`); ids are reused by path and inode, and by inode alone only with an equal mtime or path, because an inode can be reused (`reused_entry_id`, `:3292-3325`). The spec's "What Zed does" gains the deviations these imply.

**Before (be505c2, M5 Pro, 18 cores, warm, nested vault, runs 2–4 of 4):** `launch: scan 269.4–277.2ms, first frame 487.6–496.4ms, names ready 393.8–401.7ms, indexed 1.3s (50000 notes, …, longest turn 5.5–6.4ms)`. The binary is kept at `/tmp/inkra-app-before-slice6` for the flat vault's before figure (Task 6.5). A cold figure needs `sudo purge`, which an agent cannot run: every figure of this slice is warm, and the spec says so.

**Decisions this plan makes inside the spec** (the orchestrator approved A–I on 2026-09-25; the spec records E–H as **Corrected** notes):

- **A.** `Vault::new(root)` (empty, not scanned: the first frame reads nothing), `is_scanned()`, `mark_scanned()` (O8 turns `scanned` true on a failed walk, where no `Walk` exists), `Vault: Clone`. `scan` and `rescan` mark the vault scanned; `applied` copies the flag.
- **B.** `Fs::check_dir(path)`: the readability check of O6. `RealFs` opens the folder (`std::fs::read_dir`) and lists nothing.
- **C.** The root pass needs no new API: the worker builds `Walk { read: vec![root], found: fs.read_dir(root) }`. The full pass reads the root again; the flat vault measures what that costs, and a seeded deep walk is the lever.
- **D.** O4's spawner is `pub trait Jobs` with `Serial` (tests, `scan`/`rescan`) and the app's executor. It ships in Task 6.2 only if the bench keeps the parallel walk; otherwise Task 6.2 commits the bench and the figures, and no trait.
- **E.** `reidentify`: when `id` names no live entry and `identity` names a live entry, that entry takes `id` back: the inode our write produced is our note. The replay of O2 is then correct when the held walk read the saved file, and the older race (a scan swapping in between `fs.save` and its foreground re-key) closes too.
- **F.** `applied` sorts what the walk found before it mints: fresh ids follow path order, and hard links collapse to the path that sorts first. A parallel walk returns its folders in no fixed order.
- **G.** `MemFs::read_dir` fails with `NotFound` on a folder that does not exist, as `RealFs` does, and `insert_dir` creates the parents as `insert_file` does. A folder renamed mid-walk must fail, not read as empty.
- **H.** No automatic rescan after a folder fails: a permanently unreadable folder would loop. Its kept entries wait for the next requested scan (our own create, rename, trash, conflict now; the watcher later).
- **I.** `Timeline` owns the longest foreground turn (`Timeline::turn`), so the scan worker's swap counts beside `IndexSync`'s turns.
- **A re-key during a scan only records** (plan challenge, red 1). The scan's base shares the live state, so re-keying the live vault then would copy both 50,000-entry maps on the foreground. The swap replays the re-keys onto the result, or onto the live vault when the walk failed (the base is gone by then, so nothing is copied). The base is taken in the turn that sets `running`, and the full pass's base in the root pass's swap turn, so no re-key falls between a base and its recording.
- **`Vault::collect` stays**, a wrapper returning a full walk's `found`: three sidebar tests use it with `apply`.
- **`IndexSync::is_scanning` is `vault.is_some() && built == 0`**: `reconcile` builds nothing before the full scan (O5), so this covers the scan and the one build after it. Quick open shows "Scanning the vault…" until it can list, never "No recent notes" first.
- **The worker holds no `Task`.** Its scan task is detached and ends when the store does (it holds a weak handle). Dropping a stored `Task` from inside its own final update would cancel it mid-way.
- **The old state is freed off the foreground.** A swap hands the replaced `Vault` to a background task, as `apply_notes` already does with the replaced `NoteSet`.
- **Files.** `vault.rs` would pass 400 lines with the walk: `walk` and `Jobs` go to `vault/walk.rs`. Tests: `vault/walk_tests.rs`, `vault/oracle_tests.rs` (private access, so the reference can mint `EntryId`s). The app's worker leaves `tree_ops.rs` for `vault/scans.rs`.
- **Mutants run once, in Task 6.6**, on the vault and app files the slice changed.

**Plan challenge (2026-09-25, sonnet red and blue teams).** Taken: red 1, a re-key during a scan only records (Decisions); red 2 and blue 1, `swap` and the initial scan written in full (Task 6.4 Step 3); red 3, `Step::Root` marks only an applied root pass, and a failed root pass leaves the full pass as the retry (Task 6.4 Step 3, `a_failed_root_pass_marks_no_root_step`); red 4, a compile spike of `Jobs` over `scoped_priority` before the engine (Task 6.2 Step 3); red 5, the detached task's comment says the walk finishes after the store is gone; red 6, the oracle's reference says it encodes the new rule; red tests 1, 2 and 5 (`fresh_ids_follow_path_order` across folders, a save during the initial full pass, requests during the initial scan); blue 3, O4 is decided warm, cold is Deferred; blue 6, the probe of Task 6.5 Step 1 written out. Levers, to the spec's Deferred table: blue 2 (a budget check of rescans after startup), blue 4 (ids that survive a restart, slice 9), blue 5 (the watcher needs scoped rescans first). Rejected: red test 3 (root pass fails, full pass succeeds) needs a one-shot read fault in `MemFs` for a path the both-fail test and the timeline test already cover; red test 4 (a threshold on the re-key bench) is moot, since no re-key copies a shared state any more.

### File structure

| File | Task | Status | Responsibility |
|---|---|---|---|
| `crates/vault/src/vault.rs` | 6.1, 6.4 | modify | `Vault` over `Arc<State>`; `applied`; wrappers; `reidentify` rule E |
| `crates/vault/src/vault/walk.rs` | 6.1, 6.2 | create | `Walk`, `Vault::walk`; `Jobs`, `Serial` (6.2) |
| `crates/vault/src/vault/walk_tests.rs` | 6.1, 6.2 | create | partial walks, failing folders, ids, hard links, `scanned`, parallel = serial |
| `crates/vault/src/vault/oracle_tests.rs` | 6.1 | create | proptest oracles: full walk vs today's `apply`; partial vs full |
| `crates/vault/src/vault/tests.rs` | 6.1 | modify | the flipped `reidentify` test; the race test |
| `crates/vault/src/path.rs` | 6.1 | modify | `pub(crate) fn RelPath::root()` |
| `crates/vault/src/fs.rs`, `real_fs.rs`, `mem_fs.rs`, `mem_fs/faults.rs`, `mem_fs/tests.rs` | 6.1, 6.4 | modify | read-dir gate and count, after-save gate, `NotFound`, `insert_dir` parents; `check_dir` (6.4) |
| `crates/vault/tests/real_fs.rs` | 6.4 | modify | `check_dir` on disk |
| `crates/vault/src/lib.rs` | 6.1, 6.2 | modify | re-export `Walk`; `Jobs`, `Job`, `Serial` |
| `crates/vault/benches/vault.rs`, `crates/vault/Cargo.toml` | 6.2 | create, modify | the `vault` bench |
| `crates/app/src/vault/scans.rs` | 6.3, 6.4 | create | the scan worker |
| `crates/app/src/vault/tests/scans.rs` | 6.3 | create | worker tests |
| `crates/app/src/vault/{mod.rs,store.rs,tree_ops.rs}` | 6.3, 6.4 | modify | `scans` field; `rescan` moves out; the initial scan |
| `crates/app/src/timeline.rs`, `timeline/tests.rs` | 6.3, 6.4 | modify | `turn`; steps `Root`, `Scanned` |
| `crates/app/src/index.rs`, `index/startup_tests.rs`, `index/scanning_tests.rs` | 6.3, 6.4, 6.5 | modify, create | `Timeline::turn`; no build before the scan; `is_scanning`; shared entries |
| `crates/app/src/quick_open/mod.rs`, `quick_open/tests.rs` | 6.4 | modify | "Scanning the vault…" |
| `crates/app/src/bootstrap.rs` | 6.4 | modify | open first |
| `crates/app/src/shell/tests/open_first.rs`, `shell/tests/mod.rs` | 6.4 | create, modify | launch tests, `⌘P` while scanning |
| `crates/app/src/shot/{stage.rs,scenes.rs}` | 6.4 | modify | two scenes |
| `crates/app/src/sidebar/tree.rs` | 6.5 | modify | the rebuild reads the shared entries |
| `crates/app/examples/startup_vault.rs` | 6.5 | modify | the flat layout |
| the open-first spec | plan commit, 6.2, 6.5, 6.6 | modify | Zed check, Corrected notes, figures, "Built in slice 6" |

### Task 6.1: `Vault` as a shared state; `walk` and `applied` (O1, O3)

**Files:** create `crates/vault/src/vault/walk.rs`, `vault/walk_tests.rs`, `vault/oracle_tests.rs`; modify `crates/vault/src/{vault.rs, vault/tests.rs, path.rs, lib.rs, mem_fs.rs, mem_fs/faults.rs, mem_fs/tests.rs}`, and any test that scans a root it never created (G).

**Interfaces** (all in `inkra-vault`):
- `#[derive(Clone)] pub struct Vault`; `pub fn new(root: &Path) -> Self`; `pub fn shared_entries(&self) -> Arc<[Entry]>`; `pub fn is_scanned(&self) -> bool`; `pub fn mark_scanned(&mut self)`.
- `pub struct Walk { pub read: Vec<RelPath>, pub found: Vec<DirEntry> }`; `pub async fn walk(fs: &dyn Fs, root: &Path, from: &RelPath) -> Result<Walk>`; `#[must_use] pub fn applied(&self, walk: Walk) -> Vault`.
- Unchanged signatures: `scan`, `rescan`, `collect`, `apply`, `entries`, `entry`, `id_for`, `absolute`, `reidentify`, `root`.
- `MemFs`: `block_read_dir(path) -> Gate`, `block_after_save(path) -> Gate`, `read_dirs(path) -> usize`.

- [ ] **Step 1: `MemFs` first.** In `mem_fs.rs`, `State` gains `blocked_read_dirs: BTreeMap<PathBuf, oneshot::Receiver<()>>`, `blocked_after_saves: BTreeMap<PathBuf, oneshot::Receiver<()>>` and `read_dirs: BTreeMap<PathBuf, usize>`. `read_dir` counts, parks on its gate, then answers `NotFound` for a folder not in `dirs`:

```rust
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let blocked = {
            let mut state = self.state();
            *state.read_dirs.entry(path.to_path_buf()).or_default() += 1;
            state.blocked_read_dirs.remove(path)
        };
        if let Some(wait) = blocked {
            let _ = wait.await;
        }

        let state = self.state();
        if state.unreadable.as_deref() == Some(path) {
            return Err(permission_denied(path));
        }
        // As `RealFs`: a folder renamed or removed under a walk fails, it does not read as empty.
        if !state.dirs.contains_key(path) {
            return Err(not_found(path));
        }
        // … the listing, unchanged
    }
```

`save` parks on `blocked_after_saves` after it stored the new identity, just before `Ok(Saved { … })`. `insert_dir` creates the missing parents, with the loop `insert_file` has (extract it as `fn insert_parents(state: &mut State, path: &Path)` and call it from both). In `faults.rs`, beside `block_load`:

```rust
    /// Parks the next `read_dir` of `path`, before it reads anything.
    pub fn block_read_dir(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_read_dirs.insert(path.into(), wait);
        Gate(release)
    }

    /// Parks the next `save` of `path` after the write, before it returns: the file already has its new inode.
    pub fn block_after_save(&self, path: impl Into<PathBuf>) -> Gate {
        let (release, wait) = oneshot::channel();
        self.state().blocked_after_saves.insert(path.into(), wait);
        Gate(release)
    }
```

and in `mem_fs.rs`, beside `loads`: `pub fn read_dirs(&self, path: &Path) -> usize`. `Gate`'s doc becomes "Holds one parked `MemFs` operation".

Tests in `mem_fs/tests.rs`: `read_dir_of_a_missing_folder_is_not_found`; `insert_dir_creates_its_parents` (`insert_dir("/vault/a/b")`, then `read_dir("/vault")` lists `a`); `a_read_dir_gate_parks_until_released` (as the load-gate test, with `futures::join!`); `a_save_parked_after_the_write_already_moved_the_inode` (park, poll the save once with `futures::poll!`, `load` shows the new text and a new identity, release, the save returns that identity); `read_dirs_counts_each_listing`.

Run: `cargo test -p inkra-vault mem_fs`
Expected: the new tests pass. Then `cargo test --workspace 2>&1 | grep -E "FAILED|panicked" | head`: every test that scanned a root it never created fails with `NotFound`; give it `fs.insert_dir("/vault")` (or the root it uses). Record each fixed test in the task report.

- [ ] **Step 2: Write the failing vault tests.** Create `crates/vault/src/vault/walk_tests.rs`:

```rust
use std::path::Path;

use futures::executor::block_on;

use super::*;
use crate::mem_fs::MemFs;

const ROOT: &str = "/vault";

fn rel(path: &str) -> RelPath {
    RelPath::new(Path::new(path)).expect("a relative path")
}

fn scan(fs: &MemFs) -> Vault {
    block_on(Vault::scan(fs, Path::new(ROOT))).expect("scan")
}

fn walk(fs: &MemFs, from: &str) -> Walk {
    block_on(Vault::walk(fs, Path::new(ROOT), &rel(from))).expect("walk")
}

fn id_of(vault: &Vault, path: &str) -> Option<EntryId> {
    vault.entries().find(|e| e.path.as_path() == Path::new(path)).map(|e| e.id)
}

fn paths(vault: &Vault) -> Vec<String> {
    vault.entries().map(|e| e.path.as_path().to_string_lossy().into_owned()).collect()
}

#[test]
fn a_walk_from_a_folder_reads_only_below_it() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    fs.insert_file("/vault/f/g/c.md", "");

    let walk = walk(&fs, "f");

    let mut read: Vec<_> = walk.read.iter().map(|r| r.as_path().to_path_buf()).collect();
    read.sort();
    assert_eq!(read, [Path::new("f"), Path::new("f/g")]);
    assert_eq!(walk.found.len(), 3, "b.md, g and c.md");
}

#[test]
fn a_walk_fails_when_its_start_cannot_be_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    fs.make_unreadable("/vault/f");

    assert!(block_on(Vault::walk(&fs, Path::new(ROOT), &rel("f"))).is_err());
}

#[test]
fn a_folder_that_fails_mid_walk_keeps_its_entries() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    let mut vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    fs.make_unreadable("/vault/f");
    fs.insert_file("/vault/c.md", "");
    block_on(vault.rescan(&fs)).expect("the root reads");

    assert_eq!(id_of(&vault, "f/b.md"), Some(b), "kept, under the same id");
    assert!(id_of(&vault, "c.md").is_some(), "the rest of the scan still lands");
}

#[test]
fn a_folder_renamed_mid_walk_keeps_its_notes_until_the_next_scan() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    let mut vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    // The root lists `f`; `f` is renamed before its own listing.
    let gate = fs.block_read_dir("/vault/f");
    let walked = block_on(async {
        let walk = Vault::walk(&fs, Path::new(ROOT), &rel(""));
        let rename = async {
            fs.rename(Path::new("/vault/f"), Path::new("/vault/g")).await.expect("rename");
            gate.release();
        };
        futures::join!(walk, rename).0
    })
    .expect("the root reads");
    vault = vault.applied(walked);
    assert_eq!(id_of(&vault, "f/b.md"), Some(b));

    block_on(vault.rescan(&fs)).expect("rescan");
    assert_eq!(id_of(&vault, "g/b.md"), Some(b));
    assert_eq!(id_of(&vault, "f/b.md"), None);
}

#[test]
fn a_file_the_walk_found_elsewhere_leaves_the_folder_it_did_not_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/b.md", "");
    fs.insert_dir("/vault/g");
    let vault = scan(&fs);
    let b = id_of(&vault, "f/b.md").expect("b");

    block_on(fs.rename(Path::new("/vault/f/b.md"), Path::new("/vault/g/b.md"))).expect("move");
    let vault = vault.applied(walk(&fs, "g"));

    assert_eq!(id_of(&vault, "g/b.md"), Some(b));
    assert_eq!(id_of(&vault, "f/b.md"), None, "one file, one entry");
}

#[test]
fn an_entry_goes_with_a_folder_the_walk_no_longer_found() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/b.md", "");
    let vault = scan(&fs);

    fs.remove(Path::new("/vault/f"));
    let root_only = Walk { read: vec![rel("")], found: block_on(fs.read_dir(Path::new(ROOT))).expect("root") };
    let vault = vault.applied(root_only);

    assert!(paths(&vault).is_empty(), "{:?}", paths(&vault));
}

#[test]
fn a_root_pass_lists_the_root_level_and_keeps_nothing_below() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/f/b.md", "");
    let root_only = Walk { read: vec![rel("")], found: block_on(fs.read_dir(Path::new(ROOT))).expect("root") };

    let vault = Vault::new(Path::new(ROOT)).applied(root_only);

    assert_eq!(paths(&vault), ["f", "a.md"]);
    assert!(!vault.is_scanned());
}

/// Across folders: the walk's stack reads `b` before `a`, so walk order would mint `b/y.md` first.
#[test]
fn fresh_ids_follow_path_order() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a/z.md", "");
    fs.insert_file("/vault/b/y.md", "");

    let vault = scan(&fs);

    let ids: Vec<_> = vault.entries().map(|e| e.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "a, a/z.md, b, b/y.md, minted in that order");
}

#[test]
fn a_hard_link_keeps_the_path_that_sorts_first() {
    let fs = MemFs::new();
    fs.insert_file("/vault/z/x.md", "");
    fs.insert_dir("/vault/a");
    fs.hard_link(Path::new("/vault/z/x.md"), "/vault/a/y.md");

    let notes: Vec<_> = paths(&scan(&fs)).into_iter().filter(|p| p.ends_with(".md")).collect();

    assert_eq!(notes, ["a/y.md"]);
}

#[test]
fn a_vault_is_scanned_once_a_full_scan_applied() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    let mut vault = Vault::new(Path::new(ROOT));
    assert!(!vault.is_scanned());
    assert!(!vault.applied(walk(&fs, "")).is_scanned(), "applied copies the flag");

    block_on(vault.rescan(&fs)).expect("rescan");
    assert!(vault.is_scanned());

    let mut unread = Vault::new(Path::new(ROOT));
    unread.mark_scanned();
    assert!(unread.is_scanned());
}

#[test]
fn a_clone_shares_the_entries() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "");
    let vault = scan(&fs);

    let copy = vault.clone();

    assert!(Arc::ptr_eq(&vault.shared_entries(), &copy.shared_entries()));
}
```

In `vault/tests.rs`, replace `reidentify_ignores_an_entry_a_rescan_already_replaced` with:

```rust
#[test]
fn reidentify_reclaims_the_id_for_the_inode_our_save_produced() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "before");
    let mut vault = scan(&fs);
    let ours = vault.entries().next().expect("one entry").id;

    let stamp = block_on(fs.load(Path::new("/vault/a.md"))).expect("load").stamp;
    let saved = block_on(fs.save(Path::new("/vault/a.md"), "after", stamp)).expect("save");
    // The scan read the saved file before the re-key reached the vault, so it minted a fresh id.
    block_on(vault.rescan(&fs)).expect("rescan");
    assert!(vault.entry(ours).is_none());
    vault.reidentify(ours, saved.identity);

    assert_eq!(vault.entry(ours).map(|e| e.path.display_name()).as_deref(), Some("a"));
    assert_eq!(vault.entries().count(), 1, "the fresh id is gone, not doubled");
    assert_eq!(vault.id_for(saved.identity), Some(ours));
}

#[test]
fn a_scan_between_a_save_and_its_re_key_keeps_the_note_for_the_next_save() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "one");
    let mut vault = scan(&fs);
    let ours = vault.entries().next().expect("one entry").id;
    let path = Path::new("/vault/a.md");

    let stamp = block_on(fs.load(path)).expect("load").stamp;
    let first = block_on(fs.save(path, "two", stamp)).expect("save");
    block_on(vault.rescan(&fs)).expect("rescan");
    vault.reidentify(ours, first.identity);

    let second = block_on(fs.save(path, "three", first.stamp)).expect("the next save");
    vault.reidentify(ours, second.identity);
    block_on(vault.rescan(&fs)).expect("rescan");

    assert_eq!(vault.absolute(ours).as_deref(), Some(path));
}
```

Create `crates/vault/src/vault/oracle_tests.rs`. The reference is today's `Vault::apply`, copied, with one change: it sorts its input first, which is the slice-6 rule (Corrected F). It is a second implementation of the new rule, not a record of the old one. It mints its own `EntryId`s with the same counter, so ids compare directly.

```rust
//! `walk` then `applied` against today's `apply`, and a partial walk against a full one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures::executor::block_on;
use proptest::prelude::*;

use super::*;
use crate::mem_fs::MemFs;

const ROOT: &str = "/vault";

/// Today's `apply`, over input sorted first: the slice-6 minting rule, written the old way.
struct Reference {
    entries: Vec<Entry>,
    by_identity: HashMap<FileIdentity, EntryId>,
    next_id: u32,
}

impl Reference {
    fn new() -> Self {
        Self { entries: Vec::new(), by_identity: HashMap::new(), next_id: 1 }
    }

    /// Today's `reidentify`: a no-op for an id no live entry carries.
    fn reidentify(&mut self, id: EntryId, identity: FileIdentity) {
        if !self.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        self.by_identity.retain(|_, mapped| *mapped != id);
        self.by_identity.insert(identity, id);
    }

    /// The sort is the slice-6 rule (ids in path order); the rest is today's `apply`, verbatim.
    fn apply(&mut self, mut children: Vec<DirEntry>) {
        let root = Path::new(ROOT);
        children.sort_by(|a, b| compare(&probe(root, a), &probe(root, b)));
        let mut found = Vec::new();
        let mut next_by_identity = HashMap::new();
        for child in children {
            let Some(path) = child.path.strip_prefix(root).ok().and_then(RelPath::new) else {
                continue;
            };
            if next_by_identity.contains_key(&child.identity) {
                continue;
            }
            let id = if let Some(existing) = self.by_identity.get(&child.identity) {
                *existing
            } else {
                let id = EntryId(self.next_id);
                self.next_id += 1;
                id
            };
            next_by_identity.insert(child.identity, id);
            found.push(Entry { id, path, kind: child.kind, mtime: child.mtime });
        }
        found.sort_by(compare);
        self.by_identity = next_by_identity;
        self.entries = found;
    }
}

fn probe(root: &Path, child: &DirEntry) -> Entry {
    let path = child.path.strip_prefix(root).ok().and_then(RelPath::new).unwrap_or_else(RelPath::root);
    Entry { id: EntryId::DETACHED, path, kind: child.kind, mtime: child.mtime }
}

type Rows = Vec<(EntryId, PathBuf, Kind, Mtime)>;

fn rows<'a>(entries: impl Iterator<Item = &'a Entry>) -> Rows {
    entries.map(|e| (e.id, e.path.as_path().to_path_buf(), e.kind, e.mtime)).collect()
}

#[derive(Debug, Clone)]
enum Op {
    Note { folder: usize, name: usize },
    Folder { folder: usize, name: usize },
    Remove(usize),
    Rename { pick: usize, name: usize },
    Recreate(usize),
    HardLink { pick: usize, folder: usize, name: usize },
    Save(usize),
}

const NAMES: &[&str] = &["a", "b", "Ab", "é", "z"];

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..6, 0..NAMES.len()).prop_map(|(folder, name)| Op::Note { folder, name }),
        (0usize..6, 0..NAMES.len()).prop_map(|(folder, name)| Op::Folder { folder, name }),
        (0usize..12).prop_map(Op::Remove),
        (0usize..12, 0..NAMES.len()).prop_map(|(pick, name)| Op::Rename { pick, name }),
        (0usize..12).prop_map(Op::Recreate),
        (0usize..12, 0usize..6, 0..NAMES.len())
            .prop_map(|(pick, folder, name)| Op::HardLink { pick, folder, name }),
        (0usize..12).prop_map(Op::Save),
    ]
}

/// Folders an op may target: the root, then every folder `under` holds, in vault order.
fn folders(vault: &Vault, under: &Path) -> Vec<PathBuf> {
    let mut all = vec![Path::new(ROOT).join(under)];
    all.extend(
        vault
            .entries()
            .filter(|e| e.kind == Kind::Dir && e.path.as_path().starts_with(under) && e.path.as_path() != under)
            .map(|e| Path::new(ROOT).join(e.path.as_path())),
    );
    all
}

/// Notes and folders an op may move or remove, `under` excluded.
fn picks(vault: &Vault, under: &Path) -> Vec<(EntryId, PathBuf, Kind)> {
    vault
        .entries()
        .filter(|e| e.path.as_path().starts_with(under) && e.path.as_path() != under)
        .map(|e| (e.id, Path::new(ROOT).join(e.path.as_path()), e.kind))
        .collect()
}

/// Runs `op` on `fs`, confined to `under`; a save re-keys through `rekey`.
fn run(fs: &MemFs, vault: &Vault, under: &Path, op: &Op, rekey: &mut dyn FnMut(EntryId, FileIdentity)) {
    let folders = folders(vault, under);
    let picks = picks(vault, under);
    let exists = |path: &Path| fs.read_to_string(path).is_some() || block_on(fs.read_dir(path)).is_ok();
    match *op {
        Op::Note { folder, name } => {
            let path = folders[folder % folders.len()].join(format!("{}.md", NAMES[name]));
            if !exists(&path) {
                fs.insert_file(path, "new");
            }
        }
        Op::Folder { folder, name } => {
            let path = folders[folder % folders.len()].join(NAMES[name]);
            if !exists(&path) {
                fs.insert_dir(path);
            }
        }
        Op::Remove(pick) => {
            if let Some((_, path, _)) = picks.get(pick) {
                fs.remove(path);
            }
        }
        Op::Rename { pick, name } => {
            if let Some((_, from, kind)) = picks.get(pick) {
                let file = match kind {
                    Kind::Note => format!("{}.md", NAMES[name]),
                    Kind::Dir => NAMES[name].to_owned(),
                };
                let to = from.with_file_name(file);
                let _ = block_on(fs.rename(from, &to));
            }
        }
        Op::Recreate(pick) => {
            if let Some((_, path, Kind::Note)) = picks.get(pick) {
                fs.remove(path);
                fs.insert_file(path.clone(), "again");
            }
        }
        Op::HardLink { pick, folder, name } => {
            let to = folders[folder % folders.len()].join(format!("{}.md", NAMES[name]));
            if let Some((_, from, Kind::Note)) = picks.get(pick)
                && !exists(&to)
            {
                fs.hard_link(from, to);
            }
        }
        Op::Save(pick) => {
            if let Some((id, path, Kind::Note)) = picks.get(pick)
                && let Ok(loaded) = block_on(fs.load(path))
                && let Ok(saved) = block_on(fs.save(path, "written", loaded.stamp))
            {
                rekey(*id, saved.identity);
            }
        }
    }
}

fn walk(fs: &MemFs, from: &Path) -> Walk {
    let from = RelPath::new(from).expect("relative");
    block_on(Vault::walk(fs, Path::new(ROOT), &from)).expect("the start reads")
}

proptest! {
    #[test]
    fn a_full_walk_applied_matches_todays_apply(ops in prop::collection::vec(op(), 1..30)) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        let mut reference = Reference::new();
        for op in &ops {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity| rekeys.push((id, identity)));
            for (id, identity) in rekeys {
                vault.reidentify(id, identity);
                reference.reidentify(id, identity);
            }
            let found = walk(&fs, Path::new(""));
            reference.apply(found.found.clone());
            vault = vault.applied(found);

            prop_assert_eq!(rows(vault.entries()), rows(reference.entries.iter()));
            prop_assert_eq!(vault.state.next_id, reference.next_id);
            prop_assert_eq!(&vault.state.by_identity, &reference.by_identity);
            for (at, entry) in vault.entries().enumerate() {
                prop_assert_eq!(vault.state.by_id.get(&entry.id), Some(&at));
            }
        }
    }

    #[test]
    fn a_partial_walk_matches_a_full_one(
        // No hard link in the setup: one spanning the boundary of `under` is shown by the path the walk read.
        setup in prop::collection::vec(op().prop_filter("no hard link", |op| !matches!(op, Op::HardLink { .. })), 1..20),
        pick in 0usize..8,
        inside in prop::collection::vec(op(), 1..12),
    ) {
        let fs = MemFs::new();
        fs.insert_dir(ROOT);
        let mut vault = Vault::new(Path::new(ROOT));
        for op in &setup {
            let mut rekeys = Vec::new();
            run(&fs, &vault, Path::new(""), op, &mut |id, identity| rekeys.push((id, identity)));
            for (id, identity) in rekeys {
                vault.reidentify(id, identity);
            }
            vault = vault.applied(walk(&fs, Path::new("")));
        }
        let mut dirs: Vec<PathBuf> = vec![PathBuf::new()];
        dirs.extend(vault.entries().filter(|e| e.kind == Kind::Dir).map(|e| e.path.as_path().to_path_buf()));
        let under = dirs[pick % dirs.len()].clone();

        // Every change stays below `under`: a walk from it reads all of them.
        for op in &inside {
            let mut rekeys = Vec::new();
            run(&fs, &vault, &under, op, &mut |id, identity| rekeys.push((id, identity)));
            for (id, identity) in rekeys {
                vault.reidentify(id, identity);
            }
        }
        let partial = vault.applied(walk(&fs, &under));
        let full = vault.applied(walk(&fs, Path::new("")));

        prop_assert_eq!(rows(partial.entries()), rows(full.entries()));
        prop_assert_eq!(partial.state.next_id, full.state.next_id);
        prop_assert_eq!(&partial.state.by_identity, &full.state.by_identity);
    }
}
```

Two cases the generator leaves out, by design, because a partial walk cannot see them and the spec records it (O3, Corrected F): a hard link or a rename that crosses the boundary of `under`. The setup makes no hard link, and inside `under`, `run` confines both ends: `folders` and `picks` only list paths under it.

In `vault.rs`, declare the modules:

```rust
mod walk;

pub use walk::Walk;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::expect_used))]
mod walk_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::expect_used))]
mod oracle_tests;
```

and in `lib.rs`: `pub use vault::{Entry, EntryId, Vault, Walk};`.

Run: `cargo test -p inkra-vault`
Expected: compile errors: `Walk`, `Vault::walk`, `applied`, `new`, `is_scanned`, `mark_scanned`, `shared_entries`, `RelPath::root` do not exist.

- [ ] **Step 3: `RelPath::root`.** In `path.rs`, crate-private (not public API):

```rust
    /// The vault root itself: the start of a full walk.
    pub(crate) fn root() -> Self {
        Self(Arc::from(Path::new("")))
    }
```

- [ ] **Step 4: `Vault` over one `Arc`.** In `vault.rs`:

```rust
/// Cloning is an `Arc` bump: a scan applies onto a clone off the foreground (open-first spec, O1).
#[derive(Clone)]
pub struct Vault {
    state: Arc<State>,
}

#[derive(Clone)]
struct State {
    root: PathBuf,
    entries: Arc<[Entry]>,
    by_identity: HashMap<FileIdentity, EntryId>,
    /// Position of each id's row in `entries`, rebuilt from the final, sorted list.
    by_id: HashMap<EntryId, usize>,
    next_id: u32,
    /// A full scan has applied; never turns back.
    scanned: bool,
}

impl Vault {
    /// Holds nothing until a scan applies: the first frame reads no folder.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            state: Arc::new(State {
                root: root.to_path_buf(),
                entries: Arc::from([]),
                by_identity: HashMap::new(),
                by_id: HashMap::new(),
                next_id: 1,
                scanned: false,
            }),
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.state.entries.iter()
    }

    /// The entries for a background task, without a copy.
    #[must_use]
    pub fn shared_entries(&self) -> Arc<[Entry]> {
        Arc::clone(&self.state.entries)
    }

    #[must_use]
    pub fn is_scanned(&self) -> bool {
        self.state.scanned
    }

    pub fn mark_scanned(&mut self) {
        if !self.state.scanned {
            Arc::make_mut(&mut self.state).scanned = true;
        }
    }
    // `entry`, `id_for`, `absolute`, `root`: bodies unchanged, over `self.state`.
```

`reidentify`, rule E:

```rust
    /// Re-keys an entry after a write replaced the file's inode. When a scan read the written
    /// file first and minted it a fresh id, that entry takes `id` back: the inode our write
    /// produced is our note. A no-op when neither `id` nor `identity` names a live entry.
    pub fn reidentify(&mut self, id: EntryId, identity: FileIdentity) {
        if self.state.by_id.contains_key(&id) {
            let state = Arc::make_mut(&mut self.state);
            state.by_identity.retain(|_, mapped| *mapped != id);
            state.by_identity.insert(identity, id);
            return;
        }
        let Some(&minted) = self.state.by_identity.get(&identity) else {
            return;
        };
        let state = Arc::make_mut(&mut self.state);
        let Some(at) = state.by_id.remove(&minted) else {
            return;
        };
        Arc::make_mut(&mut state.entries)[at].id = id;
        state.by_id.insert(id, at);
        state.by_identity.insert(identity, id);
    }
```

The wrappers:

```rust
    /// # Errors
    ///
    /// When the root cannot be read.
    pub async fn scan(fs: &dyn Fs, root: &Path) -> Result<Self> {
        let mut vault = Self::new(root);
        vault.rescan(fs).await?;
        Ok(vault)
    }

    /// A full scan: ids are keyed by `(device, inode)`, so a rename keeps its id and a
    /// recreated file takes a new one.
    ///
    /// # Errors
    ///
    /// When the root cannot be read; the vault is then unchanged.
    pub async fn rescan(&mut self, fs: &dyn Fs) -> Result<()> {
        let root = self.state.root.clone();
        let walk = Self::walk(fs, &root, &RelPath::root()).await?;
        *self = self.applied(walk);
        self.mark_scanned();
        Ok(())
    }

    /// Everything a full walk found under `root`.
    ///
    /// # Errors
    ///
    /// When the root cannot be read.
    pub async fn collect(fs: &dyn Fs, root: &Path) -> Result<Vec<DirEntry>> {
        Ok(Self::walk(fs, root, &RelPath::root()).await?.found)
    }

    /// `applied` with every folder of `children`, and the root, read.
    pub fn apply(&mut self, children: Vec<DirEntry>) {
        let mut read = vec![RelPath::root()];
        read.extend(
            children
                .iter()
                .filter(|child| child.kind == Kind::Dir)
                .filter_map(|child| RelPath::new(child.path.strip_prefix(&self.state.root).ok()?)),
        );
        *self = self.applied(Walk { read, found: children });
    }
```

`applied`:

```rust
    /// Replaces the children of every folder `walk` read and keeps every other entry, then drops
    /// what lost its folder. Mints and reuses ids by `(device, inode)`.
    #[must_use]
    pub fn applied(&self, walk: Walk) -> Self {
        let state = &self.state;
        let read: HashSet<&Path> = walk.read.iter().map(RelPath::as_path).collect();
        let mut found: Vec<(Entry, FileIdentity)> = walk
            .found
            .into_iter()
            .filter_map(|child| {
                let path = RelPath::new(child.path.strip_prefix(&state.root).ok()?)?;
                let entry = Entry { id: EntryId::DETACHED, path, kind: child.kind, mtime: child.mtime };
                Some((entry, child.identity))
            })
            .collect();
        // Path order, not the walk's: a parallel walk reads folders in no fixed order.
        found.sort_by(|(a, _), (b, _)| compare(a, b));

        let mut next_id = state.next_id;
        let mut claimed: HashSet<FileIdentity> = HashSet::with_capacity(found.len());
        let mut rows: Vec<(Entry, FileIdentity)> = Vec::with_capacity(found.len());
        for (mut entry, identity) in found {
            // Hard links collapse to the path that sorts first.
            if !claimed.insert(identity) {
                continue;
            }
            entry.id = state.by_identity.get(&identity).copied().unwrap_or_else(|| {
                let id = EntryId(next_id);
                next_id += 1;
                id
            });
            rows.push((entry, identity));
        }
        for (identity, id) in &state.by_identity {
            if claimed.contains(identity) {
                continue;
            }
            let Some(entry) = self.entry(*id) else {
                continue;
            };
            if entry.path.as_path().parent().is_some_and(|parent| read.contains(parent)) {
                continue;
            }
            rows.push((entry.clone(), *identity));
        }
        rows.sort_by(|(a, _), (b, _)| compare(a, b));

        // A folder sorts before its subtree, so one pass knows every surviving folder.
        let keep: Vec<bool> = {
            let mut folders: HashSet<&Path> = HashSet::new();
            rows.iter()
                .map(|(entry, _)| {
                    let path = entry.path.as_path();
                    let kept = path
                        .parent()
                        .is_none_or(|parent| parent.as_os_str().is_empty() || folders.contains(parent));
                    if kept && entry.kind == Kind::Dir {
                        folders.insert(path);
                    }
                    kept
                })
                .collect()
        };
        let mut by_identity = HashMap::with_capacity(rows.len());
        let mut entries = Vec::with_capacity(rows.len());
        for ((entry, identity), kept) in rows.into_iter().zip(keep) {
            if kept {
                by_identity.insert(identity, entry.id);
                entries.push(entry);
            }
        }
        let by_id = entries.iter().enumerate().map(|(at, entry)| (entry.id, at)).collect();
        Self {
            state: Arc::new(State {
                root: state.root.clone(),
                entries: entries.into(),
                by_identity,
                by_id,
                next_id,
                scanned: state.scanned,
            }),
        }
    }
```

- [ ] **Step 5: `walk`, serial.** Create `crates/vault/src/vault/walk.rs`:

```rust
//! Reading a vault's folders from disk (open-first spec, O3).

use std::path::Path;

use super::{MAX_DEPTH, Vault};
use crate::RelPath;
use crate::fs::{DirEntry, Fs, Kind, Result};

/// What one scan read from disk.
#[derive(Debug, Default)]
pub struct Walk {
    /// The folders whose listing it got, whole.
    pub read: Vec<RelPath>,
    /// Their children, absolute.
    pub found: Vec<DirEntry>,
}

impl Vault {
    /// Reads `from` and every folder below it, depth-limited. A folder that fails is left out of
    /// `read`, so `applied` keeps what the vault knew under it.
    ///
    /// # Errors
    ///
    /// When `from` itself cannot be read.
    pub async fn walk(fs: &dyn Fs, root: &Path, from: &RelPath) -> Result<Walk> {
        let start = root.join(from.as_path());
        let mut walk = Walk::default();
        let mut worklist = vec![(start.clone(), from.as_path().components().count())];
        while let Some((dir, depth)) = worklist.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let children = match fs.read_dir(&dir).await {
                Ok(children) => children,
                Err(error) if dir == start => return Err(error),
                Err(_) => continue,
            };
            let Some(read) = dir.strip_prefix(root).ok().and_then(RelPath::new) else {
                continue;
            };
            walk.read.push(read);
            for child in children {
                if child.kind == Kind::Dir {
                    worklist.push((child.path.clone(), depth + 1));
                }
                walk.found.push(child);
            }
        }
        Ok(walk)
    }
}
```

`MAX_DEPTH` becomes `pub(super)`... no: `walk.rs` is a child of `vault`, so it reads the private const. `vault.rs` imports `HashSet` and `Arc`.

- [ ] **Step 6: Run.**

Run: `cargo test -p inkra-vault`
Expected: every test passes; `walk_tests` 11, `oracle_tests` 2, the two new `reidentify` tests.

Run: `PROPTEST_CASES=20000 cargo test -p inkra-vault --release oracle_tests`
Expected: `test result: ok. 2 passed`. Commit any `.proptest-regressions` file it writes.

Run: `cargo test --workspace`
Expected: every test passes. The app still scans before the window and still uses `Vault::collect` and `apply` through the wrappers.

- [ ] **Step 7: Prove the oracle bites** (local reverts, never committed). Revert A: in `applied`, skip the sort of `found` (mint in walk order). Revert B: drop the kept loop (a partial walk forgets the unread folders). Revert C: in the `keep` pass, keep every row. A must fail `a_full_walk_applied_matches_todays_apply` or `fresh_ids_follow_path_order`; B must fail `a_partial_walk_matches_a_full_one` and `a_folder_that_fails_mid_walk_keeps_its_entries`; C must fail `an_entry_goes_with_a_folder_the_walk_no_longer_found`. The report lists which fail under which. Restore with `git checkout -- crates/vault/src/vault.rs`.

- [ ] **Step 8: Commit.**

```bash
cargo fmt -p inkra-vault -p inkra-app -p inkra-index
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/vault crates/app crates/index
git commit -m "Vault: a shared state, walk and applied, which replace only the folders a scan read"
```

### Task 6.2: The parallel walk and the `vault` bench (O4)

**Files:** create `crates/vault/benches/vault.rs`; modify `crates/vault/Cargo.toml`, `crates/vault/src/vault/walk.rs`, `vault/walk_tests.rs`, `vault.rs` (wrappers), `lib.rs`, the spec. The app's call sites follow in Task 6.3.

- [ ] **Step 1: The bench, serial.** In `crates/vault/Cargo.toml`, add `[lib] bench = false` (with the comment the other crates carry), `criterion = { workspace = true }` under `[dev-dependencies]`, and:

```toml
[[bench]]
name = "vault"
harness = false
```

Create `crates/vault/benches/vault.rs`. It writes two trees of 50,000 empty notes once under the temp folder (nested: 100 folders of 500; flat: all in the root), each with a `.written` marker, and times on `RealFs`:

```rust
//! `walk`, `applied` and a re-key on a shared state at 50,000 notes, on `RealFs` (open-first spec, Performance).

#![allow(clippy::expect_used)]

use std::hint::black_box;
use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_vault::{Fs, RealFs, RelPath, Vault};

const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
const WRITTEN: &str = ".written";

fn tree(layout: &str, folders: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inkra-vault-bench-{layout}"));
    if dir.join(WRITTEN).exists() {
        return dir;
    }
    let _ = std::fs::remove_dir_all(&dir);
    for note in 0..NOTES {
        let folder = if folders == 0 { dir.clone() } else { dir.join(format!("f{}", note % folders)) };
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join(format!("n{note}.md")), "").expect("note");
    }
    std::fs::write(dir.join(WRITTEN), "").expect("marker");
    dir
}

fn everything() -> RelPath {
    RelPath::new(Path::new("")).expect("the root")
}

fn walks(c: &mut Criterion) {
    for (layout, folders) in [("nested", FOLDERS), ("flat", 0)] {
        let root = tree(layout, folders);
        c.bench_function(&format!("walk serial {layout} 50k"), |b| {
            b.iter(|| block_on(Vault::walk(&RealFs, &root, &everything())).expect("walk"));
        });
    }
}

fn apply(c: &mut Criterion) {
    let root = tree("nested", FOLDERS);
    let walk = || block_on(Vault::walk(&RealFs, &root, &everything())).expect("walk");
    let empty = Vault::new(&root);
    c.bench_function("applied from empty nested 50k", |b| {
        b.iter_batched(walk, |walk| black_box(empty.applied(walk)), criterion::BatchSize::LargeInput);
    });
    let scanned = empty.applied(walk());
    c.bench_function("applied again nested 50k", |b| {
        b.iter_batched(walk, |walk| black_box(scanned.applied(walk)), criterion::BatchSize::LargeInput);
    });
    // A save outside a scan: the re-key's `retain` walks the whole identity map on the foreground.
    let note = scanned.entries().find(|e| e.path.as_path().extension().is_some()).expect("a note");
    let (id, identity) = (note.id, block_on(RealFs.load(&root.join(note.path.as_path()))).expect("load").identity);
    let mut live = scanned.applied(walk()); // its own state, shared with nothing
    c.bench_function("reidentify 50k", |b| b.iter(|| live.reidentify(id, identity)));
}

criterion_group!(benches, walks, apply);
criterion_main!(benches);
```

Run: `cargo bench -p inkra-vault -- --quick` (idle machine, after one warm-up run that writes the trees)
Expected: five figures. Record them. `reidentify 50k` is foreground time on every save outside a scan: over 1 ms, the report says so (it is today's `retain`, unchanged by this slice).

- [ ] **Step 2: The failing parallel test.** Add to `vault/walk_tests.rs`:

```rust
/// Runs each job on its own thread, as the app's executor does.
struct Threads(usize);

impl Jobs for Threads {
    fn count(&self) -> usize {
        self.0
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            std::thread::scope(|scope| {
                for job in jobs {
                    scope.spawn(move || block_on(job));
                }
            });
        })
    }
}

#[test]
fn a_parallel_walk_reads_what_a_serial_one_reads() {
    let fs = MemFs::new();
    for n in 0..200 {
        fs.insert_file(format!("/vault/f{}/g{}/n{n}.md", n % 7, n % 3), "");
    }
    let everything = rel("");
    let serial = block_on(Vault::walk(&fs, Path::new(ROOT), &everything, &Serial)).expect("walk");
    let parallel = block_on(Vault::walk(&fs, Path::new(ROOT), &everything, &Threads(8))).expect("walk");

    let base = Vault::new(Path::new(ROOT));
    let (serial, parallel) = (base.applied(serial), base.applied(parallel));
    assert_eq!(paths(&serial), paths(&parallel));
    let ids = |vault: &Vault| vault.entries().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(ids(&serial), ids(&parallel), "ids follow path order, not read order");
}

#[test]
fn a_parallel_walk_waits_for_a_folder_still_being_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/n.md", "");
    let gate = fs.block_read_dir("/vault/f");
    // Released from another thread: the jobs block this one until the walk ends.
    let walk = std::thread::scope(|scope| {
        let fs = &fs;
        scope.spawn(move || {
            while fs.read_dirs(Path::new("/vault/f")) == 0 {
                std::thread::yield_now();
            }
            gate.release();
        });
        block_on(Vault::walk(fs, Path::new(ROOT), &rel(""), &Threads(4)))
    })
    .expect("walk");

    assert_eq!(walk.found.len(), 3, "f, g and n.md: no job quit while f was being read");
}
```

Every existing `Vault::walk(fs, root, from)` call in the vault tests gains `&Serial`.

Run: `cargo test -p inkra-vault walk`
Expected: compile errors: `Jobs`, `Job`, `Serial` do not exist; `walk` takes three arguments.

- [ ] **Step 3: `Jobs` and the job queue.** First, a compile spike of the one risky shape: add `Job`, `Jobs` and `Serial` (below), then a temporary `Background` in `crates/app/src/vault/store.rs` with the `impl Jobs` of Task 6.3 and a `#[cfg(test)] fn spike(executor: gpui::BackgroundExecutor) { let _ = Background(executor); }`. Run `cargo check -p inkra-app --tests`: it must compile (gpui's `Scope::spawn` erases the jobs' lifetime, `executor.rs:486-505`). Remove the spike. If it does not compile after two attempts, stop and report: the fallback (jobs over `Arc`-owned state, `fs: Arc<dyn Fs>`) changes `walk`'s signature.

Then the queue, in `walk.rs`, as Zed's `scan_dirs`: every job takes a folder, reads it and queues its subfolders; a job with nothing to take waits while another is still reading, and they all end when the queue is empty and no job is reading.

```rust
/// One job of a walk.
pub type Job<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Runs a walk's jobs to completion: the app on its background threads, a test inline.
pub trait Jobs: Sync {
    /// How many jobs a walk starts.
    fn count(&self) -> usize;
    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a>;
}

/// One job, run inline: `scan`, `rescan` and tests.
pub struct Serial;

impl Jobs for Serial {
    fn count(&self) -> usize {
        1
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            for job in jobs {
                job.await;
            }
        })
    }
}

struct Queue {
    folders: Vec<(PathBuf, usize)>,
    /// Jobs reading a folder now: a job finding the queue empty waits for them.
    reading: usize,
    waiting: Vec<Waker>,
    walk: Walk,
    failed: Option<VaultError>,
}

struct Shared<'a> {
    fs: &'a dyn Fs,
    root: &'a Path,
    start: &'a Path,
    queue: Mutex<Queue>,
}

impl Shared<'_> {
    fn queue(&self) -> MutexGuard<'_, Queue> {
        match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The next folder to read, or `None` once no folder is queued and none is being read.
    fn next(&self) -> impl Future<Output = Option<(PathBuf, usize)>> + '_ {
        poll_fn(move |task| {
            let mut queue = self.queue();
            if let Some(folder) = queue.folders.pop() {
                queue.reading += 1;
                return Poll::Ready(Some(folder));
            }
            if queue.reading == 0 {
                return Poll::Ready(None);
            }
            queue.waiting.push(task.waker().clone());
            Poll::Pending
        })
    }

    async fn job(&self) {
        while let Some((dir, depth)) = self.next().await {
            let listed = if depth > MAX_DEPTH { None } else { Some(self.fs.read_dir(&dir).await) };
            let mut queue = self.queue();
            match listed {
                Some(Ok(children)) => {
                    if let Some(read) = dir.strip_prefix(self.root).ok().and_then(RelPath::new) {
                        queue.walk.read.push(read);
                        for child in children {
                            if child.kind == Kind::Dir {
                                queue.folders.push((child.path.clone(), depth + 1));
                            }
                            queue.walk.found.push(child);
                        }
                    }
                }
                Some(Err(error)) if dir == self.start => queue.failed = Some(error),
                _ => {}
            }
            queue.reading -= 1;
            for waker in queue.waiting.drain(..) {
                waker.wake();
            }
        }
    }
}

impl Vault {
    /// Reads `from` and every folder below it on `jobs.count()` jobs, depth-limited. A folder
    /// that fails is left out of `read`, so `applied` keeps what the vault knew under it.
    ///
    /// # Errors
    ///
    /// When `from` itself cannot be read.
    pub async fn walk(fs: &dyn Fs, root: &Path, from: &RelPath, jobs: &dyn Jobs) -> Result<Walk> {
        let start = root.join(from.as_path());
        let shared = Shared {
            fs,
            root,
            start: &start,
            queue: Mutex::new(Queue {
                folders: vec![(start.clone(), from.as_path().components().count())],
                reading: 0,
                waiting: Vec::new(),
                walk: Walk::default(),
                failed: None,
            }),
        };
        let running: Vec<Job<'_>> = (0..jobs.count().max(1)).map(|_| Box::pin(shared.job()) as Job<'_>).collect();
        jobs.run(running).await;
        let queue = match shared.queue.into_inner() {
            Ok(queue) => queue,
            Err(poisoned) => poisoned.into_inner(),
        };
        match queue.failed {
            Some(error) => Err(error),
            None => Ok(queue.walk),
        }
    }
}
```

`lib.rs`: `pub use vault::{Entry, EntryId, Job, Jobs, Serial, Vault, Walk};`. The wrappers `rescan` and `collect` pass `&Serial`. The bench gains `walk parallel {layout} 50k` with the `Threads(std::thread::available_parallelism())` runner of the test, copied into the bench.

Run: `cargo test -p inkra-vault`
Expected: every test passes.

- [ ] **Step 4: Measure and decide.**

Run: `cargo bench -p inkra-vault -- --quick` (idle machine)
Expected: serial and parallel figures, nested and flat.

The parallel walk is **kept** when `walk parallel nested` beats `walk serial nested` by at least 20 %, and `walk parallel flat` is no slower than serial by more than 10 %. Otherwise it is **dropped**: `git checkout -- crates/vault/src` (the serial walk of Task 6.1 stays, no trait), keep only the bench file and `Cargo.toml`. Either way the figures go in the spec's Performance table, with the decision.

- [ ] **Step 5: Commit.**

```bash
cargo fmt -p inkra-vault -p inkra-app -p inkra-index
git add crates/vault docs/superpowers/specs/2026-09-25-open-first-design.md
git commit -m "Vault: the parallel walk, kept by the vault bench"   # or "Vault: the vault bench; the walk stays serial"
```

`inkra-app` and `inkra-index` call `Vault::walk` nowhere yet (only the wrappers), so they compile unchanged.

### Task 6.3: The scan worker in `NoteStore` (O2)

**Files:** create `crates/app/src/vault/scans.rs`, `crates/app/src/vault/tests/scans.rs`; modify `crates/app/src/vault/{mod.rs, store.rs, tree_ops.rs, tests/mod.rs}`, `crates/app/src/timeline.rs`, `timeline/tests.rs`, `crates/app/src/index.rs`, `index/startup_tests.rs`.

**Interfaces:**
- `pub(super) struct Scans` (in `NoteStore`, field `scans`, replacing `rescan: Option<Task<()>>`).
- `NoteStore::rescan(&mut self, cx) -> oneshot::Receiver<()>` moves to `scans.rs`, same signature; `NoteStore::reidentify(&mut self, id, identity, cx)`.
- `Timeline::turn(turn: Duration, cx: &mut App)`; `Step::Indexed { notes, corpus_bytes }` (no `longest_turn`); `IndexSync::longest_turn` is removed.

- [ ] **Step 1: The failing tests.** Create `crates/app/src/vault/tests/scans.rs` and add `mod scans;` to `vault/tests/mod.rs`:

```rust
//! The scan worker (open-first spec, O2): one scan at a time, requests coalesce, re-keys survive.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Entity, TestAppContext};
use inkra_text::{Buffer, ByteOffset};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs, Kind};

use crate::test_support::{entry_id, scan_notes};
use crate::vault::NoteStore;
use crate::vault::store::AUTOSAVE_DELAY;

fn ids(notes: &Entity<NoteStore>, cx: &mut TestAppContext) -> Vec<(String, EntryId)> {
    notes.read_with(cx, |notes, cx| {
        notes
            .vault()
            .expect("a vault")
            .read(cx)
            .entries()
            .map(|e| (e.path.as_path().to_string_lossy().into_owned(), e.id))
            .collect()
    })
}

fn edit(buffer: &Entity<Buffer>, cx: &mut TestAppContext, text: &str) {
    buffer.update(cx, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.insert(ByteOffset(0), text));
        cx.notify();
    });
}

async fn opened(notes: &Entity<NoteStore>, cx: &mut TestAppContext, id: EntryId) -> Entity<Buffer> {
    let buffer = notes
        .update(cx, |notes, cx| notes.open_note(id, cx))
        .await
        .expect("the note opens");
    notes.update(cx, |notes, cx| notes.set_current(id, cx));
    buffer
}

#[gpui::test]
fn a_create_during_a_held_scan_keeps_its_id_and_every_file_keeps_one(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.insert_file("/vault/sub/b.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let before = ids(&notes, cx);

    let gate = fs.block_read_dir("/vault/sub");
    let first = notes.update(cx, |notes, cx| notes.rescan(cx));
    cx.run_until_parked();
    let created = notes.update(cx, |notes, cx| notes.create(None, Kind::Note, "c", cx));
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    let c = futures::executor::block_on(created).expect("the create lands");
    assert!(futures::executor::block_on(first).is_ok());
    let after = ids(&notes, cx);
    assert!(after.contains(&("c.md".to_owned(), c)));
    for kept in &before {
        assert!(after.contains(kept), "{kept:?} changed id");
    }
    assert_eq!(after.len(), before.len() + 1);
}

#[gpui::test]
async fn three_requests_during_a_held_scan_cost_one_more_scan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let listed = fs.read_dirs(Path::new("/vault"));

    let gate = fs.block_read_dir("/vault");
    let held = notes.update(cx, |notes, cx| notes.rescan(cx));
    cx.run_until_parked();
    let mut waiting: Vec<_> = (0..3).map(|_| notes.update(cx, |notes, cx| notes.rescan(cx))).collect();
    cx.run_until_parked();
    for wait in &mut waiting {
        assert_eq!(wait.try_recv(), Ok(None), "answered only by a scan that started after it");
    }
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.read_dirs(Path::new("/vault")), listed + 2, "the held scan, then one more");
    assert!(held.await.is_ok());
    for wait in waiting {
        assert!(wait.await.is_ok());
    }
}

#[gpui::test]
async fn a_save_re_keyed_during_a_held_scan_is_found_by_the_next_save(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;

    // The walk reads the root after the save, so it sees the new inode.
    let gate = fs.block_read_dir("/vault");
    drop(notes.update(cx, |notes, cx| notes.rescan(cx)));
    cx.run_until_parked();
    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert_eq!(ids(&notes, cx), [("a.md".to_owned(), a)]);
    edit(&buffer, cx, "2");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")).as_deref(), Some("21alpha"));
}

#[gpui::test]
async fn a_scan_that_swaps_in_between_a_save_and_its_re_key_keeps_the_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;

    let gate = fs.block_after_save("/vault/a.md");
    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    drop(notes.update(cx, |notes, cx| notes.rescan(cx)));
    cx.run_until_parked();
    assert!(ids(&notes, cx).iter().all(|(_, id)| *id != a), "the scan minted the saved file a fresh id");
    gate.release();
    cx.run_until_parked();

    assert_eq!(ids(&notes, cx), [("a.md".to_owned(), a)]);
    edit(&buffer, cx, "2");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")).as_deref(), Some("21alpha"));
}

#[gpui::test]
fn a_folder_renamed_during_a_held_walk_keeps_its_notes_until_the_next_scan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/f/n.md", "");
    let notes = scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let (f, n) = (entry_id(&notes, cx, "f"), entry_id(&notes, cx, "n"));

    let first = fs.block_read_dir("/vault/f");
    drop(notes.update(cx, |notes, cx| notes.rescan(cx)));
    cx.run_until_parked();
    let renamed = notes.update(cx, |notes, cx| notes.rename(f, "g", cx));
    cx.run_until_parked();
    // The rename's own scan is held at the root, so the first scan's result shows alone.
    let second = fs.block_read_dir("/vault");
    first.release();
    cx.run_until_parked();
    assert_eq!(ids(&notes, cx), [("f".to_owned(), f), ("f/n.md".to_owned(), n)]);

    second.release();
    cx.run_until_parked();
    assert!(futures::executor::block_on(renamed));
    assert_eq!(ids(&notes, cx), [("g".to_owned(), f), ("g/n.md".to_owned(), n)]);
}
```

In `timeline/tests.rs`, `indexed(notes)` loses `longest_turn`, and:

```rust
#[test]
fn the_longest_turn_counts_until_the_line_is_due() {
    let start = Instant::now();
    let mut timeline = Timeline::new(start);
    timeline.record_turn(Duration::from_micros(900));
    timeline.record_turn(Duration::from_micros(2_500));
    timeline.record_turn(Duration::from_micros(1_000));
    assert!(!timeline.record(Step::FirstFrame, start + ms(40)));
    assert!(timeline.record(indexed(3), start + ms(900)));
    timeline.record_turn(ms(50));
    assert!(timeline.line().ends_with("longest turn 2.5ms)"), "{}", timeline.line());
}
```

`the_line_is_due_once_when_indexing_first_ends` sets its 1.5 ms through `timeline.record_turn(Duration::from_micros(1_500))` before the marks; its expected line is unchanged.

Run: `cargo test -p inkra-app scans timeline`
Expected: compile errors: `MemFs::block_read_dir` resolves, but `Timeline::turn` does not exist and `Step::Indexed` still has `longest_turn`; the worker tests fail to build until Step 3.

- [ ] **Step 2: `Timeline::turn`.** `Timeline` gains `longest_turn: Duration`; `Indexed` loses its own. `line()` prints `self.longest_turn` in the `indexed` part.

```rust
    /// A foreground turn of the launch's work; the longest one before the line is due is logged.
    pub(crate) fn turn(turn: Duration, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.global_mut::<Self>().record_turn(turn);
        }
    }

    fn record_turn(&mut self, turn: Duration) {
        if !self.is_complete() {
            self.longest_turn = self.longest_turn.max(turn);
        }
    }
```

(`record_turn` is the testable half, as `record` is for `mark`.) In `index.rs`: remove `longest_turn`; `turn_ended` becomes `Timeline::turn(turn, cx); self.mark_indexed(cx);`; `mark_indexed` builds `Step::Indexed { notes, corpus_bytes }`. `index/startup_tests.rs` keeps passing: it checks the notes and the corpus bytes.

- [ ] **Step 3: The worker.** Create `crates/app/src/vault/scans.rs`:

```rust
//! One scan worker per store (open-first spec, O2): scans never overlap, requests made during a
//! scan coalesce into one more, and a re-key made during a scan is replayed onto its result.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use futures::channel::oneshot;
use gpui::{App, AsyncApp, BackgroundExecutor, Context, Entity, Priority, WeakEntity};
use inkra_vault::{EntryId, FileIdentity, Job, Jobs, RelPath, Result, Vault};

use super::NoteStore;
use crate::timeline::Timeline;

#[derive(Default)]
pub(super) struct Scans {
    /// Between a scan's start and its swap.
    running: bool,
    /// Answered by the next scan, which starts once the running one has swapped.
    waiting: Vec<oneshot::Sender<()>>,
    /// Re-keys since the running scan took its base, replayed at its swap.
    reidentified: Vec<(EntryId, FileIdentity)>,
}

/// The walk's jobs on the background threads, at low priority as Zed's scanner.
struct Background(BackgroundExecutor);

impl Jobs for Background {
    fn count(&self) -> usize {
        self.0.num_cpus()
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(self.0.scoped_priority(Priority::Low, move |scope| {
            for job in jobs {
                scope.spawn(job);
            }
        }))
    }
}

impl NoteStore {
    /// Resolves once a scan that started after this call has swapped in; dropped if its walk fails.
    pub(super) fn rescan(&mut self, cx: &mut Context<Self>) -> oneshot::Receiver<()> {
        let (applied, wait) = oneshot::channel();
        if self.vault.is_some() {
            self.scans.waiting.push(applied);
            if !self.scans.running {
                self.start_scan(cx);
            }
        }
        wait
    }

    /// A write moved the note's inode. During a scan the swap replays it: re-keying the live vault
    /// then would copy the maps the scan's base shares.
    pub(super) fn reidentify(&mut self, id: EntryId, identity: FileIdentity, cx: &mut Context<Self>) {
        if self.scans.running {
            self.scans.reidentified.push((id, identity));
            return;
        }
        if let Some(vault) = self.vault.clone() {
            vault.update(cx, |vault, _| vault.reidentify(id, identity));
        }
    }

    pub(super) fn start_scan(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let Some(everything) = RelPath::new(Path::new("")) else {
            return;
        };
        self.scans.running = true;
        let answers = std::mem::take(&mut self.scans.waiting);
        // Taken in the turn that sets `running`: every re-key from here on is recorded and replayed.
        let base = vault.read(cx).clone();
        let fs = Arc::clone(&self.fs);
        let jobs = Background(cx.background_executor().clone());
        // Detached: a store dropped mid-scan lets the walk finish, then its updates find nothing.
        cx.spawn(async move |this, cx| {
            let root = base.root().to_path_buf();
            let scanned = cx
                .background_spawn(async move {
                    let walk = Vault::walk(&*fs, &root, &everything, &jobs).await?;
                    Ok(base.applied(walk))
                })
                .await;
            let started = Instant::now();
            let _ = this.update(cx, |this, cx| {
                let applied = this.swap(&vault, scanned, cx);
                this.scan_ended(answers, applied, cx);
            });
            let turn = started.elapsed();
            cx.update(|cx| Timeline::turn(turn, cx));
        })
        .detach();
    }

    /// Swaps a scan's result in with the re-keys made meanwhile; `false` if the walk failed.
    fn swap(&mut self, vault: &Entity<Vault>, scanned: Result<Vault>, cx: &mut Context<Self>) -> bool {
        let reidentified = std::mem::take(&mut self.scans.reidentified);
        let mut next = match scanned {
            Ok(next) => next,
            Err(error) => {
                eprintln!("vault scan: {error}");
                // The base went with the walk, so the live state is no longer shared.
                vault.update(cx, |vault, _| {
                    for (id, identity) in reidentified {
                        vault.reidentify(id, identity);
                    }
                });
                return false;
            }
        };
        for (id, identity) in reidentified {
            next.reidentify(id, identity);
        }
        next.mark_scanned();
        let previous = vault.update(cx, |vault, cx| {
            let previous = std::mem::replace(vault, next);
            cx.notify();
            previous
        });
        // The replaced state may be the last owner of 50,000 entries: freeing them is foreground time.
        cx.background_spawn(async move { drop(previous) }).detach();
        true
    }

    fn scan_ended(&mut self, answers: Vec<oneshot::Sender<()>>, applied: bool, cx: &mut Context<Self>) {
        if applied {
            for answer in answers {
                let _ = answer.send(());
            }
        }
        self.scans.running = false;
        if !self.scans.waiting.is_empty() {
            self.start_scan(cx);
        }
    }
}
```

If Task 6.2 dropped the parallel walk, `Background` and its imports go, and the walk is `Vault::walk(&*fs, &root, &everything)`.

In `store.rs`: the field `rescan: Option<Task<()>>` becomes `pub(super) scans: Scans` (`Scans::default()` in `new`); in `write`, `if let Some(vault) = this.vault.clone() { vault.update(cx, |vault, _| vault.reidentify(id, saved.identity)); }` becomes `this.reidentify(id, saved.identity, cx);` (the comment above it stays). Drop the header imports `scans.rs` does not use. In `tree_ops.rs`, `rescan` and its comment are removed. In `vault/mod.rs`: `mod scans;` and `use scans::Scans;` where `store.rs` needs it.

Run: `cargo test -p inkra-app`
Expected: every test passes, the 5 new ones in `vault::tests::scans` included.

- [ ] **Step 4: Prove the tests bite** (local reverts). Revert A: in `rescan`, always call `start_scan` (scans overlap). Revert B: in `swap`, skip the replay loop. Revert C: `scan_ended` never restarts. Revert D: `reidentify` in `inkra-vault` goes back to today's no-op for a dead id. Expected: A fails `three_requests…` (or the create test); B fails `a_save_re_keyed_during_a_held_scan…`; C fails the create and rename tests; D fails `a_scan_that_swaps_in_between…`. Restore with `git checkout -- crates`.

- [ ] **Step 5: Commit.**

```bash
cargo fmt -p inkra-vault -p inkra-app -p inkra-index
./scripts/check.sh
git add crates/app
git commit -m "NoteStore: one scan worker; requests coalesce and re-keys survive a scan"
```

Expected: `==> all checks passed`.

### Task 6.4: Launch opens first; `scanned`; quick open while scanning (O5, O6, O8, O9)

**Files:** create `crates/app/src/shell/tests/open_first.rs`, `crates/app/src/index/scanning_tests.rs`; modify `crates/vault/src/{fs.rs, real_fs.rs, mem_fs.rs, mem_fs/tests.rs}`, `crates/vault/tests/real_fs.rs`, `crates/app/src/{bootstrap.rs, timeline.rs, timeline/tests.rs, index.rs, quick_open/mod.rs, quick_open/tests.rs, vault/scans.rs, vault/store.rs, shell/tests/mod.rs, shot/stage.rs, shot/scenes.rs}`.

**Interfaces:**
- `Fs::check_dir(&self, path: &Path) -> Result<()>`.
- `Step { FirstFrame, Root, Scanned, NamesReady, Indexed { notes, corpus_bytes } }`; the line `launch: first frame {t}, root {t}, scanned {t}, names ready {t}, indexed {t} ({n} notes, corpus {b} bytes, longest turn {t})`.
- `IndexSync::is_scanning(&self) -> bool`.
- `NoteStore::new` starts the initial scan when its vault is not scanned.

- [ ] **Step 1: `Fs::check_dir`, test first.** In `crates/vault/tests/real_fs.rs`:

```rust
#[test]
fn check_dir_accepts_a_folder_and_refuses_the_rest() {
    let dir = TempDir::new("check-dir");
    fs::write(dir.0.join("a.md"), "").expect("write");

    assert!(block_on(RealFs.check_dir(&dir.0)).is_ok());
    assert!(block_on(RealFs.check_dir(&dir.0.join("a.md"))).is_err(), "a file");
    assert!(block_on(RealFs.check_dir(&dir.0.join("missing"))).is_err());
}
```

and in `mem_fs/tests.rs`: `check_dir_refuses_a_missing_or_unreadable_folder` (a created folder passes; a missing one fails; after `make_unreadable` it fails; it counts no `read_dirs`).

`fs.rs`, in the trait after `read_dir`:

```rust
    /// Fails unless `path` is a folder this process may list; lists nothing.
    async fn check_dir(&self, path: &Path) -> Result<()>;
```

`RealFs`: `fs::read_dir(path).map(drop).map_err(|source| io_error(path, source))` (opening the folder checks it; iterating would list it). `MemFs`: `permission_denied` when unreadable, `not_found` when not in `dirs`, else `Ok(())`.

Run: `cargo test -p inkra-vault check_dir`
Expected: 2 passed.

- [ ] **Step 2: The failing app tests.** `crates/app/src/shell/tests/open_first.rs` (add `mod open_first;` to `shell/tests/mod.rs`):

```rust
//! The window opens before the vault is read (open-first spec, O6).

use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle, prelude::*};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Vault};

use crate::keymap::key_bindings;
use crate::quick_open::NOTES_CHANGED_REFRESH;
use crate::shell::AppShell;
use crate::test_support::unscanned_notes;
use crate::vault::NoteStore;

/// A shell over a vault nothing has read yet, as `bootstrap::launch` builds it.
fn unscanned(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> (WindowHandle<AppShell>, VisualTestContext, Entity<NoteStore>) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = unscanned_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let window = cx.add_window({
        let notes = notes.clone();
        move |_, cx| AppShell::new(None, notes, cx)
    });
    window
        .update(cx, |shell, window, cx| window.focus(&shell.focus_handle(cx), cx))
        .expect("the shell is open");
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual, notes)
}

fn rows(window: WindowHandle<AppShell>, visual: &mut VisualTestContext) -> Vec<String> {
    window
        .update(visual, |shell, _, cx| shell.tree_row_names(cx))
        .expect("the shell is open")
}

fn vault(files: &[&str]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for file in files {
        fs.insert_file(format!("/vault/{file}"), "");
    }
    fs
}

#[gpui::test]
fn the_window_draws_empty_then_the_root_level_then_the_rest(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md", "f/g/c.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual, _) = unscanned(cx, &fs);

    assert_eq!(rows(window, &mut visual), Vec::<String>::new(), "nothing read before the first frame");
    visual.run_until_parked();
    assert_eq!(rows(window, &mut visual), ["f", "a"], "the root level, folders closed");

    gate.release();
    visual.run_until_parked();
    assert_eq!(rows(window, &mut visual), ["f", "a"], "nothing moves: f stays closed");
    window
        .update(&mut visual, |shell, _, cx| shell.toggle_tree_row("f", cx))
        .expect("the shell is open");
    assert_eq!(rows(window, &mut visual), ["f", "g", "b", "a"]);
}

#[gpui::test]
fn a_folder_expanded_during_the_scan_fills_in_when_the_scan_applies(cx: &mut TestAppContext) {
    let fs = vault(&["f/b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual, _) = unscanned(cx, &fs);
    visual.run_until_parked();

    window
        .update(&mut visual, |shell, _, cx| shell.toggle_tree_row("f", cx))
        .expect("the shell is open");
    assert_eq!(rows(window, &mut visual), ["f"]);
    gate.release();
    visual.run_until_parked();

    assert_eq!(rows(window, &mut visual), ["f", "b"]);
}

#[gpui::test]
fn cmd_p_during_the_scan_says_so_then_lists(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "f/b.md"]);
    let gate = fs.block_read_dir("/vault/f");
    let (window, mut visual, _) = unscanned(cx, &fs);
    visual.run_until_parked();

    // An empty query lists recent notes only; a query lists matches.
    visual.simulate_keystrokes("cmd-p b");
    visual.run_until_parked();
    let shown = |visual: &mut VisualTestContext| {
        window
            .update(visual, |shell, _, cx| shell.quick_open_state(cx))
            .expect("the shell is open")
            .expect("quick open is open")
    };
    assert_eq!(shown(&mut visual), (Some("Scanning the vault…"), 0));

    gate.release();
    visual.run_until_parked();
    visual.executor().advance_clock(NOTES_CHANGED_REFRESH);
    visual.run_until_parked();
    assert_eq!(shown(&mut visual).1, 1, "f/b, after one build");
}

#[gpui::test]
fn an_unreadable_root_during_the_scan_releases_quick_open(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    fs.make_unreadable("/vault");
    let (window, mut visual, _) = unscanned(cx, &fs);
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-p");
    visual.executor().advance_clock(NOTES_CHANGED_REFRESH);
    visual.run_until_parked();
    let shown = window
        .update(&mut visual, |shell, _, cx| shell.quick_open_state(cx))
        .expect("the shell is open")
        .expect("quick open is open");

    assert_eq!(shown, (Some("No recent notes"), 0), "never waits forever");
}
```

The three `AppShell` helpers are `#[cfg(test)] pub(crate)` in `shell/tests/mod.rs` (a child of `shell`): `tree_row_names` (the sidebar's visible row names, in order), `toggle_tree_row(name)` (finds the row by name and calls the sidebar's `toggle`, through a `#[cfg(test)] pub(crate) fn toggle_named` on `Sidebar` in `sidebar/tests.rs`), `quick_open_state` (the open picker's `delegate.empty_text()` and `match_count()`). An `Explore` agent first checks `shell/tests/mod.rs` and `sidebar/tests.rs` for helpers that already do this; reuse them if so. `NOTES_CHANGED_REFRESH` becomes `pub(crate)` under `#[cfg(test)]` re-export, or the test uses `Duration::from_millis(200)` with a comment naming the constant; prefer the re-export.

`crates/app/src/index/scanning_tests.rs` (add the module to `index.rs` as the others):

```rust
//! No build before the vault's first full scan, and one after it (open-first spec, O5).

use std::path::Path;
use std::sync::Arc;

use gpui::{TestAppContext, prelude::*};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Vault};

use super::IndexSync;
use crate::vault::NoteStore;

#[gpui::test]
fn the_startup_load_runs_one_build(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let notes = unscanned_notes(cx, fs as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    assert!(links.read_with(cx, |links, _| links.is_scanning()));

    cx.run_until_parked();

    links.read_with(cx, |links, _| {
        assert_eq!(links.generation, 1, "the root pass and the full pass, one build");
        assert!(!links.is_scanning());
        assert_eq!(links.index().names().len(), 2);
    });
}
```

`quick_open/tests.rs`: `the_picker_says_scanning_until_the_first_build` builds `IndexSync` over an unscanned vault (as above), opens the picker without `run_until_parked` in between, types `b`, asserts `empty_text() == Some("Scanning the vault…")` and no matches, then `run_until_parked` + `advance_clock(NOTES_CHANGED_REFRESH)` + `run_until_parked` and asserts the matches.

`timeline/tests.rs`: `each_step_keeps_its_first_time` and `the_line_is_due_once…` use `Step::Root` and `Step::Scanned` in place of `Step::Scan`, with the expected line `"launch: first frame -, root 5.0ms, scanned -, names ready 20.0ms, indexed -"` (and the second one's line in the same order).

Run: `cargo test -p inkra-app open_first scanning timeline quick_open`
Expected: compile errors (`is_scanning`, `Step::Root`, the helpers), then, once they compile, failures: the root level never shows alone, and `⌘P` lists at once.

- [ ] **Step 3: The initial scan.** In `scans.rs`, `#[derive(Clone, Copy, PartialEq, Eq)] enum Pass { Root, Full }`. The spawned task of `start_scan` runs the root pass first while its base is not scanned; the full pass's base is taken in the root pass's swap turn:

```rust
        cx.spawn(async move |this, cx| {
            let root = base.root().to_path_buf();
            let mut base = base;
            if !base.is_scanned() {
                let (fs, root, read) = (Arc::clone(&fs), root.clone(), everything.clone());
                let listed = cx
                    .background_spawn(async move {
                        let found = fs.read_dir(&root).await?;
                        Ok(base.applied(Walk { read: vec![read], found }))
                    })
                    .await;
                let started = Instant::now();
                // One turn: no re-key falls between the root pass's swap and the full pass's base.
                let Ok((applied, next)) = this.update(cx, |this, cx| {
                    (this.swap(&vault, listed, Pass::Root, cx), vault.read(cx).clone())
                }) else {
                    return;
                };
                base = next;
                let turn = started.elapsed();
                cx.update(|cx| {
                    Timeline::turn(turn, cx);
                    if applied {
                        Timeline::mark(Step::Root, cx);
                    }
                });
            }
            let scanned = cx
                .background_spawn(async move {
                    let walk = Vault::walk(&*fs, &root, &everything, &jobs).await?;
                    Ok(base.applied(walk))
                })
                .await;
            let started = Instant::now();
            let _ = this.update(cx, |this, cx| {
                let applied = this.swap(&vault, scanned, Pass::Full, cx);
                this.scan_ended(answers, applied, cx);
            });
            let turn = started.elapsed();
            cx.update(|cx| {
                Timeline::turn(turn, cx);
                if vault.read(cx).is_scanned() {
                    Timeline::mark(Step::Scanned, cx);
                }
            });
        })
        .detach();
```

A failed root pass is logged and the full pass runs anyway: it is the retry, and a root that stays unreadable fails it too. `swap` in full:

```rust
    /// Swaps a scan's result in with the re-keys made meanwhile; `false` if the walk failed.
    fn swap(&mut self, vault: &Entity<Vault>, scanned: Result<Vault>, pass: Pass, cx: &mut Context<Self>) -> bool {
        let reidentified = std::mem::take(&mut self.scans.reidentified);
        let mut next = match scanned {
            Ok(next) => next,
            Err(error) => {
                eprintln!("vault scan: {error}");
                vault.update(cx, |vault, cx| {
                    // The base went with the walk, so the live state is no longer shared.
                    for (id, identity) in reidentified {
                        vault.reidentify(id, identity);
                    }
                    // O8: the index builds on what is known, so quick open never waits forever.
                    if pass == Pass::Full && !vault.is_scanned() {
                        vault.mark_scanned();
                        cx.notify();
                    }
                });
                return false;
            }
        };
        for (id, identity) in reidentified {
            next.reidentify(id, identity);
        }
        if pass == Pass::Full {
            next.mark_scanned();
        }
        let previous = vault.update(cx, |vault, cx| {
            let previous = std::mem::replace(vault, next);
            cx.notify();
            previous
        });
        // The replaced state may be the last owner of 50,000 entries: freeing them is foreground time.
        cx.background_spawn(async move { drop(previous) }).detach();
        true
    }
```

In `store.rs`, `NoteStore::new` ends with:

```rust
        let mut store = Self { /* … */ };
        if store.vault.as_ref().is_some_and(|vault| !vault.read(cx).is_scanned()) {
            store.start_scan(cx);
        }
        store
```

`crate::test_support` gains the helper the new tests share (`index/scanning_tests.rs` and `shell/tests/open_first.rs` use it):

```rust
/// A store over a vault nothing has read yet: it scans the root, then the rest, as a launch does.
pub(crate) fn unscanned_notes(cx: &mut TestAppContext, fs: Arc<dyn Fs>) -> Entity<NoteStore> {
    cx.update(|cx| {
        let vault = cx.new(|_| Vault::new(Path::new("/vault")));
        cx.new(|cx| NoteStore::new(fs, Some(vault), cx))
    })
}
```

Three worker tests join `vault/tests/scans.rs` (plan challenge, red):

```rust
#[gpui::test]
async fn a_save_during_the_initial_full_pass_keeps_its_note(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let buffer = opened(&notes, cx, a).await;

    edit(&buffer, cx, "1");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    gate.release();
    cx.run_until_parked();

    assert!(ids(&notes, cx).contains(&("a.md".to_owned(), a)));
    edit(&buffer, cx, "2");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(fs.read_to_string(Path::new("/vault/a.md")).as_deref(), Some("21alpha"));
}

#[gpui::test]
async fn requests_during_the_initial_scan_cost_one_more_scan(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/f/b.md", "");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    assert_eq!(fs.read_dirs(Path::new("/vault")), 2, "the root pass, then the full pass");

    let waiting: Vec<_> = (0..3).map(|_| notes.update(cx, |notes, cx| notes.rescan(cx))).collect();
    gate.release();
    cx.run_until_parked();

    assert_eq!(fs.read_dirs(Path::new("/vault")), 3, "one more full scan, no second root pass");
    for wait in waiting {
        assert!(wait.await.is_ok());
    }
}

#[gpui::test]
fn a_failed_root_pass_marks_no_root_step(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "");
    fs.make_unreadable("/vault");
    let _notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();

    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("root -"), "{line}");
    assert!(!line.contains("scanned -"), "O8: the failed full pass still turns scanned: {line}");
}
```

- [ ] **Step 4: `IndexSync`.** In `reconcile`, after `let Some(vault)`:

```rust
        // One build, after the full scan: the root pass alone would cost a build the next swap discards.
        if !vault.read(cx).is_scanned() {
            return;
        }
```

and:

```rust
    /// Until the first build after the vault's full scan applies, there are no names to list.
    pub(crate) fn is_scanning(&self) -> bool {
        self.vault.is_some() && self.built == 0
    }
```

- [ ] **Step 5: Quick open.** In `quick_open/mod.rs`: `const SCANNING: &str = "Scanning the vault…";`, a field `scanning: bool` (false in `new`), and in `empty_text`, first: `if self.scanning { return Some(SCANNING); }`. In `update_matches`, right after `let run = …` and before `let wait`:

```rust
        if self.links.read(cx).is_scanning() {
            // Nothing to list yet; `NotesChanged` re-runs the query once the first build applies.
            self.scanning = true;
            self.matches.clear();
            return cx.spawn(async move |picker, cx| {
                let _ = picker.update(cx, |picker, cx| {
                    if picker.delegate.search_id == id {
                        picker.matches_updated(0, cx);
                    }
                });
            });
        }
```

and in `apply`, beside `this.searched = true;`: `this.scanning = false;`. The picker's `NotesChanged` subscription is unchanged.

- [ ] **Step 6: Launch and the timeline.** `timeline.rs`: `Step::Scan` becomes `Step::Root` ("the root level applied: the sidebar shows it") and `Step::Scanned` ("the first full scan applied"); `Timeline` fields `root`, `scanned` replace `scan`; `line()` prints `launch: first frame {}, root {}, scanned {}, names ready {}, indexed {}`. `bootstrap.rs`:

```rust
    if path.is_dir() {
        // Opens the folder, lists nothing: a vault of any size costs the first frame nothing (open-first spec, O6).
        futures::executor::block_on(RealFs.check_dir(&path)).map_err(|error| format!("{error}"))?;
        return Ok(Launch::Vault(Vault::new(&path)));
    }
```

`run()` drops `Timeline::mark(Step::Scan, cx)`; `launch()`'s doc keeps "a failure should exit rather than flash an empty window". `Launch::Vault`'s doc: "not scanned yet: the store scans it behind the first frame".

- [ ] **Step 7: Shots.** `Stage` gains `gates: Vec<Gate>` (released on drop, after the shot) and:

```rust
    /// A vault the store scans behind the first frame; the walk is held at `held`, so the shot shows the root level only.
    pub(crate) fn open_unscanned_vault(&mut self, files: &[(&str, &str)], held: &str) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        self.gates.push(fs.block_read_dir(PathBuf::from(VAULT_ROOT).join(held)));
        self.open(fs, Launch::Vault(Vault::new(Path::new(VAULT_ROOT))))
    }
```

Two scenes in `scenes.rs`, in the catalogue after `sidebar-dirty` and `quick-open-graphemes`: `sidebar-root-only` (three root notes and two folders, held at one folder, focused) and `quick-open-scanning` (the same, then `⌘P`).

Run: `cargo run -p inkra-app --features shot --example shot -- sidebar-root-only` and `… quick-open-scanning`
Expected: `target/shots/sidebar-root-only.png` and `target/shots/quick-open-scanning.png`. Report the paths; do not open them.

- [ ] **Step 8: Run and commit.**

Run: `cargo test -p inkra-app`
Expected: every test passes; `shell::tests::open_first` 4, `index::scanning_tests` 1, the new quick-open and timeline tests.

```bash
cargo fmt -p inkra-vault -p inkra-app -p inkra-index
./scripts/check.sh
git add crates/vault crates/app
git commit -m "Launch: the window opens before the vault is read; quick open says it is scanning"
```

### Task 6.5: `reconcile` shares the entries (O7), and the figures

**Files:** modify `crates/app/src/index.rs`, `crates/app/src/sidebar/tree.rs`, `crates/app/examples/startup_vault.rs`, the spec.

- [ ] **Step 1: Measure before moving anything.** Build release, write both vaults, take the timeline on each (Step 4's commands). Then add this local, uncommitted probe, one line per turn, release only:

```rust
// crates/app/src/timeline.rs, beside `turn` (probe, reverted before the commit)
pub(crate) fn probe(name: &str, started: Instant) {
    if cfg!(not(debug_assertions)) {
        eprintln!("probe: {name} {:?}", started.elapsed());
    }
}
```

and `let started = Instant::now();` first, `crate::timeline::probe("<name>", started);` last, in: `IndexSync::reconcile` (`reconcile`), `IndexSync::apply_notes` (`apply_notes`), `Sidebar::rebuild` (`rebuild`), and around the full pass's `this.update(…swap…)` in `scans.rs` (`swap`, which includes the observers the notify runs). Run the app once on the nested vault as in Step 4, keeping all of stderr:

```bash
./target/release/inkra-app "$NESTED" 2> /tmp/probe.log & pid=$!; until grep -q '^launch:' /tmp/probe.log; do sleep 0.1; done; kill $pid
grep '^probe:' /tmp/probe.log | sort -k2,2 -k3,3r
```

Expected: `probe: <name> <duration>` lines. The report gives the longest per name and names the turn that makes the `longest turn` of the `launch:` line. `git checkout -- crates` removes the probe.

`startup_vault.rs` gains a layout argument: `cargo run -p inkra-app --release --example startup_vault -- flat` writes 50,000 notes in the root of `inkra-startup-50k-flat` (the layout joins `parameters()`, so each layout has its own marker); no argument keeps today's nested vault.

- [ ] **Step 2: `reconcile` and `apply_notes`.** A build now derives everything `apply_notes` needs:

```rust
/// A build of the vault's notes, with the inputs of `apply_notes`'s diff, derived off the foreground.
struct Built {
    notes: NoteSet,
    /// Every note's id and mtime, in vault order.
    mtimes: Vec<(EntryId, Mtime)>,
    live: HashSet<EntryId>,
}

impl Built {
    fn new(entries: &[Entry]) -> Self {
        let named = entries
            .iter()
            .map(|entry| (entry.id, entry.path.as_path().to_string_lossy().into_owned(), entry.kind))
            .collect();
        let mtimes: Vec<(EntryId, Mtime)> = entries
            .iter()
            .filter(|entry| entry.kind == Kind::Note)
            .map(|entry| (entry.id, entry.mtime))
            .collect();
        let live = mtimes.iter().map(|(id, _)| *id).collect();
        Self { notes: NoteSet::build(named), mtimes, live }
    }
}
```

`reconcile` becomes: the `is_scanned` guard, `let entries = vault.read(cx).shared_entries();`, `cx.background_spawn(async move { Built::new(&entries) })`. `apply_notes(generation, built, cx)` uses `built.live` and `built.mtimes` in place of reading the vault, and sends `built.live` (and the stale build, as today) to a background drop. It no longer reads the vault; the build's entries are the vault's at `reconcile`, and a newer vault always starts a newer generation.

- [ ] **Step 3: The sidebar.** `rebuild` reads `vault.read(cx).shared_entries()` in place of `entries().cloned().collect()`: 50,000 clones less per swap.

Run: `cargo test -p inkra-app`
Expected: every test passes, unchanged.

- [ ] **Step 4: The figures.** On an idle machine (no `cargo` running):

```bash
cargo build -p inkra-app --release
NESTED=$(cargo run -q -p inkra-app --release --example startup_vault)
FLAT=$(cargo run -q -p inkra-app --release --example startup_vault -- flat)
/tmp/launch-figure.sh ./target/release/inkra-app "$NESTED" 4
/tmp/launch-figure.sh ./target/release/inkra-app "$FLAT" 4
/tmp/launch-figure.sh /tmp/inkra-app-before-slice6 "$FLAT" 4
```

`/tmp/launch-figure.sh <binary> <vault> <runs>` runs the binary in the background, waits for its `launch:` line on stderr, kills it, and prints the line; run 1 warms the cache and is reported apart. Expected: `first frame` under 100 ms on both; `longest turn` at 4 ms or under, or the report names the turn over it and why.

- [ ] **Step 5: Commit.**

```bash
cargo fmt -p inkra-app
./scripts/check.sh
git add crates/app docs/superpowers/specs/2026-09-25-open-first-design.md
git commit -m "IndexSync: builds from the shared entries and derives the diff off the foreground"
```

### Task 6.6: Figures, spec, mutants

- [ ] **Step 1: The spec.** Fill the Performance table (bench and timeline figures, each with its command, machine and cache state), add "Built in slice 6" after Decisions (what shipped, what the figures decided, the parallel walk kept or dropped and why), and add the open-first spec to `CLAUDE.md`'s Docs list.
- [ ] **Step 2: Mutants.** In a temporary detached worktree with `vendor` symlinked, one run at a time:

```bash
git worktree add --detach /tmp/inkra-mutants HEAD
ln -s "$PWD/vendor" /tmp/inkra-mutants/vendor
cd /tmp/inkra-mutants
cargo mutants --list -p inkra-vault -f crates/vault/src/vault.rs | head   # does examine_globs, which lists no vault file, hide it?
```

If the list is empty, run the vault files with `--no-config --gitignore false -p inkra-vault --test-package inkra-vault -f <file>`; otherwise keep the config. Vault files: `vault.rs`, `vault/walk.rs`, `real_fs.rs` (`check_dir` only, `--re check_dir`). App files, each `cargo mutants -p inkra-app --test-package inkra-app -f <file>`: `vault/scans.rs`, `index.rs`, `timeline.rs`, `quick_open/mod.rs`, `sidebar/tree.rs`. Zero missed; an equivalent mutant goes to `.cargo/mutants.toml` with its reason. Then `cd -` and `git worktree remove --force /tmp/inkra-mutants`.
- [ ] **Step 3: Commit.**

```bash
./scripts/check.sh
git add docs CLAUDE.md .cargo/mutants.toml crates
git commit -m "Open first: figures, built in slice 6, mutants"
```

---

## Slice 7 — The word index (detailed)

Spec: smart quick open S1 (words and folding), S2 (the word index), Performance (index build, re-index, memory), Testing (oracle, updates). Branch `search/7-word-index` from `search/6-open-first`.

**Zed check.** Zed has no word index: project search rereads files, and the file finder matches subsequences. Read `language/src/buffer.rs`, `CharClassifier` (around line 6166), for Zed's word rule, and `fuzzy/src/char_bag.rs` for its prefilter. Record in the smart quick open spec, under a new "What Zed does" paragraph: S1 uses `regex_syntax::is_word_character`, not `CharClassifier`, since it must agree with D4's whole-word rule and a note has no language scope.

**Checked against the code on 2026-09-26 (slice lead, Flow step 2).** Slices 5 and 6 moved the load path; the tasks below are corrected in place. What changed:

- Loads run in `IndexSync::load_more` (`crates/app/src/index/loading.rs`), each a `Priority::Low` background task that sends a `Landed` to `drain`; `apply_landed` applies them in 4 ms turns. `NoteContent::parse` runs in that task, after `fs.load`.
- `saved` wraps its apply in `Self::timed_turn`, so the timeline counts it; the plan's `this.update` would drop that.
- Call sites of the old `insert` the plan missed: `benches/names.rs` (four), `benches/search.rs:92`, `crates/app/src/index/loading_tests.rs:112`, and `sync.apply(…, Source::parse(..), text, cx)` in `crates/app/src/index/tests.rs` (three).
- `crates/app/src/index/tests.rs` is 961 lines and has no `edit` helper (it is `saved_edit`). The app's word tests go to a new `crates/app/src/index/words_tests.rs`.
- The updates oracle's `model` already holds exactly the notes `NoteIndex` indexes (`Load` records a text only for a note of the current table), so it needs no `is_indexed` filter.
- The startup figure is taken with `/tmp/launch-figure.sh` on the nested and flat 50,000-note vaults; the "before" binary is slice 6's. The budget compares their `indexed` figures (1.1 s nested, 1.2–1.3 s flat at slice 6's gate).
- The launch line reports the corpus's bytes, not the word index's: `Step::Indexed` gains `words_bytes`, so the app gives the real-vault memory figure beside the bench's.

### Plan challenge, 2026-09-26

Red and blue teams (sonnet, read-only) on this section. Each point, taken or rejected:

| # | Point | Answer |
|---|---|---|
| R1 | The first layout duplicates the vocabulary per shard: ~625 MB on the Zipf corpus, ~55 MB on the startup vault | Taken: the F store (Task 7.3), decided by the orchestrator |
| R2 | `set_notes` with 5,500 gone while a snapshot is held copies every shard | Taken: F makes it O(gone) plus pointer copies; bench case `set_notes/5,500 gone, a snapshot held` |
| R3 | A 1 MB base64 paste gives ~40,000 distinct words in one foreground insert | Taken: a base64 test (Task 7.2) and bench case; under F the insert is a move, the cost moves to that shard's next merge, which the merge max figure shows |
| R4 | `ς`/`σ` never match under `to_lowercase` | Taken: fold by simple case folding (decision above, orchestrator) |
| R5 | The foreground `insert` may eat the 20 % startup budget | Taken: F's insert is O(1) plus bounded merges; `build` prints per-insert p50/p99/max, with a 40-word case beside the Zipf one |
| R6 | `lookup` is one range per shard, not "one ordered range" | Taken: S2's text says what is built (Task 7.7 Step 7) |
| R7 | `heap_bytes` undercounts; gate on RSS | Taken: the alarm reads the RSS delta; `heap_bytes` is recorded beside it |
| R8 | A same-words re-save still copies a shard under a snapshot | Rejected: under F the copy is the shard's small `fresh` map and `dead` set (pointers); comparing words would need each note's words kept, which F drops for frozen notes |
| R9 | Pins: `ſ`, keycaps, many words with a snapshot held, oracle alphabet | Taken: fold tests, the base64 bench with a snapshot held, `ς σ Σ ſ s` and a keycap in the oracle's alphabet. Rejected: a cost assertion in the app oracle; budgets are benches and the launch line, tests assert state (CLAUDE.md) |
| B1 | S1 still says "fold, then split" | Taken: Step 7 edits S1's sentence |
| B2 | Same as R6 | Taken |
| B3 | The lego rule omits `find`'s `CorpusSnapshot` | Taken: the rule names it |
| B4 | Visibility is scoped right | Noted |
| B5 | `words::words` stutters | Taken: the split is `word_spans` |
| B6 | No bench holds both snapshots | Taken: `insert/one note, both snapshots held` |
| B7 | The startup figure is sound | Noted |
| B8 | Compare RSS, not the estimate | Taken, as R7 |
| B9 | An empty lookup means empty or cancelled | Taken: a note in Task 8.2 |
| B10 | The excerpt has no single-note text read | Taken: a note in Task 8.3 |
| B11 | Mtimes must not be `Option` | Taken: a note in Task 8.1 |
| B12 | `EntryId` is minted per scan: a persisted index cannot key by it | Taken: the Evolution row gains the seam (Step 7) |
| B13 | `TermHit`'s public fields allow `count: 0` | Rejected: plain public result structs are this crate's shape (`Hit`, `NameMatch`); only `lookup` builds one |
| B14 | `WordsSnapshot::default()` has 0 shards, `Words::default()` 256 | Taken: `WordsSnapshot` has no `Default` |
| B15 | Capped words merge, as intended | Noted |

### Decisions this plan makes inside the spec

Record these in the smart quick open spec, under S1 and S2, in Task 7.7, one short paragraph each ("Built in slice 7").

- **Split, then fold, on both sides.** S1 folds the query, then splits it. The plan splits the query into words by the same `word_spans` function as the text, then folds each word. So a word copied from a note always finds that note, whatever a fold does at a word edge.
- **Terms and stored words are capped in folded chars.** A term keeps its first 64 folded chars (`TERM_MAX_CHARS`). A stored word keeps its first 65 (`WORD_MAX_CHARS`). Prefix and whole answers are unchanged: a term matches a prefix within its 64 chars, and a 65-char stored word can never equal a term. A 1 MB base64 word costs 65 chars. The typed query cap of S1, 64 chars, stays; folding can still lengthen a term (`한` → three jamo), which is why the term cap is in folded chars.
- **A word that folds to nothing is dropped**, in the text and in the query.
- **`NoteContent` is what a load or a save hands the index.** `NoteContent::parse(text: Arc<str>)` computes the `Source` and the words; `IndexSync` calls it on the background executor. `NoteIndex::insert(id, NoteContent)` only stores. This replaces `insert(id, Source, Arc<str>)` (a D1 API change the user approves with this plan). `NoteWords` stays crate-private: `NoteContent` is its only way in.
- **`NoteQuery` arrives with its terms only:** `NoteQuery::new(text)`. Slice 8 adds `showing` and `recent` (`NoteQuery::new(text, showing, recent)`, as S3 writes it) when `find_notes` first reads them. Shipping them now would be dead code.
- **Mtimes move to slice 8**, for the same reason: the ranking is their only reader.
- **`Lookup` is public and readable:** `Lookup::get(term, id) -> Option<TermHit>`, where `TermHit { count, whole }`. `find_notes` reads it in slice 8, and the oracle reads it now.
- **Fold by Unicode simple case folding** (orchestrator, 2026-09-26, after the plan challenge). `char::to_lowercase` leaves `ς`/`σ`, `ſ`/`s`, `ϐ`/`β` apart, so a typed `οδυσσευς` missed `ΟΔΥΣΣΕΥΣ`. After lowercase, NFD and dropping marks, each char maps to one representative of its simple case folding orbit, read from `regex-syntax`'s public `hir::ClassUnicode::try_case_fold_simple`, the tables D4's case-insensitive search uses; so quick open and search agree by construction. The representative is the orbit's smallest char, lowercased when that gives one char. ASCII skips the lookup (every ASCII lowercase letter already represents its orbit). Recorded in S1 as a **Corrected** note.
- **Superseded by G** (orchestrator, 2026-09-26, after a researcher's comparison): one global segment with varint postings, dense ordinals, an alive bitset, a fresh tier and merges off the foreground (Task 7.3G). F, below, was built in `414b676` and replaced.
- **The store is F: frozen, fresh, dead** (orchestrator, 2026-09-26, after the red team measured the plan's first layout at about 625 MB on the Zipf corpus). S2 leaves the representation to the implementer; F replaces S2's "Content" bullet with a **Corrected** note. Each of 256 shards, chosen by a hash of the note id, holds:
  - `frozen: Arc<Frozen>`: the notes merged so far as sorted arrays, CSR-shaped: the distinct words packed in one string with `u32` bounds, one flat postings array with `u32` starts, and the sorted merged note ids. About 8 bytes per (note, word) pair plus 8 bytes and the word's bytes per (shard, word). Immutable; every snapshot shares it.
  - `fresh: HashMap<EntryId, Arc<NoteWords>>`: the notes written since the last merge, each with its own sorted words.
  - `dead: HashSet<EntryId>`: notes of `frozen` removed or rewritten since the merge. `lookup` skips their postings at once; the next merge purges them.
  Frozen notes keep no per-note word list: removal is a tombstone, O(1), and touches no posting. A write copies one shard's `fresh` map and `dead` set (pointers only) while a snapshot holds it; `frozen` is shared. A `set` merges its shard when `fresh.len() + dead.len()` reaches `FRESH_MAX`; `set_notes` never merges, so a trash of 5,500 notes is O(gone). `FRESH_MAX` and `SHARDS` are set by the bench (Task 7.7); a merge must fit in the 2 ms write budget, else Task 7.8 moves merges off the foreground.
- **Memory is judged on RSS, per corpus.** The 40-word startup vault is the realistic one; the Zipf corpus (about 250 distinct words per note) is the stress case. Any uncompressed layout costs at least 8 bytes per (note, word) pair, so the stress case is over 80 MB whatever the layout; if the startup vault is under the line, the alarm stands for the stress case only, with delta-coded postings as its lever in the Evolution table.
- **`WordsSnapshot::heap_bytes()`** is a structural estimate (string bytes, vector capacities, map slots; no allocator or B-tree node overhead). `unsafe_code` is denied workspace-wide, so the bench cannot install a counting allocator; it prints the estimate and the process RSS before and after the build (`ps -o rss=`).

### File structure

| File | Status | Responsibility |
|---|---|---|
| `crates/index/src/words.rs` | create | module list, re-exports, `WORD_MAX_CHARS`, `TERM_MAX_CHARS` |
| `crates/index/src/words/fold.rs`, `fold/tests.rs` | create | S1: `words` (the split) and `fold` |
| `crates/index/src/words/note_words.rs` | create | `NoteWords`: a note's folded words and counts |
| `crates/index/src/words/query.rs` | create | `NoteQuery`: typed text → folded terms |
| `crates/index/src/words/shard.rs` | create | `Words`, `WordsSnapshot`, `Shard`: storage and copy-on-write |
| `crates/index/src/words/lookup.rs` | create | `Lookup`, `TermHit`, `WordsSnapshot::lookup` |
| `crates/index/src/words/tests.rs` | create | behaviour through the public API |
| `crates/index/src/words/model_tests.rs` | create | the lookup oracle |
| `crates/index/src/content.rs`, `content/tests.rs` | create | `NoteContent::parse` |
| `crates/index/src/index.rs` | modify | the `words` field; `insert(id, NoteContent)`; `remove`; `set_notes` drops gone words; `words()` |
| `crates/index/src/index/words_tests.rs` | create | `NoteIndex` keeps the words current |
| `crates/index/src/index/model_tests.rs` | modify | the updates oracle: words equal a fresh build |
| `crates/index/src/index/{tests,corpus_tests}.rs`, `test_support.rs` | modify | `insert` call sites; `load` helper |
| `crates/index/src/lib.rs` | modify | modules and re-exports |
| `crates/index/benches/{index,names,search}.rs` | modify | `insert` call sites |
| `crates/index/benches/words.rs`, `crates/index/Cargo.toml` | create / modify | the `words` bench |
| `crates/app/src/index.rs`, `index/loading.rs` | modify | `NoteContent::parse` off the foreground, in `load_more`'s task and `saved`'s parse task |
| `crates/app/src/index/{tests,loading_tests}.rs` | modify | `apply` and `Landed` call sites; the random-operations snapshot compares words |
| `crates/app/src/index/words_tests.rs` | create | `IndexSync` feeds the words |
| `crates/app/src/timeline.rs`, `timeline/tests.rs` | modify | `Step::Indexed` reports `words_bytes` |
| spec | modify | Zed paragraph, decisions, figures |

### Task 7.1: Words and folding (S1)

**Files:** create `crates/index/src/words.rs`, `words/fold.rs`, `words/fold/tests.rs`; modify `crates/index/src/lib.rs`.

**Interfaces:**
- Produces: `pub(crate) fn words::word_spans(text: &str) -> impl Iterator<Item = (usize, &str)>`; `pub(crate) fn words::fold(word: &str) -> Cow<'_, str>`; `pub(crate) fn words::cap(word: &str, chars: usize) -> &str`.

- [ ] **Step 1: The module, empty.** Create `crates/index/src/words.rs`:

```rust
//! The word index: which notes hold a word starting with a term (smart quick open spec S1, S2).

mod fold;

pub(crate) use fold::{cap, fold, word_spans};
```

In `crates/index/src/lib.rs`, add `mod words;` in alphabetical order (after `mod target;`). Nothing is re-exported yet.

- [ ] **Step 2: Write the failing tests.** Create `crates/index/src/words/fold/tests.rs`:

```rust
use super::{cap, fold, word_spans};

fn split(text: &str) -> Vec<(usize, &str)> {
    word_spans(text).collect()
}

#[test]
fn a_word_is_a_maximal_run_of_word_chars() {
    assert_eq!(split("l'été"), [(0, "l"), (2, "été")]);
    assert_eq!(split("2026-09"), [(0, "2026"), (5, "09")]);
    assert_eq!(split("snake_case"), [(0, "snake_case")]);
    assert_eq!(split("  a  b "), [(2, "a"), (5, "b")]);
    assert_eq!(split(""), []);
    assert_eq!(split("—!?"), []);
}

#[test]
fn apostrophes_straight_and_typographic_split_words() {
    assert_eq!(split("aujourd’hui"), [(0, "aujourd"), (10, "hui")]);
    assert_eq!(split("aujourd'hui"), [(0, "aujourd"), (8, "hui")]);
}

#[test]
fn a_combining_mark_stays_inside_its_word() {
    assert_eq!(split("e\u{301}te x"), [(0, "e\u{301}te"), (6, "x")]);
}

#[test]
fn an_emoji_is_not_a_word_char() {
    assert_eq!(split("a🎉b"), [(0, "a"), (5, "b")]);
}

#[test]
fn case_and_accents_fold_both_ways() {
    for word in ["Été", "ÉTÉ", "ete", "e\u{301}te", "été"] {
        assert_eq!(fold(word), "ete", "{word:?}");
    }
}

#[test]
fn special_letters_fold_as_the_spec_says() {
    assert_eq!(fold("İstanbul"), "istanbul");
    assert_eq!(fold("ı"), "ı", "dotless i is its own letter");
    assert_eq!(fold("\u{212A}elvin"), "kelvin");
    assert_eq!(fold("Straße"), "straße", "no compatibility folding");
    assert_eq!(fold("ﬁn"), "ﬁn");
    assert_eq!(fold("ǅ"), "ǆ", "a compatibility decomposition is not applied");
    assert_eq!(fold("한"), "\u{1112}\u{1161}\u{11AB}", "NFD can lengthen a word");
}

#[test]
fn case_variants_fold_by_simple_case_folding() {
    assert_eq!(fold("ſ"), "s");
    assert_eq!(fold("ΟΔΥΣΣΕΥΣ"), "οδυσσευσ");
    assert_eq!(fold("οδυσσευς"), "οδυσσευσ");
    assert_eq!(fold("ϐ"), fold("β"));
    assert_eq!(fold("µ"), fold("μ"), "micro sign and mu");
    assert_eq!(fold("ẞ"), "ß");
    assert_eq!(fold("漢字"), "漢字");
}

/// `fold` must not drift from `(?i)` in D4's search: two letters fold alike exactly when
/// `regex-syntax` parses `(?i)x` into a class holding `y`.
#[test]
fn fold_agrees_with_regex_case_insensitive_classes() {
    // Letters `fold` changes by case alone: no decomposition, and a one-char lowercase.
    let letters: Vec<char> = ('\u{0}'..='\u{2FFF}')
        .filter(|c| c.is_alphabetic())
        .filter(|c| std::iter::once(*c).nfd().eq(std::iter::once(*c)))
        .filter(|c| c.to_lowercase().count() == 1)
        .collect();
    // For each letter, the letters `regex-syntax`'s parser puts in `(?i)x`, against the letters
    // with the same fold; the implementer writes this without an O(n²) loop.
}

#[test]
fn a_few_letters_agree_with_the_regex_engine() {
    for (x, y) in [("ς", "σ"), ("Σ", "ς"), ("ſ", "S"), ("ϐ", "Β"), ("\u{212A}", "k"), ("ß", "ẞ")] {
        let Ok(re) = regex::Regex::new(&format!("(?i)^{}$", regex::escape(x))) else {
            panic!("{x:?} compiles");
        };
        assert!(re.is_match(y), "{x:?} {y:?}");
        assert_eq!(fold(x), fold(y), "{x:?} {y:?}");
    }
    assert_ne!(fold("ı"), fold("i"), "dotless i is its own letter");
}

#[test]
fn a_keycap_folds_to_its_digit() {
    // VS16 and the enclosing keycap are word chars and combining marks.
    assert_eq!(split("1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}"), [(0, "1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}")]);
    assert_eq!(fold("1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}"), "12");
}

#[test]
fn a_lone_mark_folds_to_nothing() {
    assert_eq!(fold("\u{301}"), "");
}

#[test]
fn ascii_folds_without_allocating_when_already_lower() {
    assert!(matches!(fold("plan"), std::borrow::Cow::Borrowed("plan")));
    assert_eq!(fold("PLAN"), "plan");
}

#[test]
fn cap_cuts_on_a_char_boundary() {
    assert_eq!(cap("été", 2), "ét");
    assert_eq!(cap("été", 3), "été");
    assert_eq!(cap("été", 9), "été");
    assert_eq!(cap("", 1), "");
}
```

- [ ] **Step 3: Run it to see it fail.**

Run: `cargo test -p inkra-index words::fold`
Expected: compile error, `fold.rs` does not exist.

- [ ] **Step 4: Implement.** Create `crates/index/src/words/fold.rs`:

```rust
use std::borrow::Cow;
use std::iter;

use regex_syntax::hir::{ClassUnicode, ClassUnicodeRange};
use regex_syntax::is_word_character;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Each maximal run of word chars in `text`, with its byte offset: D4's word rule, so a
/// whole-word search and quick open agree on what a word is.
pub(crate) fn word_spans(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut chars = text.char_indices().peekable();
    iter::from_fn(move || {
        let (start, _) = chars.by_ref().find(|&(_, c)| is_word_character(c))?;
        let mut end = text.len();
        while let Some(&(at, c)) = chars.peek() {
            if !is_word_character(c) {
                end = at;
                break;
            }
            chars.next();
        }
        Some((start, &text[start..end]))
    })
}

/// Lowercase, NFD, no combining mark, then one char per case-folding orbit (S1): `Été`,
/// `ETE` and `e\u{301}te` fold to `ete`, `ς` and `Σ` to `σ`.
pub(crate) fn fold(word: &str) -> Cow<'_, str> {
    if word.is_ascii() {
        return if word.bytes().any(|byte| byte.is_ascii_uppercase()) {
            Cow::Owned(word.to_ascii_lowercase())
        } else {
            Cow::Borrowed(word)
        };
    }
    Cow::Owned(
        word.chars()
            .flat_map(char::to_lowercase)
            .nfd()
            .filter(|&c| !is_combining_mark(c))
            .map(representative)
            .collect(),
    )
}

/// One char per simple case folding orbit, from the tables D4's `(?i)` search uses.
fn representative(c: char) -> char {
    // Every ASCII lowercase letter represents its orbit (`s` for `S`, `ſ`); a caseless char is alone.
    if c.is_ascii() || !(c.is_lowercase() || c.is_uppercase()) {
        return c;
    }
    let mut orbit = ClassUnicode::new([ClassUnicodeRange::new(c, c)]);
    if orbit.try_case_fold_simple().is_err() {
        return c;
    }
    let first = orbit.ranges().first().map_or(c, ClassUnicodeRange::start);
    let mut lower = first.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => first,
    }
}

/// The first `chars` chars of `word`.
pub(crate) fn cap(word: &str, chars: usize) -> &str {
    word.char_indices()
        .nth(chars)
        .map_or(word, |(at, _)| &word[..at])
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 5: Run the tests.**

Run: `cargo test -p inkra-index words::fold`
Expected: 13 passed. `cargo clippy -p inkra-index --all-targets -- -D warnings` may report the three fns as unused outside tests: that is expected until Task 7.2 and is not committed alone. Go on to Task 7.2 before the commit.

### Task 7.2: `NoteWords` and `NoteQuery`

**Files:** create `crates/index/src/words/note_words.rs`, `words/query.rs`, `words/tests.rs`; modify `words.rs`, `lib.rs`.

**Interfaces:**
- Consumes: `words`, `fold`, `cap` (Task 7.1).
- Produces: `pub(crate) struct NoteWords` (`Debug`, `Clone`, `Default`, `PartialEq`, `Eq`), packed, with `parse`, `is_empty`, `iter`, `with_prefix` and `heap_bytes` (Step 3); `pub struct inkra_index::NoteQuery` (`Debug`, `Clone`) with `#[must_use] pub fn new(text: &str) -> Self`, `#[must_use] pub fn is_empty(&self) -> bool`, `pub(crate) fn terms(&self) -> &[Box<str>]`; `pub(crate) const WORD_MAX_CHARS: usize = 65`, `pub(crate) const TERM_MAX_CHARS: usize = 64`.

- [ ] **Step 1: Write the failing tests.** Create `crates/index/src/words/tests.rs`:

```rust
use super::{NoteWords, TERM_MAX_CHARS, WORD_MAX_CHARS};
use crate::NoteQuery;

fn counts(text: &str) -> Vec<(String, u32)> {
    NoteWords::parse(text)
        .iter()
        .map(|(word, count)| (word.to_owned(), count))
        .collect()
}

fn terms(text: &str) -> Vec<String> {
    NoteQuery::new(text)
        .terms()
        .iter()
        .map(|term| term.to_string())
        .collect()
}

#[test]
fn a_note_s_words_are_folded_counted_and_sorted() {
    assert_eq!(
        counts("Été, l'été et ETE. Plan"),
        [
            ("et".to_owned(), 1),
            ("ete".to_owned(), 3),
            ("l".to_owned(), 1),
            ("plan".to_owned(), 1)
        ]
    );
    assert_eq!(counts(""), []);
    assert_eq!(counts("\u{301} ! \u{301}"), [], "a word folding to nothing is dropped");
}

#[test]
fn a_long_word_is_stored_capped_and_still_found() {
    let long = "a".repeat(1_000_000);
    let found = counts(&format!("{long} {long}b"));
    assert_eq!(found, [("a".repeat(WORD_MAX_CHARS), 2)]);
}

#[test]
fn a_base64_image_splits_on_plus_and_slash_and_stays_capped() {
    const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // A fixed walk over the alphabet: a 1 MB paste of an image, not one repeated letter.
    let blob: String = (0..1_000_000usize)
        .map(|n| char::from(BASE64[n * 7_919 % 61 + usize::from(n % 7 == 0) * 3]))
        .collect();
    let words = NoteWords::parse(&blob);
    assert!(words.iter().len() > 1_000, "`+` and `/` cut the blob into many words");
    assert!(words.iter().all(|(word, _)| word.chars().count() <= WORD_MAX_CHARS));
    assert!(words.iter().all(|(word, _)| !word.contains(['+', '/'])));
}

#[test]
fn a_query_splits_like_the_text_then_folds_and_drops_repeats() {
    assert_eq!(terms("Été  l'ÉTÉ plan"), ["ete", "l", "plan"]);
    assert_eq!(terms("aujourd'hui"), ["aujourd", "hui"]);
    assert_eq!(terms("plan PLAN plän"), ["plan"]);
}

#[test]
fn a_query_of_marks_is_empty() {
    assert!(NoteQuery::new("\u{301} \u{301}").is_empty());
    assert!(NoteQuery::new("  —  ").is_empty());
    assert!(NoteQuery::new("").is_empty());
    assert!(!NoteQuery::new("a").is_empty());
}

#[test]
fn a_query_keeps_its_first_64_typed_chars() {
    let typed = format!("{} tail", "a".repeat(64));
    assert_eq!(terms(&typed), ["a".repeat(64)]);
}

#[test]
fn a_term_keeps_at_most_64_folded_chars() {
    // `한` folds to three jamo, so 40 of them would fold to 120 chars.
    let typed = "한".repeat(40);
    let found = terms(&typed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].chars().count(), TERM_MAX_CHARS);
    assert!(WORD_MAX_CHARS > TERM_MAX_CHARS, "a capped word never equals a term");
}
```

- [ ] **Step 2: Run them to see them fail.**

Run: `cargo test -p inkra-index words::tests`
Expected: compile error, `NoteWords` and `NoteQuery` are not defined.

- [ ] **Step 3: `NoteWords`.** Create `crates/index/src/words/note_words.rs`. It is what a parse hands the store, and what a fresh note keeps until its shard merges (Task 7.3), so it is compact: one allocation for the words, not one per word.

```rust
/// A note's distinct folded words, each capped, sorted, with how often each occurs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NoteWords {
    /// The words, concatenated in order; word `n` ends at `ends[n]`.
    bytes: Box<str>,
    ends: Box<[usize]>,
    counts: Box<[u32]>,
}

impl NoteWords {
    /// Counts in a map keyed by the folded, capped word, borrowing from `text` when the fold
    /// does not allocate; then sorts the keys and packs them.
    pub(crate) fn parse(text: &str) -> Self;

    pub(crate) fn is_empty(&self) -> bool;

    /// Every word and its count, in order.
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = (&str, u32)> + '_;

    /// The words starting with `term`, in order: one binary search, then a run.
    pub(crate) fn with_prefix<'a>(&'a self, term: &'a str) -> impl Iterator<Item = (&'a str, u32)> + 'a;

    /// Its bytes, `ends` and `counts`, for `WordsSnapshot::heap_bytes`.
    pub(crate) fn heap_bytes(&self) -> usize;
}
```

The implementer writes the bodies. A word folding to nothing is skipped; a repeat adds to its count with `saturating_add`. `with_prefix` is read by Task 7.3's lookup, `heap_bytes` by Task 7.3's estimate; both ship in the same commit as their reader. Tests call `iter`: in `words/tests.rs`, `counts` becomes

```rust
fn counts(text: &str) -> Vec<(String, u32)> {
    NoteWords::parse(text)
        .iter()
        .map(|(word, count)| (word.to_owned(), count))
        .collect()
}
```

and `a_base64_image_splits_on_plus_and_slash_and_stays_capped` reads `NoteWords::parse(&blob).iter()`. Add

```rust
#[test]
fn with_prefix_finds_the_run_of_words_starting_with_a_term() {
    let words = NoteWords::parse("pro projet projets apro prune");
    let found: Vec<(&str, u32)> = words.with_prefix("pro").collect();
    assert_eq!(found, [("pro", 1), ("projet", 1), ("projets", 1)]);
    assert_eq!(words.with_prefix("zz").count(), 0);
    assert_eq!(words.with_prefix("").count(), 5);
}
```

- [ ] **Step 4: `NoteQuery`.** Create `crates/index/src/words/query.rs`:

```rust
use super::{TERM_MAX_CHARS, cap, fold, word_spans};

/// S1 bounds the typed query; folding can still lengthen a term, which `TERM_MAX_CHARS` bounds.
const QUERY_MAX_CHARS: usize = 64;

/// A quick-open query, folded once per keystroke and shared by every range.
#[derive(Debug, Clone)]
pub struct NoteQuery {
    terms: Vec<Box<str>>,
}

impl NoteQuery {
    /// Splits `text` into words as a note's text is split, folds each, and drops repeats.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let text: String = text.chars().take(QUERY_MAX_CHARS).collect();
        let mut terms: Vec<Box<str>> = Vec::new();
        for (_, word) in word_spans(&text) {
            let folded = fold(word);
            let term = cap(&folded, TERM_MAX_CHARS);
            if !term.is_empty() && !terms.iter().any(|seen| **seen == *term) {
                terms.push(term.into());
            }
        }
        Self { terms }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub(crate) fn terms(&self) -> &[Box<str>] {
        &self.terms
    }
}
```

- [ ] **Step 5: Wire the module.** Replace `crates/index/src/words.rs` with:

```rust
//! The word index: which notes hold a word starting with a term (smart quick open spec S1, S2).

mod fold;
mod note_words;
mod query;

pub(crate) use fold::{cap, fold, word_spans};
pub(crate) use note_words::NoteWords;
pub use query::NoteQuery;

/// Above `TERM_MAX_CHARS`, so a capped word keeps every prefix a term can ask for and never
/// equals a term.
pub(crate) const WORD_MAX_CHARS: usize = TERM_MAX_CHARS + 1;
pub(crate) const TERM_MAX_CHARS: usize = 64;

#[cfg(test)]
mod tests;
```

In `crates/index/src/lib.rs`, add `pub use words::NoteQuery;` in alphabetical order of the `pub use` lines.

- [ ] **Step 6: Run the tests.**

Run: `cargo test -p inkra-index words`
Expected: 21 passed (13 fold, 8 words). `NoteWords` is still unread outside tests; clippy may flag it until Task 7.3. Do not commit yet.

### Task 7.3: The sharded store — `Frozen`, `Words` and `WordsSnapshot` (F)

**Built in `414b676`, replaced by Task 7.3G** (the global store). Kept here for the record.

Rewritten on 2026-09-26 after the plan challenge (decision "The store is F"). The first layout (a `BTreeMap<Arc<str>, Vec<Posting>>` per shard and a per-note `Box<[Arc<str>]>`) was estimated by the red team at about 625 MB on the Zipf corpus and 55 MB on the startup vault; F aims at about 180 MB and 16 MB, with O(1) writes on the foreground.

**Files:** create `crates/index/src/words/frozen.rs`, `words/frozen/tests.rs`, `words/shard.rs`, `words/shard/tests.rs`; modify `words.rs`, `lib.rs`.

**Interfaces:**
- Consumes: `NoteWords` (Task 7.2: `iter`, `with_prefix`, `heap_bytes`).
- Produces, crate-private: `Frozen` (`words/frozen.rs`); `Words` (`Default`) with `set(&mut self, id, NoteWords)`, `remove(&mut self, id)`, `remove_all(&mut self, gone: &HashSet<EntryId>)`, `snapshot(&self) -> WordsSnapshot`; test-only `Words::with_shards(n)`, `Words::merge_all()` and `WordsSnapshot::entries()`.
- Produces, public: `inkra_index::WordsSnapshot` (`Clone`, no `Default`) with `#[must_use] pub fn heap_bytes(&self) -> usize`.

**`Frozen`** (`words/frozen.rs`): a shard's merged notes, immutable.

```rust
pub(super) type Posting = (EntryId, u32);

/// A shard's notes as of its last merge, in sorted arrays; every snapshot shares it.
#[derive(Debug, Default)]
pub(super) struct Frozen {
    /// The distinct words, sorted and concatenated; word `w` is `bytes[bounds[w]..bounds[w + 1]]`.
    bytes: Box<str>,
    bounds: Box<[u32]>,
    /// Word `w`'s notes, by id, are `postings[starts[w]..starts[w + 1]]`.
    starts: Box<[u32]>,
    postings: Box<[Posting]>,
    /// The merged notes, sorted.
    notes: Box<[EntryId]>,
}

impl Frozen {
    /// `self` without `dead`, plus `fresh`: one pass over `self`, `fresh`'s pairs sorted once.
    pub(super) fn merge(&self, dead: &HashSet<EntryId>, fresh: &HashMap<EntryId, Arc<NoteWords>>) -> Self;
    pub(super) fn contains(&self, id: EntryId) -> bool;           // binary search on `notes`
    /// The words starting with `term` and their postings: one binary search, then a run.
    pub(super) fn with_prefix<'a>(&'a self, term: &'a str) -> impl Iterator<Item = (&'a str, &'a [Posting])> + 'a;
    pub(super) fn heap_bytes(&self) -> usize;
    #[cfg(test)]
    pub(super) fn words(&self) -> impl Iterator<Item = (&str, &[Posting])>;
}
```

`bounds` and `starts` have one more entry than there are words (a leading 0), so every word has both ends and an empty `Frozen` has `[0]` in each; `Default` must give that shape too (write `Default` by hand, or build through `merge` of nothing). Offsets are `u32`: a shard holding 4 GiB of words or 4 G postings is a caller bug, a panic with a message, never a silent wrap (no `as`; `u32::try_from` and a `let … else { panic!(…) }`).

`frozen/tests.rs`: a merge of nothing is empty; a merge sorts the words and each word's postings by id; `dead` is purged from postings and `notes`, and a word left with no posting is gone; a fresh note that is also frozen (and so dead) keeps only its fresh words; `with_prefix` returns exactly the run (an empty term returns every word; a term past the last word returns nothing); a word of 65 chars and a word with multibyte chars keep their bytes exact.

**`Shard`, `Words`, `WordsSnapshot`** (`words/shard.rs`):

```rust
/// Set by the `words` bench (Task 7.7).
pub(super) const SHARDS: usize = 256;
/// A shard merges when its fresh and dead notes reach this; set by the `words` bench.
pub(super) const FRESH_MAX: usize = 32;

/// One shard: its merged notes, the notes written since, and the merged ones since removed.
#[derive(Clone, Default)]
pub(super) struct Shard {
    frozen: Arc<Frozen>,
    /// A note here is found by its own words; a later write replaces them.
    fresh: HashMap<EntryId, Arc<NoteWords>>,
    /// Notes of `frozen` removed or rewritten since the merge: their postings are skipped.
    dead: HashSet<EntryId>,
}
```

Invariants, each held by construction: `dead ⊆ frozen.notes`; a note in `fresh` that `frozen` holds is in `dead`; `fresh` never holds empty words. `Shard::holds(id)` is `fresh.contains_key(&id) || (frozen.contains(id) && !dead.contains(&id))`.

- `set(id, words)`: mark `id` dead if `frozen` holds it; store `words` in `fresh`, or drop `id` from `fresh` when `words` is empty; then merge when `fresh.len() + dead.len() >= FRESH_MAX`. A merge replaces `frozen` with `frozen.merge(&dead, &fresh)` and clears `fresh` and `dead`.
- `remove(id)`: drop it from `fresh`; mark it dead if `frozen` holds it. Never merges.
- `remove_all(gone)`: the same per id. Never merges, so a trash of 5,500 notes is O(gone) plus one copy of each touched shard's `fresh` map and `dead` set while a snapshot holds them.
- `matches(term) -> impl Iterator<Item = (EntryId, &str, u32)>`: `frozen.with_prefix(term)` skipping dead postings (check `dead` only when it is not empty), then each fresh note's `with_prefix(term)`. Read by Task 7.5's `lookup`.

`Words { shards: Arc<Vec<Arc<Shard>>> }`:

- `Default` builds `SHARDS` clones of one empty `Arc<Shard>`; `#[cfg(test)] with_shards(n)` builds `n`, so tests can put several notes in one shard. `shard_of(id)` is a hash of `id` modulo `self.shards.len()` (`DefaultHasher`, `u64::try_from(len)`, no `as`).
- `set`, `remove`, `remove_all` guard as before: a note the shard does not hold, written with no words or removed, copies nothing (`Arc::make_mut` only after the guard). `remove_all` groups the held ids by shard first.
- `snapshot()` is an `Arc` clone of the list.
- `#[cfg(test)] merge_all()` merges every shard with fresh or dead notes.

`WordsSnapshot(pub(super) Arc<Vec<Arc<Shard>>>)`: `heap_bytes` sums each shard's `frozen.heap_bytes()` (a `Frozen` shared by two snapshots counts in each), its fresh notes' `NoteWords::heap_bytes` plus their map slots, and its dead set's slots. `#[cfg(test)] entries()` returns every visible `((word, id), count)`: frozen postings not dead, plus fresh words.

`shard/tests.rs` (use `Words::with_shards(1)` unless the test is about shards), each an invariant the orchestrator named:

```rust
#[test] fn set_replaces_a_note_s_words_and_remove_drops_them()     // before and after `merge_all`
#[test] fn remove_all_drops_every_gone_note()                     // fresh and frozen notes alike
#[test] fn a_held_words_snapshot_never_changes()                  // across set, remove, remove_all and a merge
#[test] fn a_write_that_changes_nothing_copies_no_shard()         // `Arc::ptr_eq` on the shard list
#[test] fn a_tombstoned_note_is_invisible_at_once()               // merged, removed, not merged again: absent
#[test] fn re_inserting_a_tombstoned_note_shows_only_its_new_words()
#[test] fn a_merge_changes_no_entry()                             // `entries()` before and after `merge_all`
#[test] fn a_shard_merges_when_fresh_and_dead_reach_fresh_max()  // FRESH_MAX writes in one shard: none fresh
#[test] fn notes_spread_over_shards_and_each_is_found()           // `Words::default()`, 300 notes
#[test] fn heap_bytes_grows_with_the_words_and_returns_when_they_leave() // after remove and merge_all
```

`a_shard_merges_…` reads a `#[cfg(test)] fn Words::fresh_len(&self) -> usize` (the sum over shards).

- [ ] **Step 1: Write the failing tests** above, in `frozen/tests.rs` and `shard/tests.rs`.
- [ ] **Step 2: Run them to see them fail.** Run: `cargo test -p inkra-index words::` — compile errors, `Frozen` and `Words` are not defined.
- [ ] **Step 3: Implement** `frozen.rs`, then `shard.rs`. In `words.rs`: `mod frozen; mod shard;`, `pub(crate) use shard::Words;`, `pub use shard::WordsSnapshot;`. In `lib.rs`: `pub use words::{NoteQuery, WordsSnapshot};`.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p inkra-index words::` — all pass. `Words` is not read outside tests until Task 7.4; do not commit yet.

### Task 7.3G: The global store (G), replacing F

Decided by the orchestrator on 2026-09-26, after a researcher (opus) compared F with tantivy's segment model, a global vocabulary, compressed postings and Meilisearch's prefix postings. F (Task 7.3) was built and committed in `414b676`; its 256-way split repeats the vocabulary and widens the postings' gaps. A simulation of the stress corpus (`/tmp/zipf_sim.py`: 50k notes, 30k-word Zipf vocabulary, 250 distinct words per note, 12.5M pairs) gives: F, 256 shards, about 153 MB; 256 shards with varint postings, about 93 MB; 64 shards, varint, about 60 MB; **one global segment, varint, about 28 MB**. The global segment also serializes without re-sharding (slice 9). Sources to cite in the spec: tantivy `ARCHITECTURE.md` and `src/indexer/log_merge_policy.rs`, Lucene `TieredMergePolicy`, Meilisearch `words_prefixes_fst.rs`.

The public API of Tasks 7.4–7.5 stays: `Words::{set, remove, remove_all, snapshot}`, `WordsSnapshot::{lookup, heap_bytes}`, `Lookup::{get, terms}`, `TermHit`. G adds the merge API below. `frozen.rs` and `shard.rs` are deleted; `note_words.rs`, `fold.rs`, `query.rs`, `content.rs` and every oracle stay.

**Files:** create `words/ordinal.rs` (ordinals and the alive bitset), `words/segment.rs` + `segment/tests.rs` (the frozen segment and its build), `words/varint.rs` + `varint/tests.rs`, `words/store.rs` + `store/tests.rs` (`Words`, `WordsSnapshot`, the merge); modify `words/lookup.rs`, `words.rs`, `lib.rs`, `words/model_tests.rs`, `words/tests.rs`; delete `words/frozen*`, `words/shard*`. Split a file past ~400 lines.

**Data.**

- **Ordinals.** The store gives each note a dense `u32` ordinal at its first `set` with words, in a `HashMap<EntryId, u32>`, and never reuses one within a session: a note removed then set again gets a new ordinal. `inkra-vault` gains no accessor. A removed note's entry leaves the map; its ordinal's postings are dead. No reverse table (ordinal → `EntryId`) ships unless a non-test reader needs one; tests invert the map. Ordinals are not compacted; the bench reports how many exist after its churn cases, and the spec says whether compaction is needed.
- **The segment**, immutable, `Arc`-shared by every snapshot until the next merge:
  - the distinct words, sorted, packed in one string with `u32` bounds (a leading 0), range-scanned for a prefix with the shared `first_at_or_after`;
  - per word, its postings as varint pairs in one `Box<[u8]>`: the gap from the previous ordinal (the first is the ordinal itself), then the count, both LEB128; a `u32` start per word (a leading 0). Counts stay exact `u32`: the ranking reads them;
  - encoded only by a merge.
- **Alive**, a bitset (`Vec<u64>`) over ordinals: bit set when the segment's postings for that ordinal are live. A remove or a re-set clears the bit: O(1). `Arc`-shared, copied on write while a snapshot holds it (about 6 KB at 50k).
- **Fresh**, the notes set since they were last merged: `HashMap<u32, Arc<NoteWords>>` by ordinal, `Arc`-shared, copied on write (pointers only) while a snapshot holds it. A fresh note is found by its own `with_prefix`.
- `WordsSnapshot { ordinals, segment, alive, fresh }`: four `Arc`s.

Invariants, held by construction and tested: an ordinal is in `fresh` or has its alive bit set, never both; a re-set clears the bit and replaces the fresh entry; an alive bit is never set for an ordinal the segment does not hold.

**Merges run off the foreground**, as a `NoteSet` build does.

```rust
/// A merge of the fresh notes into a new segment, run off the foreground.
pub struct WordsMerge { /* segment, alive and fresh as the merge started; its generation */ }
/// A merge's result, to swap in with `NoteIndex::apply_words_merge`.
pub struct MergedWords { /* the new segment, the ordinals it holds live, its generation */ }

impl WordsMerge {
    #[must_use]
    pub fn run(self) -> MergedWords;            // background: decodes, merges, encodes
}
impl NoteIndex {
    /// A merge, when the policy says one is due and none runs.
    #[must_use]
    pub fn words_merge(&mut self) -> Option<WordsMerge>;
    /// Swaps a merge's result in; a stale one (another generation) is dropped.
    pub fn apply_words_merge(&mut self, merged: MergedWords);
}
```

- **Start** (`Words::merge(&mut self) -> Option<WordsMerge>`, the policy-free start the tests call; `NoteIndex::words_merge` calls it when `Words::merge_due` holds): nothing when a merge runs or there is nothing to merge; else bump the generation, capture `segment`, `alive` and the fresh entries (sorted by ordinal), and start recording `touched: HashSet<u32>`, the ordinals set or removed from now on.
- **Run**: one pass over the old segment's words in order, merged with the captured fresh notes' sorted words (k-way, as F's `sorted_pairs`); per word, the old postings whose alive bit was set and whose ordinal is not a captured fresh note, merged by ordinal with the fresh postings, re-encoded. Words left with no posting are dropped. The result also carries the ordinals it holds live: the captured alive bits plus the captured fresh ordinals.
- **Apply**: dropped unless its generation is the running merge's. Else the new segment replaces the old; alive becomes the result's live ordinals minus `touched`; every captured fresh entry leaves `fresh` unless its ordinal is in `touched` (a write since the start replaced it, and stays fresh); `touched` stops recording. So a delete during a running merge stays deleted, and an insert during it is visible once after it.
- **Policy** (`Words::merge_due`): fresh holds at least `max(FRESH_MIN, live / FRESH_RATIO)` notes, or dead segment ordinals are 20 % or more of the segment's (Lucene's `deletesPctAllowed`). `FRESH_MIN` and `FRESH_RATIO` are set by the bench. The foreground does nothing but the start's pointer captures and the apply's swap and bit clears.
- **`IndexSync`** (Task 7.4G) asks for a merge after each foreground turn that changed the index, runs it with `cx.background_spawn`, applies it in a `timed_turn`, and drops the old segment off the foreground. One merge runs at a time.

**`lookup`** accumulates each term into a dense `Vec<TermHit>` indexed by ordinal (zeroed per term, a count of 0 is "absent"), not a `HashMap`. `Lookup` keeps the snapshot's `Arc` ordinal map, so `get(term, id)` is one map read and one index. Its public shape does not change.

**Short prefixes: measure first.** If `lookup/1 letter` misses its budget, precompute postings for the 1- and 2-char prefixes shared by 100 words or more (Meilisearch's rule), exact, built by the merge. Preferred over S3's one-letter cap.

**Tests** (`segment/tests.rs`, `varint/tests.rs`, `store/tests.rs`), besides every F invariant test carried over through the public API (a held snapshot never changes; a tombstoned note is invisible at once; a re-set note shows only its new words; a merge changes no entry; a write that changes nothing copies nothing):

- varint round-trips `0`, `127`, `128`, `u32::MAX`, and a gap sequence;
- a segment built from nothing is empty; built words are sorted, postings sorted by ordinal with exact counts; `with_prefix` is exactly the run;
- `a_delete_during_a_running_merge_stays_deleted_after_it_applies`;
- `an_insert_during_a_running_merge_is_visible_once_after_it`;
- `a_re_set_during_a_running_merge_keeps_only_its_new_words`;
- `a_stale_merge_is_dropped`;
- `a_merge_is_due_on_fresh_size_and_on_dead_share`;
- `ordinals_are_not_reused`.

The words oracle (`words/model_tests.rs`) gains `StartMerge` and `ApplyMerge` ops: a started merge's result is applied at a random later point, with writes, removes and `remove_all` in between, and a held snapshot across both. `Words::with_shards` goes; the oracle drives `Words::default()`.

- [ ] **Step 1:** the failing tests above. **Step 2:** `varint.rs`, `ordinal.rs`, `segment.rs`, `store.rs`, then `lookup.rs`; delete F. **Step 3:** `cargo test -p inkra-index`, `PROPTEST_CASES=20000 cargo test -p inkra-index --release words::model_tests`, clippy. Commit with Task 7.4G.

### Task 7.4G: `NoteIndex` and `IndexSync` run the merges

**Files:** modify `crates/index/src/index.rs`, `index/model_tests.rs`, `lib.rs` (re-export `WordsMerge`, `MergedWords`); `crates/app/src/index.rs` (or a new `crates/app/src/index/merging.rs` if `index.rs` grows past ~400 lines), `crates/app/src/index/words_tests.rs`.

- `NoteIndex::words_merge` and `apply_words_merge` as above; `#[cfg(test)] merge_words` becomes a start-run-apply in one call.
- The updates oracle (`index/model_tests.rs`) gains `StartMerge` and `ApplyMerge` ops, as the words oracle.
- `IndexSync`: after `apply_landed`, `apply` and `forget` change the index, `self.merge_words(cx)` starts a due merge on the background executor; its apply runs in `Self::timed_turn`; the replaced segment is freed off the foreground. A merge result arriving after this entity is gone is dropped.
- App tests: during the startup load of a vault large enough to cross the policy, a merge runs and applies, and `looked_up` answers as a fresh build (the random-operations test's snapshot compares words); a trash during a held merge (gate the background task) leaves the note absent after the merge applies.
- [ ] Run `cargo test -p inkra-index -p inkra-app`, then `./scripts/check.sh`; commit "Words: one global segment with varint postings, merged off the foreground".

### Task 7.4: `NoteContent`, and `NoteIndex` keeps the words current

**Files:** create `crates/index/src/content.rs`, `content/tests.rs`, `index/words_tests.rs`; modify `index.rs`, `index/tests.rs`, `index/corpus_tests.rs`, `index/model_tests.rs`, `test_support.rs`, `lib.rs`, `benches/{index,names,search}.rs`.

**Interfaces:**
- Consumes: `Source::parse`, `NoteWords::parse`, `Words`.
- Produces: `pub struct inkra_index::NoteContent` with `#[must_use] pub fn parse(text: Arc<str>) -> Self` and `#[must_use] pub fn source(&self) -> &Source`; `NoteIndex::insert(&mut self, id: EntryId, content: NoteContent)`; `#[must_use] pub fn NoteIndex::words(&self) -> WordsSnapshot`.

- [ ] **Step 1: Write the failing tests.** Create `crates/index/src/index/words_tests.rs`:

```rust
use std::collections::BTreeMap;

use inkra_vault::EntryId;

use super::NoteIndex;
use crate::test_support::{ROOT, id, load, note_set, rename, vault};

fn entries(index: &NoteIndex) -> BTreeMap<(String, EntryId), u32> {
    index.words().entries()
}

#[test]
fn a_load_indexes_the_words_and_a_reload_replaces_them() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    load(&mut index, a, "Été [[plan]]");
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("ete".to_owned(), a), 1), (("plan".to_owned(), a), 1)])
    );
    load(&mut index, a, "other");
    assert_eq!(entries(&index), BTreeMap::from([(("other".to_owned(), a), 1)]));
}

#[test]
fn remove_drops_the_words() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let a = id(&vault, "a.md");
    load(&mut index, a, "plan");
    index.remove(a);
    assert!(entries(&index).is_empty());
}

#[test]
fn a_load_for_a_non_note_indexes_nothing() {
    let (_fs, vault) = vault(&["a.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, EntryId::DETACHED, "plan");
    assert!(entries(&index).is_empty());
}

#[test]
fn a_rename_keeps_the_words_and_a_trash_drops_them() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let (a, b) = (id(&vault, "a.md"), id(&vault, "b.md"));
    load(&mut index, a, "alpha");
    load(&mut index, b, "beta");

    rename(&fs, &mut vault, "a.md", "c.md");
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(
        entries(&index),
        BTreeMap::from([(("alpha".to_owned(), a), 1), (("beta".to_owned(), b), 1)])
    );

    fs.remove(std::path::Path::new(&format!("{ROOT}/b.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    assert_eq!(entries(&index), BTreeMap::from([(("alpha".to_owned(), a), 1)]));
}

#[test]
fn a_load_finishing_after_its_note_left_indexes_nothing() {
    let (fs, mut vault) = vault(&["a.md", "b.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let b = id(&vault, "b.md");
    fs.remove(std::path::Path::new(&format!("{ROOT}/b.md")));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("a MemFs rescan cannot fail");
    };
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, b, "late");
    assert!(entries(&index).is_empty());
}
```

A note leaves the vault as in `index/corpus_tests.rs`: `MemFs::remove`, then a rescan.

Create `crates/index/src/content/tests.rs`:

```rust
use std::sync::Arc;

use super::NoteContent;
use crate::Source;

#[test]
fn parse_computes_the_links_and_the_words_of_the_text() {
    let content = NoteContent::parse(Arc::from("See [[plan]], Été"));
    assert_eq!(*content.source(), Source::parse("See [[plan]], Été"));
    assert_eq!(
        content
            .words
            .iter()
            .map(|(word, count)| (word.to_owned(), count))
            .collect::<Vec<_>>(),
        [
            ("ete".to_owned(), 1),
            ("plan".to_owned(), 1),
            ("see".to_owned(), 1)
        ]
    );
    assert_eq!(&*content.text, "See [[plan]], Été");
}
```

- [ ] **Step 2: Run them to see them fail.**

Run: `cargo test -p inkra-index`
Expected: compile errors: `NoteContent`, `NoteIndex::words`, and `load` with the new `insert`.

- [ ] **Step 3: `NoteContent`.** Create `crates/index/src/content.rs`:

```rust
use std::sync::Arc;

use crate::source::Source;
use crate::words::NoteWords;

/// What the index keeps of a note's text. Parse it off the foreground: `insert` only stores it.
pub struct NoteContent {
    pub(crate) source: Source,
    pub(crate) words: NoteWords,
    pub(crate) text: Arc<str>,
}

impl NoteContent {
    #[must_use]
    pub fn parse(text: Arc<str>) -> Self {
        Self {
            source: Source::parse(&text),
            words: NoteWords::parse(&text),
            text,
        }
    }

    #[must_use]
    pub fn source(&self) -> &Source {
        &self.source
    }
}

#[cfg(test)]
mod tests;
```

In `lib.rs`: `mod content;` and `pub use content::NoteContent;`, in alphabetical order.

- [ ] **Step 4: `NoteIndex`.** In `crates/index/src/index.rs`:

Imports: add `use crate::content::NoteContent;` and `use crate::words::{Words, WordsSnapshot};`.

Field, after `corpus: Corpus,`:

```rust
    words: Words,
```

In `set_notes`, after the `for key in stems { … }` loop and before `previous`:

```rust
        self.words.remove_all(&gone);
```

Replace `insert` with:

```rust
    /// Replaces `id`'s links, text and words. Ignored for an id that is not a note, so a load
    /// finishing after its note was trashed leaves nothing behind.
    pub fn insert(&mut self, id: EntryId, content: NoteContent) {
        if !self.notes.contains(id) {
            return;
        }
        self.remove(id);
        let NoteContent {
            source,
            words,
            text,
        } = content;
        for (at, link) in source.links.iter().enumerate() {
            if let Target::Note { path, .. } = &link.target {
                self.by_stem
                    .entry(Key::stem(path))
                    .or_default()
                    .push((id, at));
            }
        }
        self.sources.insert(id, source);
        self.corpus.set(self.notes.order(id), Some(text));
        self.words.set(id, words);
    }
```

In `remove`, as its first line, before `self.corpus.set(…)`:

```rust
        self.words.remove(id);
```

After `corpus()`:

```rust
    /// The words of every loaded note. Cloning is an `Arc` bump; a later write never reaches it.
    #[must_use]
    pub fn words(&self) -> WordsSnapshot {
        self.words.snapshot()
    }
```

At the end of the file, add `#[cfg(test)] mod words_tests;` beside the other test modules.

`set_notes` already builds `gone` from `self.sources.retain`: a note has words only if it was inserted, and `insert` always stores a source, so `gone` covers every note with words.

- [ ] **Step 5: Call sites.** `test_support.rs`:

```rust
/// `id`'s links, text and words, as a load gives them.
pub(crate) fn load(index: &mut NoteIndex, id: EntryId, text: &str) {
    index.insert(id, NoteContent::parse(text.into()));
}
```

and in `corpus()`: `index.insert(*id, NoteContent::parse((*text).into()));`. Update the imports (`NoteContent` in, `Source` out if unused).

Every other call of the old `insert` takes a `NoteContent`:

- `index/corpus_tests.rs:97`: `index.insert(id(&vault, "gone.md"), NoteContent::parse(text))`; the `Weak` still watches the text, which `NoteContent` moves in.
- `index/model_tests.rs:72`: `index.insert(id, NoteContent::parse(text.as_str().into()))`. `:284`: `index.insert(*source, NoteContent::parse(text.as_str().into()))`; `check` keeps its own `Source::parse` of the same text.
- `benches/index.rs:74`: `index.insert(id, NoteContent::parse(text.into()))`.
- `benches/names.rs:145`, `:176`, `:219`, `:236`: `NoteContent::parse(…)` of the same text. At `:170`, `sources` becomes the texts (`Vec<Arc<str>>`), and each insert parses its text; the re-insert at `:219` runs outside the timed part already.
- `benches/search.rs:92` and `:203`: `index.insert(id, NoteContent::parse(Arc::clone(&text)))`. In the snapshot group (`:149`), parse inside the `iter_custom` loop but before `Instant::now()`, so the timed part stays the store alone:

```rust
            for _ in 0..iters {
                let held = index.corpus();
                let content = NoteContent::parse(Arc::clone(&text));
                let start = Instant::now();
                index.insert(id, content);
                total += start.elapsed();
                drop(held);
            }
```

Run: `cargo build -p inkra-index --all-targets`
Expected: builds. Then `rg -n 'Source::(parse|default)\(' crates/index/benches crates/index/src/index crates/index/src/test_support.rs` lists no call that feeds `insert`.

- [ ] **Step 6: The app compiles.** Slice 5 replaced `work` with `load_more`, `drain` and `apply_landed`, in `crates/app/src/index/loading.rs`.

In `load_more`'s `Priority::Low` task, the load becomes:

```rust
                    let loaded = fs
                        .load(&path)
                        .await
                        .map(|loaded| NoteContent::parse(loaded.text.into()));
```

`Landed::loaded` becomes `inkra_vault::Result<NoteContent>`; in `apply_landed`, `Ok((source, text)) => self.apply(id, seq, source, text, cx)` becomes `Ok(content) => self.apply(id, seq, content, cx)`. Imports: `NoteContent` in, `Source` and (if unused) `Arc` out.

In `crates/app/src/index.rs`, `saved` becomes:

```rust
    fn saved(&mut self, _: Entity<NoteStore>, event: &NoteSaved, cx: &mut Context<Self>) {
        if self.vault.is_none() {
            return;
        }
        let id = event.id;
        let seq = self.next_seq(id);
        self.indexed.insert(id, event.stamp.mtime);
        let text = Arc::clone(&event.text);
        let parsed = cx.background_spawn(async move { NoteContent::parse(text) });
        cx.spawn(async move |this, cx| {
            let content = parsed.await;
            Self::timed_turn(&this, cx, |this, cx| this.apply(id, seq, content, cx));
        })
        .detach();
    }
```

`apply` takes `content: NoteContent` in place of `source` and `text`, and calls `self.index.insert(id, content)`. Imports: `NoteContent` in, `Source` out.

Tests: `crates/app/src/index/loading_tests.rs:112` sends `Ok(NoteContent::parse(Arc::from(text)))`; `crates/app/src/index/tests.rs:219`, `:523` and `:552` call `sync.apply(a, seq, NoteContent::parse(Arc::from(…)), cx)`.

Run: `cargo test -p inkra-index -p inkra-app`
Expected: every test passes, the new ones included.

- [ ] **Step 7: Commit Tasks 7.1–7.4.**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/index crates/app/src/index.rs crates/app/src/index
git commit -m "Index: the word index, fed by NoteContent on every load and save"
```

### Task 7.5: `lookup` and its oracle

**Files:** create `crates/index/src/words/lookup.rs`, `words/model_tests.rs`; modify `words.rs`, `words/tests.rs`, `lib.rs`, `test_support.rs`.

**Interfaces:**
- Consumes: `WordsSnapshot` (its shards' `matches`), `NoteQuery::terms`.
- Produces: `#[must_use] pub fn WordsSnapshot::lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Lookup`; `pub struct inkra_index::Lookup` (`Debug`, `Default`) with `#[must_use] pub fn get(&self, term: usize, id: EntryId) -> Option<TermHit>` and `#[must_use] pub fn terms(&self) -> usize`; `pub struct inkra_index::TermHit { pub count: u32, pub whole: bool }` (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`).

- [ ] **Step 1: Write the failing tests.** Add a helper to `crates/index/src/test_support.rs`:

```rust
/// An index of one note per text, in the given order; `None` is a note not loaded.
pub(crate) fn indexed(texts: &[Option<&str>]) -> (Vec<EntryId>, NoteIndex) {
    let files: Vec<String> = (0..texts.len()).map(|n| format!("n{n:03}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
    for (id, text) in ids.iter().zip(texts) {
        if let Some(text) = text {
            load(&mut index, *id, text);
        }
    }
    (ids, index)
}
```

and make `corpus()` call it: `let (ids, index) = indexed(texts); (ids, index.corpus())`.

Append to `crates/index/src/words/tests.rs`:

```rust
use std::sync::atomic::AtomicBool;

use crate::test_support::indexed;
use crate::{Lookup, TermHit};

fn look(texts: &[Option<&str>], query: &str) -> (Vec<EntryId>, Lookup) {
    let (ids, index) = indexed(texts);
    let found = index
        .words()
        .lookup(&NoteQuery::new(query), &AtomicBool::new(false));
    (ids, found)
}

#[test]
fn a_term_finds_every_word_it_starts_and_counts_them() {
    let (ids, found) = look(&[Some("projet projets Projet"), Some("pro"), Some("apro")], "pro");
    assert_eq!(found.terms(), 1);
    assert_eq!(found.get(0, ids[0]), Some(TermHit { count: 3, whole: false }));
    assert_eq!(found.get(0, ids[1]), Some(TermHit { count: 1, whole: true }));
    assert_eq!(found.get(0, ids[2]), None, "a term matches the start of a word only");
}

#[test]
fn accents_and_case_fold_both_ways_in_lookup() {
    let (ids, found) = look(&[Some("Été"), Some("ete")], "ÉTÉ");
    assert_eq!(found.get(0, ids[0]), Some(TermHit { count: 1, whole: true }));
    assert_eq!(found.get(0, ids[1]), Some(TermHit { count: 1, whole: true }));
}

#[test]
fn each_term_is_resolved_on_its_own() {
    let (ids, found) = look(&[Some("plan été"), Some("plan")], "plan ete");
    assert_eq!(found.terms(), 2);
    assert!(found.get(0, ids[1]).is_some());
    assert_eq!(found.get(1, ids[1]), None);
    assert_eq!(found.get(2, ids[0]), None, "a term past the last is absent, not a panic");
}

#[test]
fn a_note_not_loaded_is_absent() {
    let (ids, found) = look(&[None, Some("plan")], "plan");
    assert_eq!(found.get(0, ids[0]), None);
}

#[test]
fn a_set_cancel_returns_an_empty_lookup() {
    let (_ids, index) = indexed(&[Some("plan")]);
    let found = index
        .words()
        .lookup(&NoteQuery::new("plan"), &AtomicBool::new(true));
    assert_eq!(found.terms(), 0);
}

#[test]
fn an_empty_query_looks_up_nothing() {
    let (_ids, found) = look(&[Some("plan")], "  ");
    assert_eq!(found.terms(), 0);
}
```

Create `crates/index/src/words/model_tests.rs`:

```rust
//! `lookup` against a model that re-reads every text. It shares `fold` with the code under
//! test (S1) and splits words with its own `str::split`.

use std::sync::atomic::AtomicBool;

use proptest::prelude::*;
use regex_syntax::is_word_character;

use inkra_vault::EntryId;

use super::{NoteWords, TERM_MAX_CHARS, Words, cap, fold};
use crate::test_support::{id, vault};
use crate::{Lookup, NoteQuery, TermHit};

const PARTS: &[&str] = &[
    "a", "e", "é", "e\u{301}", "É", "E", "ß", "ss", "\u{212A}", "k", "İ", "i", "ı", "한", "\u{1112}",
    "'", "’", "-", "_", "1", "🎉", "\u{301}", " ", "\n", ".", "l'été", "aaaaaaaaaaaaaaaaaaaa",
    "ς", "σ", "Σ", "ſ", "s", "\u{fe0f}\u{20e3}", "\u{200d}",
];

fn text(len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PARTS), 0..len).prop_map(|parts| parts.concat())
}

/// The folded, capped, distinct terms of `query`, as S1 and this plan define them.
fn model_terms(query: &str) -> Vec<String> {
    let typed: String = query.chars().take(64).collect();
    let mut terms: Vec<String> = Vec::new();
    for word in typed.split(|c: char| !is_word_character(c)) {
        let folded = fold(word);
        let term = cap(&folded, TERM_MAX_CHARS).to_owned();
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

/// Every whole word of `text`, folded and never capped.
fn model_words(text: &str) -> Vec<String> {
    text.split(|c: char| !is_word_character(c))
        .map(|word| fold(word).into_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

fn model_hit(text: &str, term: &str) -> Option<TermHit> {
    let mut hit = TermHit { count: 0, whole: false };
    for word in model_words(text) {
        if word.starts_with(term) {
            hit.count += 1;
            hit.whole |= word == term;
        }
    }
    (hit.count > 0).then_some(hit)
}

#[derive(Debug, Clone)]
enum Op {
    Set(usize, String),
    Remove(usize),
    Merge,
    Hold,
}

fn op(notes: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..notes, text(60)).prop_map(|(n, text)| Op::Set(n, text)),
        1 => (0..notes).prop_map(Op::Remove),
        1 => Just(Op::Merge),
        1 => Just(Op::Hold),
    ]
}

proptest! {
    /// Across writes, tombstones and merges, in 1 to 3 shards so notes share them, `lookup`
    /// answers as the model does, and a held snapshot keeps its first answer.
    #[test]
    fn lookup_agrees_with_a_model(
        shards in 1..4usize,
        ops in prop::collection::vec(op(6), 0..24),
        query in text(12),
    ) {
        let files: Vec<String> = (0..6).map(|n| format!("n{n}.md")).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_fs, vault) = vault(&files);
        let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
        let mut words = Words::with_shards(shards);
        let mut model: Vec<Option<String>> = vec![None; ids.len()];
        let mut held = Vec::new();
        let query = NoteQuery::new(&query);
        let terms = model_terms(&query_text);
        for op in ops {
            match op {
                Op::Set(n, text) => {
                    words.set(ids[n], NoteWords::parse(&text));
                    model[n] = Some(text);
                }
                Op::Remove(n) => {
                    words.remove(ids[n]);
                    model[n] = None;
                }
                Op::Merge => words.merge_all(),
                Op::Hold => {
                    let snap = words.snapshot();
                    let answer = answers(&snap.lookup(&query, &AtomicBool::new(false)), &ids, terms.len());
                    held.push((snap, answer));
                }
            }
            let found = words.snapshot().lookup(&query, &AtomicBool::new(false));
            prop_assert_eq!(found.terms(), terms.len());
            for (t, term) in terms.iter().enumerate() {
                for (id, text) in ids.iter().zip(&model) {
                    let expected = text.as_deref().and_then(|text| model_hit(text, term));
                    prop_assert_eq!(found.get(t, *id), expected, "term {:?} in {:?}", term, text);
                }
            }
            for (snap, answer) in &held {
                let again = answers(&snap.lookup(&query, &AtomicBool::new(false)), &ids, terms.len());
                prop_assert_eq!(&again, answer);
            }
        }
    }
}

/// Every (term, note) answer of `found`, in order.
fn answers(found: &Lookup, ids: &[EntryId], terms: usize) -> Vec<Option<TermHit>> {
    (0..terms)
        .flat_map(|t| ids.iter().map(move |id| found.get(t, *id)))
        .collect()
}
```

The block above is the shape; the implementer fixes its details (the query's text is drawn once as `query_text` and `NoteQuery::new(&query_text)` built from it; imports: `Words`, `NoteWords`, `Lookup`, `TermHit`, `test_support::{id, vault}`). `PROPTEST_CASES=20000` runs it at the gate.

In `words.rs`, add `#[cfg(test)] mod model_tests;` after `mod tests;`.

The alphabet holds `aaaaaaaaaaaaaaaaaaaa`, so a text of 60 parts can build a word of 80 chars and more: the model never caps words, which checks the `WORD_MAX_CHARS` argument. `model_hit` counts every occurrence of a matched word, which is what `TermHit::count` sums.

- [ ] **Step 2: Run them to see them fail.**

Run: `cargo test -p inkra-index words`
Expected: compile error, `lookup`, `Lookup` and `TermHit` are not defined.

- [ ] **Step 3: Implement.** Create `crates/index/src/words/lookup.rs`:

```rust
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use inkra_vault::EntryId;

use super::query::NoteQuery;
use super::shard::WordsSnapshot;

/// How a note holds one term: how many of its words start with it, and whether one equals it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermHit {
    pub count: u32,
    pub whole: bool,
}

/// A query's terms resolved in the word index, once per keystroke; every range reads it.
#[derive(Debug, Default)]
pub struct Lookup {
    terms: Vec<HashMap<EntryId, TermHit>>,
}

impl Lookup {
    /// How the note holds the query's `term`-th term; `None` when it holds no word starting with it.
    #[must_use]
    pub fn get(&self, term: usize, id: EntryId) -> Option<TermHit> {
        self.terms.get(term)?.get(&id).copied()
    }

    /// The query's terms, or 0 when the lookup was cancelled.
    #[must_use]
    pub fn terms(&self) -> usize {
        self.terms.len()
    }
}

impl WordsSnapshot {
    /// Each term's notes: per shard, one ordered range of its frozen words and one per fresh note.
    #[must_use]
    pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Lookup {
        let mut terms = Vec::with_capacity(query.terms().len());
        for term in query.terms() {
            let mut notes: HashMap<EntryId, TermHit> = HashMap::new();
            for shard in self.0.iter() {
                if cancel.load(Ordering::Relaxed) {
                    return Lookup::default();
                }
                for (id, word, count) in shard.matches(term) {
                    let hit = notes.entry(id).or_insert(TermHit {
                        count: 0,
                        whole: false,
                    });
                    hit.count = hit.count.saturating_add(count);
                    hit.whole |= word == &**term;
                }
            }
            terms.push(notes);
        }
        Lookup { terms }
    }
}
```

`WordsSnapshot`'s field and `Shard::matches` are `pub(super)` since Task 7.3, so `lookup.rs` reads them inside `words` only.

In `words.rs`, add `mod lookup;` and `pub use lookup::{Lookup, TermHit};`. In `lib.rs`: `pub use words::{Lookup, NoteQuery, TermHit, WordsSnapshot};`.

- [ ] **Step 4: Run the tests, then the oracle at the gate's case count.**

Run: `cargo test -p inkra-index words`
Expected: all pass.

Run: `PROPTEST_CASES=20000 cargo test -p inkra-index --release words::model_tests`
Expected: 1 passed. Commit any new `.proptest-regressions` file.

- [ ] **Step 5: Commit.**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index
git add crates/index
git commit -m "Words: lookup resolves each term by one range per shard, with its oracle"
```

### Task 7.6: The updates oracle, and `IndexSync` tests

**Files:** modify `crates/index/src/index/model_tests.rs`, `crates/app/src/index.rs`, `crates/app/src/index/tests.rs`; create `crates/app/src/index/words_tests.rs`.

- [ ] **Step 1: The updates oracle.** In `crates/index/src/index/model_tests.rs`, `the_corpus_agrees_with_a_model_across_vault_changes` applies random `Create`, `Delete`, `Rename`, `Load`, `Unload`, `Build` and `Hold` operations and keeps `model: HashMap<EntryId, String>`, the texts of exactly the notes `NoteIndex` indexes (`Load` records a text only for a note of the current table, as `insert` stores one). After each operation's check of the corpus, add:

```rust
            let fresh = {
                let mut words = crate::words::Words::default();
                for (id, text) in &model {
                    words.set(*id, crate::words::NoteWords::parse(text));
                }
                words.snapshot().entries()
            };
            prop_assert_eq!(index.words().entries(), fresh);
            for (snap, seen) in &held_words {
                prop_assert_eq!(&snap.entries(), seen);
            }
```

`Hold` also pushes `(index.words(), index.words().entries())` onto `held_words`. Add `Op::Merge`, drawn as often as `Build`, which calls a `#[cfg(test)] pub(crate) fn NoteIndex::merge_words(&mut self)` (it calls `Words::merge_all`), so the oracle crosses merges at random points; with 256 shards and a handful of notes, `Build` and `Unload` also leave tombstones in merged shards. Widen both `Load` strategies from `"[a-cé]{0,6}"` to `"[a-céÉ' ]{0,12}"`, so a text holds several words, a repeat, and a case to fold.

Run: `PROPTEST_CASES=20000 cargo test -p inkra-index --release the_corpus_agrees_with_a_model`
Expected: 1 passed.

- [ ] **Step 2: `IndexSync` tests.** In `crates/app/src/index/tests.rs`, make `saved_edit` `pub(super)`. In `crates/app/src/index.rs`, add `#[cfg(test)] #[allow(clippy::expect_used)] mod words_tests;` beside the other test modules. Create `crates/app/src/index/words_tests.rs`:

```rust
use std::sync::atomic::AtomicBool;

use gpui::{Entity, TestAppContext};
use inkra_index::{NoteQuery, TermHit};
use inkra_vault::EntryId;

use super::IndexSync;
use super::tests::{create, indexed_vault, saved_edit};
use crate::test_support::entry_id;

/// The notes of `ids` holding a word that starts with `term`, with their `TermHit`.
pub(super) fn looked_up(
    cx: &mut TestAppContext,
    links: &Entity<IndexSync>,
    ids: &[EntryId],
    term: &str,
) -> Vec<(EntryId, TermHit)> {
    links.read_with(cx, |links, _| {
        let found = links
            .index()
            .words()
            .lookup(&NoteQuery::new(term), &AtomicBool::new(false));
        ids.iter()
            .filter_map(|id| Some((*id, found.get(0, *id)?)))
            .collect()
    })
}

#[gpui::test]
async fn a_load_and_a_save_index_their_words(cx: &mut TestAppContext) {
    let (_fs, notes, links) = indexed_vault(cx, &[("a.md", "Loaded été"), ("b.md", "")]);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    assert_eq!(
        looked_up(cx, &links, &[a, b], "ete"),
        [(a, TermHit { count: 1, whole: true })]
    );

    saved_edit(cx, &notes, "b", "saved Été été").await;

    assert_eq!(
        looked_up(cx, &links, &[a, b], "ete"),
        [
            (a, TermHit { count: 1, whole: true }),
            (b, TermHit { count: 2, whole: true })
        ]
    );
}

#[gpui::test]
fn a_load_finishing_after_a_trash_indexes_no_words(cx: &mut TestAppContext) {
    let (fs, notes, links) = indexed_vault(cx, &[("a.md", "old"), ("b.md", "")]);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    let gate = fs.block_load("/vault/a.md");
    fs.insert_file("/vault/a.md", "changed");
    create(cx, &notes, "c");
    let trashed = notes.update(cx, |notes, cx| notes.trash(a, cx));
    cx.run_until_parked();
    drop(trashed);
    gate.release();
    cx.run_until_parked();

    assert_eq!(looked_up(cx, &links, &[a], "changed"), []);
    assert_eq!(looked_up(cx, &links, &[a], "old"), []);
}
```

Then extend `random_operations_leave_the_index_as_a_fresh_build` in `tests.rs`: each row of `Snapshot` gains, for its note, `looked_up` over the pool terms `["a", "b", "c", "e", "n", "r", "s", "zz"]` (the row keeps one `Vec<Option<TermHit>>` per note, one entry per term). The texts are link lists such as `[[n1]] [[r2]]`, whose words are `n1` and `r2`, so these prefixes reach every note that has a text.

Run: `cargo test -p inkra-app index::`
Expected: all pass, the new ones included.

- [ ] **Step 3: Commit.**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/index crates/app/src/index.rs crates/app/src/index
git commit -m "Words: equal a fresh build after any update; IndexSync feeds them"
```

### Task 7.7: The `words` bench, the figures, mutants

**Files:** create `crates/index/benches/words.rs`; modify `crates/index/Cargo.toml`, the smart quick open spec.

- [ ] **Step 1: The bench target.** In `crates/index/Cargo.toml`, after the `search` bench:

```toml
[[bench]]
name = "words"
harness = false
```

- [ ] **Step 2: The corpus.** `crates/index/benches/words.rs` generates 50,000 notes of 1 to 5 KB whose words follow a Zipf law over a vocabulary of 30,000 generated words, since the `search` bench's 40 words would give a vocabulary no vault has. Reuse the `search` bench's `Lcg` and `entries()` code (copy them; benches share no module).

```rust
const VOCABULARY: usize = 30_000;
const SYLLABLES: &[&str] = &[
    "pro", "jet", "ré", "u", "nion", "é", "té", "ca", "fé", "plan", "da", "mi", "no", "ta",
    "lu", "ver", "sa", "ki", "ou", "ain", "ment", "tion", "ble", "que", "é", "è", "ç",
];

/// Word `n` of the vocabulary: 1 to 4 syllables, some accented; word 0 is the most frequent.
fn word(n: usize) -> String {
    let mut lcg = Lcg(u64::try_from(n).unwrap_or(0) * 7 + 3);
    (0..1 + lcg.next(4))
        .map(|_| SYLLABLES[lcg.next(SYLLABLES.len())])
        .collect()
}

/// The running sum of the harmonic weights 1/1, 1/2, … 1/`VOCABULARY`.
fn cumulative() -> Vec<f64> {
    let mut sum = 0.0;
    (1..=VOCABULARY)
        .map(|rank| {
            sum += 1.0 / f64::from(u32::try_from(rank).unwrap_or(u32::MAX));
            sum
        })
        .collect()
}

/// A Zipf rank in `0..VOCABULARY`, by inverse transform on `cumulative`.
fn zipf(lcg: &mut Lcg, cumulative: &[f64]) -> usize {
    let total = cumulative[cumulative.len() - 1];
    let draw = u32::try_from(lcg.next(1 << 30)).unwrap_or(0);
    let x = f64::from(draw) / f64::from(1u32 << 30) * total;
    cumulative.partition_point(|&c| c < x).min(cumulative.len() - 1)
}
```

Clippy's cast lints are met with `try_from` and `f64::from`, never with `as` and never with an `allow`. Each note is lines of 8 to 20 Zipf words with `.`, `,` and `'` between some, and one note in 1,000 holds the rare word `zanzibar`.

- [ ] **Step 3: The cases.** Each case prints what it measures; criterion's middle estimate is the figure. Budgets from the smart quick open spec.

| Group / case | What is timed | Budget |
|---|---|---|
| `parse/one note of 3 KB` | `NoteContent::parse` | none; × 50,000 gives the background cost added to startup |
| `build/50k notes` | 50,000 `insert`s of parsed contents into an empty index | none; the foreground total of a startup load, also printed per note |
| `build/50k notes, 40 words` | the same over the startup vault's 40-word vocabulary (`benches/search.rs` `WORDS`) | none; the foreground the startup figure pays |
| `insert/one note, a snapshot held` | one `insert` while a `words()` snapshot is held (`iter_custom`, as the search bench's snapshot case) | 2 ms |
| `insert/one note, no snapshot` | the same without the snapshot | 2 ms |
| `set_notes/nothing gone` | `set_notes` with the same notes | 8 ms |
| `set_notes/5,500 gone` | `set_notes` without the first 5,500 notes, all loaded; the returned set is dropped outside the timer, as the app frees it off the foreground, and the 5,500 are re-inserted outside it | 8 ms |
| `set_notes/5,500 gone, a snapshot held` | the same while a `words()` snapshot is held: every shard is touched, so every shard is copied | 8 ms |
| `insert/one note, both snapshots held` | one `insert` while a `words()` and a `corpus()` snapshot are held | 2 ms |
| `insert/a 1 MB base64 image, a snapshot held` | one `insert` of a note holding a 1 MB base64 paste (tens of thousands of distinct words), a `words()` snapshot held | 2 ms; 8 ms hard, since one note's apply is never split |
| `lookup/1 letter "e"` | `lookup` on one core | recorded; slice 8 splits it or caps it if the end-to-end needs it |
| `lookup/top word` | word 0 of the Zipf vocabulary | recorded |
| `lookup/mid word` | word 1,000 | recorded |
| `lookup/rare word "zanzibar"` | | recorded |
| `lookup/two words` | word 0 and word 1,000 | recorded |
| `lookup/no match "zzzz"` | | recorded |

The `build` cases also time each `insert` alone and print its p50, p99 and max over the 50,000: the max is a merge at `FRESH_MAX`, and it must stay under 2 ms. `SHARDS` and `FRESH_MAX` are set here: run the `build`, `insert` and `lookup` cases for `FRESH_MAX` in {8, 16, 32, 64} at `SHARDS` = 256, then the best one at 64 and 1,024 shards, editing the constants between runs; record every run in the spec and keep the pair with the lowest `lookup/1 letter` whose merge max stays under 2 ms and whose `build` stays under 20 % of the startup figure.

Print once, before the groups: `words: {n} notes, heap_bytes {estimate}, RSS {before} → {after} kB`, where RSS is read with `std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()])` before and after building the index.

- [ ] **Step 4: Run and record.**

Run: `cargo bench -p inkra-index --bench words`
Expected: every case prints; the budgeted ones meet their budget, or Task 7.8 runs.

Record in the smart quick open spec, under Performance, a "Measured, slice 7" table as the search spec's: machine, core count, command, each figure, budget, met or missed; the RSS delta against the 80 MB alarm line (the alarm reads the RSS; `heap_bytes` undercounts node and allocator overhead and is recorded beside it), for the Zipf corpus and the 40-word one; `parse` × 50,000 as the added background cost, compared with 20 % of the startup figure recorded in slice 5.

- [ ] **Step 5: The word index in the launch line.** `Step::Indexed` in `crates/app/src/timeline.rs` gains `words_bytes: usize`, printed after the corpus: `indexed {t} ({n} notes, corpus {b} bytes, words {w} bytes, longest turn {t})`. `IndexSync::mark_indexed` fills it from `self.index.words().heap_bytes()`. Update `timeline/tests.rs` to the new line. So the app gives the real vault's memory figure beside the bench's.

- [ ] **Step 6: The startup figure again.** The machine idle (load average under 2, no process over 20 % of a core), and `ioreg -n Root -d1 | grep CGSSessionScreenIsLocked` printing nothing or `No`; else leave the figures PENDING for the orchestrator. Build `cargo build -p inkra-app --release`, then, in one sitting, four runs each:

```bash
/tmp/launch-figure.sh /tmp/inkra-app-final "$TMPDIR/inkra-startup-50k" 4
/tmp/launch-figure.sh target/release/inkra-app "$TMPDIR/inkra-startup-50k" 4
/tmp/launch-figure.sh /tmp/inkra-app-final "$TMPDIR/inkra-startup-50k-flat" 4
/tmp/launch-figure.sh target/release/inkra-app "$TMPDIR/inkra-startup-50k-flat" 4
```

`/tmp/inkra-app-final` is slice 6's release build. Record runs 2–4 of each in the spec, beside slice 6's gate table: the budget is `indexed` after ≤ 1.2 × `indexed` before, per vault. Record the longest turn too: it must stay under 8 ms.

- [ ] **Step 7: The decisions in the spec.** Add the "Built in slice 7" paragraphs (the decisions list above) under S1 and S2. Edit S1's own sentence too: the query is split into words as the text is, then each word is folded (not folded, then split). Add two **Corrected** notes: in S1, the fold ends with one char per simple case folding orbit, from `regex-syntax`'s tables (the `ς`/`σ` finding); in S2, replacing the "Content" bullet, the F store (frozen, fresh, dead; frozen notes keep no word list; removal is a tombstone the next merge purges), with the red team's estimate of the first layout as the reason. In S2's "Prefix lookup", say what is built: one ordered range per shard, each bounded to the words with the term as prefix; no shard's vocabulary is scanned. In the Evolution table's "Persisted word index" row, add the seam the blue team found: `EntryId` is minted per scan, so a persisted index keys notes by a stable key (path or `FileIdentity`) and remaps to the scan's ids at load, re-sharding by the new ids.

- [ ] **Step 8: Mutants** (the orchestrator runs them at the gate; the lead runs them once here to fix misses early), on `crates/index/src/words/**`, `crates/index/src/content.rs` and the slice's diff of `crates/index/src/index.rs`, and on `crates/app/src/index.rs` with the app command. Zero missed; an equivalent goes to `.cargo/mutants.toml` with its reason.

- [ ] **Step 9: Commit.**

Run: `./scripts/check.sh`
Expected: `==> all checks passed`.

```bash
cargo fmt -p inkra-index -p inkra-app
git add crates/index/Cargo.toml crates/index/benches/words crates/app/src docs .cargo/mutants.toml
git commit -m "Words: the bench, the figures, the slice 7 decisions in the spec"
```

### Task 7.8: Levers, only for a budget Task 7.7 misses

Each lever keeps the public API and passes Task 7.5's and Task 7.6's oracles unchanged, with a bench figure before and after in the spec. The lead applies only the ones whose trigger fired, and asks the orchestrator first if a lever is not in this list.

- **A merge at `FRESH_MAX` over 2 ms on the foreground → merge off the foreground and swap** (orchestrator, 2026-09-26), as `NoteSet` is built: the shard's `Frozen::merge` runs on the background executor from a snapshot of the shard, and the foreground swaps the result in only if the shard's fresh and dead notes still include what it merged (else the result is dropped and the merge retried). This adds an API to `NoteIndex` and a task to `IndexSync`; the lead asks the orchestrator for its shape first.
- **`insert/one note, a snapshot held` over 2 ms → a smaller `FRESH_MAX`**, then more shards; record each figure and the `lookup` cost it adds.
- **`set_notes/5,500 gone` over 8 ms** cannot come from the words under F (tombstones, no merge); profile it and report.
- **Memory over the 80 MB alarm line.** On the startup vault: stop and report. On the Zipf stress case only: record it, and add delta-coded postings to the spec's Evolution table with its trigger (orchestrator, 2026-09-26); the storage is not compacted in this slice.
- **`lookup/1 letter` over 8 ms** is recorded, not fixed here: S3's one-letter cap and a split of `lookup` across shards are slice 8's choice, made with the end-to-end figure.

---

## Slice 8 — Smart quick open (detailed)

> **Fix batch, 2026-09-27.** The review's fix batch changed the design below; the stale lines are fixed in place.
> - `find_notes`, `merge_notes` and `excerpt` take one `IndexSnapshot` (names, corpus, words), built only by `NoteIndex::snapshot()`; the pair check and its fallback are gone.
> - `find_notes` returns `Vec<Ranked>`, a rank only; `merge_notes(snapshot, chunks, query, limit)` builds the `NoteMatch` of the merged top alone.
> - The words store's ordinals and fresh maps are sharded (64 each), so a write under a held snapshot copies one shard.
> - A run is a user search or a background refresh (`Run`), and `Scan` tracks the scan in flight: a background refresh never cancels it, never moves the selection or the scroll, and a keystroke waits no debounce behind it.
> - Rows apply first; excerpts are computed only for the rows on screen, one task per row, as they are ready.
> - Each row's text (`RowText`) is built once, when its row or its excerpt lands, and stored with the row.
> - The exact name ranks first in its group, right after the group (S3 amendment, decided by the user on 2026-09-27).
> - The second line shows the first prose line, its markdown markup dropped and its highlights mapped through the drop (S4 amendment, decided by the user on 2026-09-27).

Spec: smart quick open S3, S4, S5, Performance (`find_notes`, end to end), Testing (oracle, split, excerpt, app); search spec D7 (a refresh never cancels a running scan of the same query), D8, D9; open-first spec O5. Branch `search/8-smart-quick-open` from `search/7-word-index`.

**Zed check** (slice lead, 2026-09-26; recorded in the spec under "What Zed does", in Task 8.6):

- `file_finder.rs` `push_new_matches` / `cmp_matches` (`:546-663`): history items that match are merged with the search matches into one list sorted by score; the currently opened file is bubbled to the **top**; history matches only on the file name (`matching_history_items`, `:675-700`), to avoid false positives from folders. Inkra keeps D2's rule instead: the note showing ranks after the notes of its group that tie with it on the exact name and keys 1–2 (S3 key 3), since opening the note already showing is a no-op. The MRU is a tie-break after the group, the exact name, whole terms and stem terms or occurrences (S3 key 4), not a score of its own: a better word match beats a recent note. Zed's name-only history rule is what S3's `Name` group gives by construction.
- `set_search_matches` (`:1097-1127`): the selected match is kept when the query did not change. Inkra already keeps the selected note by id after a refresh (slice 3).
- A worktree update in Zed calls `picker.refresh` with a 200 ms debounce (`:1003-1008`, `:1874`) and cancels the running search; a cancelled search of an unchanged query then *extends* the old matches (`extend_old_matches`, `:1112`). Inkra follows D7 instead: a refresh never cancels a running scan; it marks the query dirty and re-runs once the scan lands. Zed's extension works because its matches arrive as one stream per worktree; ours are ranked across the whole vault, so a partial list is not a prefix of the final one.
- `text_finder/delegate.rs` `render_matched_line` (`:1342-1428`): the excerpt is a bounded window around the match, not the whole line ("a minified single-line file stays cheap"), leading indentation is trimmed when the window starts at the line start, and the match is highlighted. Inkra takes all three: S4's window (60 bytes before, 100 after, within the line), the trim, and the highlight. Zed computes the excerpt at render time per visible row from a buffer snapshot; Inkra computes it off the foreground, after the merge, for the rows on screen (decision below).
- `picker.rs` `render_footer` (`:408`): `Option<AnyElement>`, `None` by default. Inkra's `PickerDelegate` takes the same shape.

**Checked against the code on 2026-09-26 (slice lead, Flow step 2).** The task list above predates slices 5–7. What changed:

- `NoteQuery::new(text)` has one argument (slice 7); `Lookup::get(term, id) -> Option<TermHit>` and `term_count()`; `WordsSnapshot::lookup` returns `Option<Lookup>` (`None`: cancelled).
- `Corpus` keeps its slots in chunks of 256 behind `Arc`s (`corpus.rs`), so a slot of mtimes beside the texts is one more field of the slot, copied with its chunk.
- `NoteContent::parse(text)` runs in `IndexSync::load_more`'s task (`index/loading.rs`) and in `saved`'s parse task (`index.rs`); `Landed.loaded` holds the `NoteContent`, so the load's `Loaded::stamp` must be read in the task.
- `NoteSet::build` has 22 call sites in index tests and benches, and `Built::new` in `app/src/index.rs`; `test_support::entries` feeds most of them.
- The picker's list height is `UI_LINE_HEIGHT × rows` (`picker/render.rs`), fixed for every delegate.
- The backlinks panel's `seen`/`throttled`/`BUILDING_REFRESH` (`backlinks/mod.rs`) is the refresh logic to share.
- `QuickOpenDelegate::new` subscribes to `NotesChanged` only, and a `NotesChanged` calls `Picker::refresh`, which drops the pending task: that is the cancel D7 forbids.

### Plan challenge, 2026-09-27

Red and blue teams (sonnet, read-only) on this section. Each point, taken or rejected; the decisions and tasks below already integrate what was taken.

| # | Point | Answer |
|---|---|---|
| R1 | One matched 1 MB word makes a 1 MB excerpt | Taken: the window is built around the matched word's first `WORD_MAX_CHARS` chars, so an excerpt is at most about 60 + 260 + 100 bytes; test `a_one_megabyte_word_gives_a_short_excerpt` |
| R2 | Finding the first matching word is O(bytes before it), not O(window) | Taken: the scan stops after `EXCERPT_SCAN_BYTES` (1 MB, about 2–3 ms) with no second line, recorded in the spec; test and bench with the match at the **end** of a 10 MB line |
| R3 | `NotesChanged` and the notify of the same turn refresh twice | Taken: `update_matches` records `IndexWatch::seen` when it takes the snapshots, so the notify after `NotesChanged` is skipped; test `one_build_runs_one_scan` |
| R4 | `Run` cannot tell the 200 ms `NotesChanged` wait from a notify refresh | Taken: `Run::{Search, Refresh, Notify}`; `Notify` keeps the selection and waits nothing (`IndexWatch` throttles it) |
| R5 | A slot keeps a stale mtime until its reload lands | Rejected as a change, taken as a record: the mtime describes the text the index holds, so the two never disagree; the window is the reload queue, empty at startup (no text yet, so the build's mtime is used). Writing the scan's mtime at once would rank a note by an edit its text does not show yet. Test `external_edits_keep_each_old_mtime_until_the_reload` over several notes |
| R6 | `set(at, text, modified)` cannot express "clear the text, keep the mtime" | Taken: `Corpus::write(at, text, modified)` and `Corpus::clear(at)` |
| R7 | The highlight property does not check minimality | Taken: no shorter grapheme prefix folds to a string starting with the term |
| R8 | `NameWords` memory is not reported | Taken: `NamesSnapshot::heap_bytes()` and `names_bytes` in `Step::Indexed` |
| R9 | The names/corpus pair is a convention | Taken by data shape: `find_notes` and `excerpt` take one `IndexSnapshot` (names, corpus, words), built only by `NoteIndex::snapshot()`, so the names and corpus always come from one build and no runtime check or fallback exists. (The first build checked `Arc::ptr_eq` on the build's ids and tied every mtime on a mismatch; the fix batch replaced it.) |
| R10 | Notes imported together share an mtime | Taken as a record: ties fall to key 6; the spec says so |
| R11 | `Rank::order` must be the global vault order | Taken: a doc line; the split oracle pins it |
| R12 | `Built::new` is under-specified | Taken: every entry, folders included, carries its own `Entry::mtime` (a folder has one); the notes-only reload list is unchanged |
| R13 | The footer toggling may move the selection | Taken: `the_selection_survives_the_footer_leaving`. Scroll is untouched: the footer sits under the list |
| R14 | Cancel is checked only between excerpt rows | Taken: also every 64 KB inside the scan |
| B1 | `Mtime` has no order, and the vault rule forbids ordering it | Taken: `Rank` holds `Reverse(SystemTime)` read from `Mtime::0`; `Mtime` gains no `Ord`. The spec records key 5 as a heuristic: a synced file's mtime need not follow the order of edits |
| B2 | `fold`, `word_spans`, `cap` are private to `words` | Taken: `words.rs` re-exports them `pub(crate)` (Task 8.2) |
| B3 | "No single-note read" contradicts `text(order)` | Taken: "no **public** single-note read" |
| B4 | Keep `names` free of `words` with a fourth accessor | Rejected: name words are name data, built with the names in one pass; a separate accessor would add a third snapshot to pair by convention (R9). The amended rule keeps `names` from reading them |
| B5 | Search spec D1/D5 still show the 3-tuple | Taken: Task 8.6 adds "Corrected in slice 8" notes to D1 and D5 |
| B6 | Mutants scope misses new files | Taken: the gate list names `words/name_words.rs`, `window.rs`, `find/**`, `app/src/index/watch.rs`, `app/src/count.rs` and the quick-open files |
| B7 | No sequence oracle for mtimes | Taken: the corpus model in `index/model_tests.rs` tracks and asserts mtimes |
| B8 | No bench of the whole keystroke pipeline | Taken: `find.rs` gains `pipeline/<query>`: lookup, `find_notes` over the chosen split, merge, 100 excerpts |
| B9 | The Zed paragraph is accurate | Noted; the 200 ms constant (`file_finder.rs:844`) is cited |
| B10 | The code-drift section is accurate | Noted |

### Decisions this plan makes inside the spec

Each is recorded in the spec in Task 8.6, under S3, S4 or S5, "Built in slice 8".

- **Mtimes follow S3 exactly: a save updates them** (user, 2026-09-25). A corpus slot holds its text **and** its mtime: `struct Slot { text: Option<Arc<str>>, modified: Mtime }`. `Mtime` gains no `Ord` (the vault rule: never order a stamp); `Rank` orders `Mtime::0`, a `SystemTime`, and the spec calls key 5 a heuristic. Notes sharing an mtime fall to key 6. `NoteSet::build` takes `(EntryId, String, Kind, Mtime)` and keeps the notes' mtimes in vault order. `set_notes` → `Corpus::reorder` gives each slot of the new build **its previous slot's mtime when the note keeps its text, else the build's**. `insert` writes the text and `NoteContent::modified`. So a slot's mtime is always the mtime of the text it holds, or the scan's while no text is loaded; a note never lacks one (not `Option`). A rescan that sees an external edit keeps the old mtime until the reload lands with the new text: text and mtime move together. `remove` clears the text and keeps the mtime. `NoteContent::parse(text, modified: Mtime)`: a load passes `Loaded::stamp.mtime`, a save `NoteSaved::stamp.mtime`.
- **`find_notes` takes the corpus**, inside one `IndexSnapshot`: `find_notes(snapshot, lookup, range, query, cancel, limit)`. S3's signature has no corpus, but key 5 needs the mtimes, and the lego rule already names the `CorpusSnapshot` as `find`'s source for them. `names` and `corpus` must come from the same build: `IndexSnapshot` holds both, and only `NoteIndex::snapshot()` builds it, so no runtime check or fallback exists.
- **Name words are built once per build**, by `NoteSet::build`, off the foreground: for each note, the S1 words of its NFC path (`word_spans` then `fold`, capped at `WORD_MAX_CHARS`), each with its byte range in the path and whether it is in the stem. They are stored CSR-style for the whole snapshot (one folded string, one word array, one start per candidate), not a `Box` per word: 50,000 names × about 5 words would otherwise be 250,000 allocations. The type is `words::NameWords` (crate-private); `NamesSnapshot` holds it beside the D2 candidates. **The lego rule is amended**: `names` stores `NameWords` and exposes it (`words()`), never reads it; `find` reads it. Name words are not sorted (they keep path order for highlights), so they do not fire the "shared `Lexicon`" lever.
- **A term's place.** A term is placed in the name when it matches a word of the stem or folders, else in the text. S3's key 1 counts the terms matching whole **in their place** (a name word equal to the term, or `TermHit::whole`); key 2 counts, for `Name`, the terms matching a stem word; for `NameAndText` and `Text`, the sum of `TermHit::count` over the terms placed in the text. So a `Name` row never ranks by its text.
- **Highlights light the matched prefix.** In a name and in an excerpt, a matched word lights the shortest run of its graphemes whose fold starts with the term, not the whole word: `pla` lights `Pla` in `Planning`. Computed only for the rows returned, never per candidate.
- **The excerpt.** `pub fn excerpt(snapshot: &IndexSnapshot, found: &NoteMatch, query: &NoteQuery, cancel: &AtomicBool) -> Option<Excerpt>`: it reads the note's text at `found`'s vault order (checked against `found.id`) and uses `found.group`, so no vault order leaks into the public API and no **public** single-note read is added to `CorpusSnapshot`. The rows apply as soon as the merge lands; the delegate then computes the excerpts **off the foreground, for the rows on screen** (about 10), one background task per row, each checking `cancel` every 64 KB of its scan. The scan for the first matching word stops after `EXCERPT_SCAN_BYTES` (1 MB): past it the row has no second line. The window is built around the matched word's first `WORD_MAX_CHARS` chars, so a 1 MB word gives a short excerpt. S4 reads the text "for at most the rows on screen", as built, and a 10 MB note whose first matching word is at its end never costs a frame. Zed computes it at render time; its buffers are already in memory as ropes with line indexes, ours are plain strings.
- **The picker gains two delegate methods with defaults**: `fn render_footer(&self, cx) -> Option<AnyElement> { None }` and `fn row_height(&self) -> Pixels { theme::UI_LINE_HEIGHT }`. Quick open returns `2 × UI_LINE_HEIGHT`; the list height is `row_height × shown rows`.
- **The refresh helper** is `crate::index::IndexWatch` (`app/src/index/watch.rs`): it holds the `(revision, building)` last seen and the throttle task, and tells its owner to skip, refresh now, or refresh after `BUILDING_REFRESH`. The backlinks panel and quick open both use it. Quick open records `seen` when `update_matches` takes its snapshots, so the notify that follows a `NotesChanged` in the same turn is skipped.
- **The footer** reads `Indexing 12,340 of 50,000 notes` while `is_building() && !awaits_first_build()`, from `IndexSync::progress()`, with thousands separators (a 10-line helper in `app`). While `awaits_first_build()` the picker already says "Scanning the vault…" (O5), so the footer stays off.
- **Ranges.** Quick open runs `find_notes` over `names.chunks(num_cpus)`, one range per core (S5: 2–4×): Task 8.6's `find` bench compared 1×, 2× and 4×, and one per core was faster.

### File structure

| File | Status | Responsibility |
|---|---|---|
| `crates/index/src/words/query.rs` | modify | `NoteQuery::new(text, showing, recent)`, `showing()`, `recent(id)` |
| `crates/index/src/words/name_words.rs`, `name_words/tests.rs` | create | `NameWords`: every name's folded words, CSR |
| `crates/index/src/words/lookup.rs` | modify | `pub(crate)` per-ordinal access for `find` |
| `crates/index/src/corpus.rs`, `corpus/tests.rs` | modify | `Slot { text, modified }`; `reorder` takes the build's mtimes; `set` writes the mtime; `CorpusSnapshot::modified(order)`, `text(order)` (crate) |
| `crates/index/src/content.rs`, `content/tests.rs` | modify | `NoteContent::parse(text, modified)` |
| `crates/index/src/note_set.rs`, `note_set/tests.rs` | modify | the 4-tuple; mtimes; name words |
| `crates/index/src/names.rs` | modify | `NamesSnapshot` holds `NameWords` |
| `crates/index/src/index.rs` | modify | `insert` writes the mtime; `set_notes` passes the build's mtimes |
| `crates/index/src/window.rs`, `window/tests.rs` | create | the line window shared by `Hit` and the excerpt |
| `crates/index/src/search/hit.rs` | modify | `Hit::new` uses `window` |
| `crates/index/src/find.rs` | create | module list, re-exports, `Group`, `NoteMatch` |
| `crates/index/src/find/rank.rs` | create | `Rank`, placing terms, S3's order |
| `crates/index/src/find/run.rs` | create | `find_notes`, `merge_notes` |
| `crates/index/src/find/highlight.rs` | create | the matched prefix of a word, grapheme-widened |
| `crates/index/src/find/excerpt.rs` | create | `Excerpt`, `excerpt` |
| `crates/index/src/find/{tests,model_tests,excerpt_tests}.rs` | create | behaviour, the oracle, the excerpt |
| `crates/index/src/lib.rs` | modify | modules, re-exports |
| `crates/index/src/test_support.rs`, index tests, benches | modify | the 4-tuple, `NoteContent::parse(text, mtime)`, `NoteQuery::new(text, None, &[])` |
| `crates/index/benches/find.rs`, `crates/index/Cargo.toml` | create / modify | the `find` bench |
| `crates/app/src/index.rs`, `index/loading.rs` | modify | the 4-tuple; stamps into `parse` |
| `crates/app/src/index/watch.rs`, `watch/tests.rs` | create | `IndexWatch` |
| `crates/app/src/backlinks/mod.rs` | modify | uses `IndexWatch` |
| `crates/app/src/picker/{mod,render}.rs`, `picker/tests.rs` | modify | `render_footer`, `row_height` |
| `crates/app/src/quick_open/mod.rs`, `quick_open/search.rs`, `render.rs`, tests | modify / create | the new delegate, two-line rows, footer, refresh |
| `crates/app/src/count.rs` | create | `thousands(n)` for the footer |
| `crates/app/src/shot/scenes.rs` | modify | new scenes, notes with text |
| spec, `CLAUDE.md` | modify | Zed paragraph, "Built in slice 8", figures, shots |

### Task 8.1: `NoteQuery` with `showing` and `recent`; mtimes; name words

> **Reshaped at build time (slice lead, 2026-09-27).** `check.sh` builds the library without `cfg(test)`, so a field whose only reader is `find_notes` fails clippy's dead-code lint even with a test reader. Task 8.1 ships the mtimes alone (`ac8eb52`, `Index: each corpus slot holds its text's mtime`; `reorder` takes `&[Mtime]`, clippy's `needless_pass_by_value`). `NoteQuery`'s `showing` and `recent`, `NameWords`, `NamesSnapshot`'s `words`, `ids` and `heap_bytes`, and `Step::Indexed::names_bytes` move to Task 8.2, with `find_notes`, their reader. `CorpusSnapshot::modified` is `#[cfg(test)]` until 8.2 reads it; 8.2 also adds the app test that a load and a save hand their stamp's mtime to the index.

**Files:** modify `words/query.rs`, `words.rs`, `corpus.rs` (+ tests), `content.rs` (+ tests), `note_set.rs` (+ tests), `names.rs`, `index.rs`, `test_support.rs`, every index test and bench call site, `app/src/index.rs`, `app/src/index/loading.rs`, app index tests; create `words/name_words.rs`, `words/name_words/tests.rs`.

**Interfaces:**
- `pub fn NoteQuery::new(text: &str, showing: Option<EntryId>, recent: &[EntryId]) -> Self` (S3); `pub(crate) fn showing(&self) -> Option<EntryId>`; `pub(crate) fn recent(&self, id) -> Option<usize>` (newest is 0; a repeated id keeps its newest rank, as `NameQuery`).
- `pub fn NoteSet::build(entries: Vec<(EntryId, String, Kind, Mtime)>) -> NoteSet`.
- `pub fn NoteContent::parse(text: Arc<str>, modified: Mtime) -> Self`; `pub(crate) modified: Mtime`.
- `Corpus`: slot `Slot { text: Option<Arc<str>>, modified: Mtime }`; `write(at, text: Arc<str>, modified)` and `clear(at)` (the text only) replace `set`; `reorder(ids, mtimes: Arc<[Mtime]>, order)`; `CorpusSnapshot::modified(order) -> Option<Mtime>` and `text(order) -> Option<(EntryId, &str)>`, both `pub(crate)`. `Mtime` has no `Default`: `Corpus::default()` has no slots, so none is needed.
- `pub(crate) struct NameWords` in `words/name_words.rs`: `build<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self`; `of(order) -> impl Iterator<Item = NameWord<'_>>` where `NameWord { folded: &str, bytes: Range<usize>, in_stem: bool }`; `heap_bytes()`.
- `NamesSnapshot` gains `words: Arc<NameWords>`, `#[must_use] pub fn heap_bytes(&self) -> usize` (the name words; `Step::Indexed` gains `names_bytes`), and `pub(crate) fn words(&self) -> &NameWords`, `pub(crate) fn path(&self, order) -> &Arc<str>`, `pub(crate) fn id(&self, order) -> EntryId`.

- [ ] **Step 1: failing tests.**
  - `words/tests.rs`: `a_query_ranks_recent_notes_newest_first_and_keeps_showing` (a repeated id keeps its newest rank; `showing` is returned).
  - `words/name_words/tests.rs`: `a_name_s_words_are_folded_with_their_path_ranges` (`Projets/Été 2026` → `projets` 0..7 folder, `ete` 8..12 stem, `2026` stem); `an_nfd_file_name_folds_like_its_nfc_form` (NoteSet NFC-normalizes first, so the ranges index the NFC path); `a_hangul_name_folds_to_jamo` (`한글` → the NFD jamo, range covers the whole syllables); `a_stem_with_no_word_char_has_no_stem_word` (`notes/---` → `notes` only, in the folder); `a_long_name_word_is_capped` (70 × `a`).
  - `corpus/tests.rs`: `a_slot_keeps_its_text_s_mtime_across_a_reorder`; `a_note_without_text_takes_the_build_s_mtime`; `a_write_sets_the_mtime_with_the_text`; `a_remove_keeps_the_mtime`; the existing snapshot-isolation test extended: a held snapshot's mtimes never change.
  - `index/corpus_tests.rs`: `a_save_moves_the_note_s_mtime_without_a_rescan` (insert with a later mtime; `corpus().modified(order)` is the save's).
  - `content/tests.rs`: `parse_keeps_the_mtime`.
  - `index/model_tests.rs`: the corpus model tracks each note's mtime (a load or save carries a generated one, a build its scan's); asserted with the texts after every step. `external_edits_keep_each_old_mtime_until_the_reload` (several notes edited, reloads landing out of order).
- [ ] **Step 2: implement.** `prepare` keeps the mtime; `NoteSet { …, mtimes: Arc<[Mtime]> }`; `NamesSnapshot::build` also builds `NameWords` from the same paths. `Corpus::reorder` fills `modified` from the previous slot when it keeps a text, else from `mtimes[i]`; `clear` returns early when the slot has no text (the chunk-copy guard). `NoteIndex::insert` calls `write` with `content.modified`; `remove` calls `clear`.
- [ ] **Step 3: call sites.** `test_support::entries` returns the 4-tuple (the entry's `mtime`); benches build the tuple from their scans (`Mtime(SystemTime::UNIX_EPOCH)` where the bench has no scan); `NoteContent::parse(text, mtime)`: tests use a `test_support::mtime(n)` helper (`UNIX_EPOCH + n s`). App: `Built::new` builds the 4-tuple from every entry, folders included (each `Entry` has its `mtime`), and keeps its notes-only `mtimes` list for the reload diff unchanged; `load_more`'s task passes `loaded.stamp.mtime`; `saved` passes `event.stamp.mtime`; app tests follow. `NoteQuery::new(text, None, &[])` at slice 7's call sites.
- [ ] **Step 4:** `./scripts/check.sh`; `./scripts/test-names.sh` diff clean (no file split). `timeline` logs `names_bytes`. Commit: `Index: mtimes beside the corpus texts, name words, NoteQuery ranks recent notes`.

### Task 8.2: `find_notes`, `merge_notes`, the order, the oracle

> **Built** in `80d99d3` and `1a071af`. `inkra-index` gains `unicode-segmentation` from `[workspace.dependencies]` (already used by `text` and `app`), for grapheme-widened highlights. A highlight widens to the **path's** graphemes, not the word's: a word can start inside a grapheme (`🎉\u{301}x`, `\u{600}1`) or end inside one (`a🏻`), and a range may start before the stem when a grapheme straddles the last `/`; the renderer clamps.

**Files:** create `find.rs`, `find/{rank,run,highlight}.rs`, `find/tests.rs`, `find/model_tests.rs`; modify `lib.rs`, `words.rs` (`pub(crate) use fold::{cap, fold, word_spans}`), `words/lookup.rs`.

**Interfaces** (all `pub`, re-exported from `lib.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group { Name, NameAndText, Text }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteMatch {
    pub id: EntryId,
    pub path: Arc<str>,                     // NFC, `.md` stripped, as NameMatch::path
    pub group: Group,
    pub name_highlights: Vec<Range<usize>>, // byte ranges into `path`, whole graphemes, ascending, disjoint
    rank: Rank,
}

/// A note a range kept: its rank only.
pub struct Ranked(Rank);

#[must_use]
pub fn find_notes(snapshot: &IndexSnapshot, lookup: &Lookup, range: Range<usize>,
                  query: &NoteQuery, cancel: &AtomicBool, limit: usize) -> Vec<Ranked>;
#[must_use]
pub fn merge_notes(snapshot: &IndexSnapshot, chunks: Vec<Vec<Ranked>>,
                   query: &NoteQuery, limit: usize) -> Vec<NoteMatch>;
```

`Rank` (private, `Ord`, smaller is better): `(group, Reverse(exact: bool), Reverse(whole), Reverse(key2: u64), showing: bool, recent: usize /* MAX when absent */, Reverse(SystemTime), chars, order)`. `order` is the candidate's global vault order, not its index in `range`; it is unique, so the order is total and `merge_notes` over any split equals one range.

- `lookup.rs` gains `pub(crate) fn ordinal(&self, id) -> Option<Ordinal>` and `pub(crate) fn hit(&self, term, ordinal) -> Option<TermHit>`, so a candidate resolves its ordinal once, not once per term.
- A candidate's terms: for each term, `in_name` = some name word starts with it (and whether one equals it, and whether one in the stem starts with it); `in_text` = `lookup.hit(term, ordinal)`. Listed when every term is in its name or its text. Group: all in name → `Name`; some → `NameAndText`; none → `Text`.
- An empty query lists the MRU without `showing`, group `Name`, no highlights; test `NoteQuery::is_empty`, never `Lookup::term_count() == 0`.
- A bounded top-`limit` heap, as `match_names`; `cancel` checked every 1,024 candidates, and a set `cancel` returns an empty list. `limit` 0 returns none; `usize::MAX` is allowed.
- `highlight.rs`: `pub(crate) fn prefix(word: &str, term: &str) -> Range<usize>`: the shortest run of `word`'s graphemes whose fold starts with `term` (the whole word if none does, which cannot happen for a matched word). `name_highlights` is computed for the returned rows only: every name word a term matches, its prefix shifted to the word's range, sorted, merged when touching.

- [ ] **Step 1: failing tests, `find/tests.rs`** (through the public API, over `test_support` vaults with texts inserted):
  - `every_term_must_match_a_word_start` (`plan` finds `Planning`; `ann` does not, since no word starts with it).
  - `the_three_groups_rank_in_order` (`Name` ⊳ `NameAndText` ⊳ `Text`).
  - One test per order key, each with notes equal on every earlier key: `whole_terms_beat_prefixes`, `stem_terms_beat_folder_terms`, `more_occurrences_rank_first_in_the_text_groups`, `the_note_showing_ranks_last_in_its_group`, `recent_notes_rank_first_newest_first`, `the_newest_mtime_ranks_first`, `a_shorter_path_ranks_first_then_vault_order`.
  - `a_name_row_never_ranks_by_its_text` (two `Name` notes, one with the term 50 times in its text; the stem decides).
  - `accents_fold_both_ways_in_names` (`ete` finds `Été`, `été` finds `ete`).
  - `highlights_cover_the_matched_prefix_in_whole_graphemes`: an NFD name (`e\u{301}te` lit as one grapheme), an emoji ZWJ name, `Planning` lit `0..3` for `pla`, two terms on one word merged.
  - `an_empty_query_lists_the_recent_notes_without_the_one_showing`.
  - `a_cancelled_search_lists_nothing`; `limit_zero_lists_nothing`.
  - `a_note_not_loaded_is_found_by_its_name_only`.
- [ ] **Step 2: the oracle, `find/model_tests.rs`.** A naive model over random vaults (paths from Task 7.5's alphabet plus `/`, texts from it): for each note it splits its path and text with its own rule (`str::split` on every char that `regex_syntax::is_word_character` rejects), folds each word with the shared `fold` (S1: the oracle shares the fold; only its callers are under test), places each term, groups, and sorts by S3's keys, built independently of `Rank`. It must list the same ids in the same order as `find_notes` over `0..len` with `limit = usize::MAX`, and as `merge_notes` over a random split with a random `limit`. `showing`, `recent` and the mtimes are generated (mtimes with ties). Also `the_highlights_are_word_prefixes` over the same cases: each highlight's text folds to a string starting with some term, and no shorter grapheme prefix of it does (minimal). `PROPTEST_CASES=20000` at the gate.
- [ ] **Step 3: implement**, run, `check.sh`. Commit: `Index: find_notes ranks notes by name and text in three groups`.

### Task 8.3: The excerpt

**Files:** create `window.rs`, `window/tests.rs`, `find/excerpt.rs`, `find/excerpt_tests.rs`; modify `search/hit.rs`, `find.rs`, `lib.rs`.

**Interfaces:**
- `pub(crate) fn window::around(text: &str, hit: Range<usize>) -> Range<usize>`: the hit extended within its line by up to 60 bytes before and 100 after, on char boundaries, reading only the window (moved from `Hit::new`, which then calls it; `search`'s tests stay green unchanged).
- `pub struct Excerpt { pub text: String, pub highlights: Vec<Range<usize>> }` (`Debug`, `Clone`, `PartialEq`, `Eq`).
- `#[must_use] pub fn excerpt(snapshot: &IndexSnapshot, found: &NoteMatch, query: &NoteQuery, cancel: &AtomicBool) -> Option<Excerpt>`.

Rules:
- `None` when the note's text is not loaded, or the slot at `found`'s order holds another id.
- `Name` rows, and every row of an empty query: the first non-empty line, cut to its first 160 bytes on a char boundary, leading whitespace trimmed, no highlight.
- `NameAndText`, `Text`: the first word of the text (`word_spans`, `fold`, `cap`) that a term matches, searched in the first `EXCERPT_SCAN_BYTES` (1 MB) only, `cancel` checked every 64 KB (`None` past the limit or on cancel); the window around that word's first `WORD_MAX_CHARS` chars; leading whitespace trimmed when the window starts at its line's start (Zed); every word inside the window that a term matches, highlighted on its matched prefix (`highlight::prefix`), ranges into `Excerpt::text`. `None` if no word matches (a text changed since the lookup: the `IndexSnapshot` is of one build, so this is only defensive).
- The first matching word is found without folding a word whose first char cannot start any term: an ASCII word whose first byte, lowercased, is not the first byte of a term is skipped before `fold` (a pre-check, never a different answer; the model test pins it).

- [ ] **Step 1: failing tests** (`excerpt_tests.rs`): `a_text_row_shows_the_line_around_the_first_match`; `every_matched_word_in_the_window_is_lit_on_its_prefix`; `a_match_at_a_line_start_and_at_a_line_end`; `a_multibyte_text_cuts_on_char_boundaries` (Greek, emoji); `a_ten_megabyte_line_gives_a_window_not_the_line` (one 10 MB line, match in the middle; the excerpt is at most 60 + 100 bytes around the word); `a_match_past_the_scan_limit_has_no_excerpt` (match at the end of a 10 MB line); `a_one_megabyte_word_gives_a_short_excerpt`; `a_set_cancel_gives_none`; `a_name_row_shows_the_first_non_empty_line_unlit`; `an_unloaded_note_has_no_excerpt`; `a_slot_of_another_note_gives_none`; `leading_indentation_is_trimmed_at_a_line_start`; `the_pre_check_never_changes_the_answer` (proptest against the excerpt without it).
- [ ] **Step 2: implement**, `check.sh`. Commit: `Index: the excerpt under a quick-open row`.

### Task 8.4: The quick-open delegate

**Files:** modify `picker/mod.rs`, `picker/render.rs`, `picker/tests.rs`, `quick_open/mod.rs`, `quick_open/render.rs`, `quick_open/render/tests.rs`, `quick_open/tests.rs`; create `quick_open/search.rs` (the background part of a query, to keep `mod.rs` under 400 lines), `app/src/count.rs`; modify `lib.rs`/`main` module list.

**Interfaces:**
- `PickerDelegate` gains `fn render_footer(&self, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> { None }` and `fn row_height(&self) -> Pixels { theme::UI_LINE_HEIGHT }`. The picker draws the footer under the list (or under the empty text), dimmed, and sizes the list `row_height × shown`.
- `QuickOpenDelegate::matches: Vec<Row>` with `struct Row { found: NoteMatch, excerpt: Option<Excerpt> }`.
- `fn count::thousands(n: usize) -> String` (`12340` → `12,340`).

Flow of `update_matches` (the O5 `Scanning` branch and the `Search`/`Refresh` runs unchanged):
1. On the foreground: take `NoteIndex::snapshot()` (names, corpus and words of one build), build `Arc<NoteQuery>` with `showing` and `recent`.
2. In the task, after the wait (none for `Run::Notify`): `lookup` on one background task (`None` → return, cancelled); then `find_notes` on each of `snapshot.chunks(num_cpus)` with the `Arc<Lookup>`; `merge_notes(snapshot, …, query, LIMIT)`; the rows apply at once. Then `rows_shown` spawns one background task per row on screen not asked for yet, running `excerpt`; each lands on its row only while its list's `cancel` is clear.
3. `apply` as today, keyed by `found.id`.

Rows (`render.rs`): line 1 as today, built from `name_highlights` (ranges, already whole graphemes) instead of `positions`; line 2 the excerpt, `placeholder_text` colour, its highlights in `match_text`, one line, ellipsis at the end; an empty second line when there is no excerpt, so every row is `2 × UI_LINE_HEIGHT`. ASCII control chars drawn as spaces (`one_line`), as line 1.

`row_label`: `"{path}: {excerpt}"` when there is an excerpt, else the path, both through `one_line`.

`render_footer`: `Some("Indexing {loaded} of {total} notes")` while `is_building() && !awaits_first_build()`.

- [ ] **Step 1: failing tests.**
  - `quick_open/tests.rs`: `the_selection_survives_the_footer_leaving`.
  - `picker/tests.rs`: `a_delegate_s_footer_is_drawn_under_the_rows` and `the_list_is_as_tall_as_its_rows` (a test delegate with `row_height` 2×; measure the list's bounds via `debug_selector`, per the app-tests rule on pitches).
  - `quick_open/tests.rs`: each existing matching test rewritten to S3 (never deleted): `typing_lists_the_matching_notes_best_first` now `plan` over `plan`, `projects/plan`, `other`, `planning` → `plan`, `projects/plan`, `planning` (whole before prefix, stem before folder), and `p l a n` (four one-letter terms) lists what four word-start terms give. New: `rows_come_in_three_groups`; `a_text_row_shows_its_excerpt`; `a_name_row_shows_its_first_line`; `a_note_not_loaded_has_no_second_line` (a `MemFs` `block_load` gate); `the_footer_counts_the_notes_indexed_then_leaves`; `the_scanning_state_shows_no_footer`; `each_row_label_holds_the_path_and_the_excerpt`; `enter_opens_a_text_only_match` (`simulate_keystrokes`).
  - `quick_open/render/tests.rs`: `a_name_highlight_splits_into_stem_and_folder`; `an_excerpt_highlight_is_kept`.
  - `count.rs` tests: `0`, `999`, `1,000`, `12,340`, `50,000`, `1,234,567`.
- [ ] **Step 2: implement**, `check.sh`, render `quick-open-results` and `quick-open-graphemes` and report the paths. Commit: `Quick open: word-start matching on names and text, two-line rows, the indexing footer`.

### Task 8.5: Refresh during the load, without starving the scan

**Files:** create `app/src/index/watch.rs`, `index/watch/tests.rs`; modify `app/src/index.rs` (module, re-export), `backlinks/mod.rs`, `quick_open/mod.rs`, `quick_open/tests.rs`, `backlinks/tests.rs` (unchanged expectations).

**Interfaces:**

```rust
/// What an observer of `IndexSync` does with a notify.
pub(crate) enum Refresh { Skip, Now, Later }

pub(crate) struct IndexWatch { seen: Option<(u64, bool)>, throttled: Option<Task<()>> }
impl IndexWatch {
    /// `Skip` when this revision and building state were seen; `Later` while building
    /// (at most one pending, `BUILDING_REFRESH` apart); else `Now`.
    pub(crate) fn on_notify(&mut self, sync: &IndexSync) -> Refresh;
    /// Records what a rebuild read; drops the pending throttle.
    pub(crate) fn seen(&mut self, sync: &IndexSync);
    pub(crate) fn throttle(&mut self, task: Task<()>);
}
pub(crate) const BUILDING_REFRESH: Duration = Duration::from_millis(100);
```

The backlinks panel keeps its behaviour exactly (its tests unchanged) with `IndexWatch` in place of `seen` and `throttled`.

Quick open:
- It observes `IndexSync` (loads, saves, merges that notify) through `IndexWatch`, as the panel does, beside its `NotesChanged` subscription (names: `Run::Refresh`, after 200 ms; O5's first-names rule kept). A notify refresh is `Run::Notify`: it keeps the selection like `Refresh` and waits nothing, since `IndexWatch` already throttles it. `update_matches` calls `IndexWatch::seen` when it takes the snapshots.
- **D7.** Any refresh (`NotesChanged` or a notify) while a scan runs (`self.running`) does not call `Picker::refresh`: it sets `dirty`. When the running scan's result applies, a set `dirty` is cleared and one `Run::Refresh` starts. A keystroke still cancels and replaces the scan, and clears `dirty` (its own scan reads the newest index).
- A refresh keeps the selected note (slice 3's rule), whatever its source.

- [ ] **Step 1: failing tests.**
  - `watch/tests.rs`: `a_seen_revision_is_skipped`; `a_notify_while_building_waits_once`; `a_notify_after_building_is_immediate`.
  - `quick_open/tests.rs`: `ten_notifies_during_a_scan_cause_no_cancel_and_one_rerun` (the scan is held by a test-only gate the task awaits before its scan; the cancel flag, read after each notify, stays false; a test-only counter of runs goes up by exactly one after the gate opens); `a_keystroke_during_the_scan_cancels_it`; `the_selected_note_is_kept_across_a_load_refresh`; `a_note_loading_under_the_picker_joins_its_rows` (text arrives, a `Text` row appears after `BUILDING_REFRESH`); `one_build_runs_one_scan` (`NotesChanged` and its notify in one turn); `notes_changed_during_a_scan_reruns_once`.
  - `backlinks/tests.rs`: green, unchanged.
- [ ] **Step 2: implement**, `check.sh`. Commit: `Quick open: refresh as text loads, never cancelling a running scan`; then, if the extraction is its own diff, `Backlinks: the refresh throttle moves to IndexWatch` first.

### Task 8.6: Benches, the end-to-end figure, shots, the spec

- `crates/index/benches/find.rs` (`[[bench]] name = "find", harness = false`): the `words` bench's 50,000-note corpora (40 words, Zipf) with names from the `names` bench's long-stem generator (reuse by `#[path]` module includes, as `words/` does, or a shared bench module; no copy). Cases: `find_notes` with the lookup included for `e`, a common word, a rare word, two words, a name-only query, and an empty query with 20 recent; one core, then split over 10 threads (1×, 2×, 4× ranges); `excerpt` for 100 rows; and `pipeline/<query>`: lookup, `find_notes` over the chosen split, merge and 100 excerpts, as the delegate runs them. If a case misses 16 ms with the lookup, apply S3's one-letter cap or split `lookup`; record the cap and its figure in the spec.
- The release log `quick open: N matches, T from the edit` stays; the orchestrator types the queries in the release app on the generated vault and records p50 and p99.
- Shots (`scenes.rs`; the notes gain texts so rows have second lines): `quick-open-groups` (the three groups), `quick-open-text-only` (a `Text` row with its excerpt lit), `quick-open-indexing` (the footer; a `MemFs` `block_load` gate keeps the load pending), and `quick-open-results`, `quick-open-empty`, `quick-open-no-match`, `quick-open-graphemes`, `quick-open-scanning` re-rendered. Report the PNG paths; the orchestrator looks at them.
- The spec: the Zed paragraph for slice 8, "Built in slice 8" under S3, S4, S5 with the decisions above, the figures, the shots. Search spec D9: rows are two lines in quick open; D1 and D5: "Corrected in slice 8" notes for the 4-tuple and `find_notes`'s corpus argument. The Zed paragraph cites `WORKTREE_UPDATE_REFRESH_DEBOUNCE` (`file_finder.rs:844`). `CLAUDE.md`: drop "(not yet built)" from the smart quick open line.
- Mutants on `crates/index/src/find/**`, `words/name_words.rs`, `window.rs`, `app/src/index/watch.rs`, `app/src/count.rs` and the quick-open files are the orchestrator's gate (Flow step 10).


---

## After slice 8: a warm start and vault search

The user wants the app to open at once, at any vault size. Slice 6 (above) opens the window first. One more slice follows here, with its own spec, written and reviewed before code, then run by the same method. Slice 5's timeline gives its starting figures.

- **Slice 9 — A persisted index (warm start).** On quit, `inkra-index` serializes names, words and the corpus to bytes, and `app` writes them through `Fs` beside the vault's state. On launch, it reads them back and reloads only the notes whose mtime changed since, so search is complete at once instead of after the startup load. It is an Evolution lever of both specs ("Persisted index", "Persisted word index"), and it changes the D1 API. The serialization format is a dependency choice: `researcher` and the `research-before-recommending` protocol run before its spec is frozen.

Vault search, the search plan's slice 6, becomes **slice 10**, unchanged except that its Task 6.0 no longer adds `render_footer` or the refresh helper (Tasks 8.4 and 8.5 build them). It follows the same method.
