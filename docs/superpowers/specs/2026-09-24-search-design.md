# Search — design

Date: 2026-09-24. Status: **reviewed**, before code.

The application is "a vault of markdown files, links and backlinks, search,
panes". Links shipped (`2026-09-24-links-and-backlinks-design.md`), and that
spec reserved full-text search for `inkra-index`, fed by the same loads as
links (its D6). This spec adds two features:

- **Quick open** (`⌘P`): a fuzzy match on note names and paths.
- **Vault search** (`⌘⇧F`): a literal full-text search across every note.

Both open the same modal picker. Both rest on `inkra-index`.

A first draft was reviewed by three adversarial passes (correctness against
the code and gpui, performance at 50,000 notes, external facts) and two
constructive passes (evolvability, simplicity and UX). Each adversarial
finding was checked by an independent verifier; none was refuted. Where the
draft was wrong, a **Corrected** note records it.

## Requirements

- **Scale.** A vault of 50,000 notes stays usable.
  - Quick open answers within one frame, 16 ms, from the keystroke.
  - Vault search shows results within 100 ms of the end of the debounce.
  - No step runs over 8 ms on the foreground at that scale.
  - The benches below measure each figure in release.
- **Evolvable.** Performance, memory and speed will be improved after v1. The
  engine sits behind a narrow API (D1), so its storage and matching can change
  without touching `app`. Every later change passes the same oracles and is
  measured by the same benches against a saved baseline. The levers and their
  triggers are listed under "Evolution".
- **What was saved.** Vault search reads the last saved text, as the link
  index does (links spec, D5). Autosave runs about two seconds after typing.

## Scope

| In | Out, deferred below |
|---|---|
| Quick open: fuzzy match on name and path, accent-insensitive, in-memory MRU | Persisted frecency, typo tolerance |
| Vault search: literal text, case and whole-word toggles | Regex, `path:`/`tag:` operators, accent folding in note text |
| One modal picker for both, hits grouped under a header per note | Find in the current note (`⌘F`), replace |
| Opening a search hit at the match | Search in unsaved buffers, streaming results |
| Benches at 50,000 notes and an oracle for each matcher | A persisted index |

## What Zed does, and what we take from it

Paths are under `vendor/zed-reference/crates/`. Two Zed features are the
reference: the file finder for quick open, and the Text Finder
(`search/src/text_finder/`), a modal picker over project search, for vault
search.

Taken:

- **A picker split from its data.** `Picker<D>` owns the query field and the
  list. `PickerDelegate` owns the matches and renders one row
  (`picker/src/picker.rs:127-153`, `:164-383`). We have two delegates, quick
  open and vault search, so the trait has two real implementers.
- **A modal layer that occludes the views below.** A full-window backdrop
  with `.occlude()` dismisses on click. The inner container, also occluded,
  stops propagation (`workspace/src/modal_layer.rs:299-327`). The layer stores
  the focused handle when a modal opens and restores it on dismiss
  (`:160`, `:254-257`). It dismisses on focus-out (`:189-193`).
- **Cancellation in three layers** (`file_finder/src/file_finder.rs:1069-1105`,
  `picker/src/picker.rs:1244-1250`). Replacing the pending `Task` drops it. A
  shared `AtomicBool` stops the matcher mid-run. A monotonic search id drops a
  result that arrives after a newer query.
- **A name bonus.** A match in the file name outranks a match in the folder
  (`fuzzy_nucleo/src/paths.rs:113-135`, `:194-197`). Terms separated by spaces
  match in any order (`fuzzy_nucleo/src/fuzzy_nucleo.rs:51-87`).
- **Accent-insensitive names, one way.** Zed builds its pattern with nucleo's
  `Normalization::Smart` (`fuzzy_nucleo/src/fuzzy_nucleo.rs:73-78`): `ete`
  finds `été`, `été` does not find `ete`.
- **A character-set prefilter.** Each path's `CharBag` is computed once, at
  scan time. A candidate that lacks a query character is rejected before any
  scoring (`fuzzy/src/char_bag.rs:8-31`, `worktree/src/worktree.rs:6521-6525`).
- **Parallel segments on the gpui executor**, counted by
  `BackgroundExecutor::num_cpus()`, which returns 4 under test
  (`gpui/src/executor.rs:261-267`, `fuzzy_nucleo/src/paths.rs:290-354`).
- **Hits grouped under a non-selectable header per file** (Text Finder,
  `text_finder/delegate.rs:113-117`, `:822`).
- **Case and whole-word toggles** in the search field's trailer
  (`text_finder/delegate.rs:733-775`).
- **Caps with a "limit reached" signal** (`project/src/project_search.rs:165-166`,
  `:635-662`).
- **A stale finder re-runs its query** when the worktree changes, after 200 ms
  (`file_finder.rs:843-844`, `:1003-1016`).
- **An MRU of about 20 notes** for an empty query, deduplicated from the
  results (`file_finder.rs:557-613`, `workspace/src/workspace.rs:178`).
- **A placeholder and a "no matches" text** per picker
  (`picker/src/picker.rs:222-225`).

Deviations:

- **No rereading of every file on every query.** Zed rereads the worktree on
  each project search (`project_search.rs:444-529`). At 50,000 notes of 3 KB,
  open and read took 1.1 to 1.25 s per query, on 10 threads with a warm cache
  (measured in review). We keep the saved text in memory instead (D3).
- **No open-buffer precedence** (`project_search.rs:914-925`), for the reason
  the links spec gives in D5.
- **Whole word decided per char, not per byte.** Zed tests the first and last
  byte of the escaped pattern (`project/src/search.rs:243-254`), so an edge
  like `é` gets no `\b`, and whole word does nothing. We test chars (D4).
- **Debounce only while a search runs**, for vault search too. The Text Finder
  always waits 100 ms (`text_finder/delegate.rs:716-719`, `:891`); the file
  finder waits only when a search is in flight (`file_finder.rs:1944-1954`).
  We use the file finder's rule for both, since an idle picker then answers
  the first keystroke at once.
- **Results arrive once, not streamed.** The Text Finder appends batches of
  256 (`text_finder/delegate.rs:1227-1315`). Our per-range scans fit the
  budget (Performance); streaming is a lever.
- **Accessibility roles on the picker.** Zed's picker sets none. We set them
  (D8).
- **Our own matcher, not a port.** `fuzzy`, `fuzzy_nucleo` and `picker` are
  GPL-3.0-or-later (`fuzzy/Cargo.toml:6`). We take the ideas above and write
  our own code. `nucleo-matcher`, which Zed uses, is MPL-2.0 and allowed by
  `deny.toml`, but its last release is from 2024-02-20, which fails the
  twelve-month rule in `rules/dependencies.md`.

## What fff does, and what we take from it

fff (`github.com/dmtrKovalenko/fff`, read at `e3f694a`) is a Rust file finder
with content search. It targets code repositories up to Chromium size.

Taken:

- **A current-note penalty.** The note already showing is not the best answer
  to "open a note". fff subtracts a quarter of its match score
  (`crates/fff-core/src/score.rs:790`, `:983-986`).
- **Results in file order, capped**, not ranked by relevance
  (`grep/grep.rs:664-707`). Ranking content hits is deferred here too.
- **A cancel check every few files** during a content search
  (`grep/grep.rs:571-660`).
- **A bigram prefilter**, as the first lever and not v1
  (`index/bigram_filter.rs`). fff reports about 30 MB for 50,000 files.

> **Corrected.** The draft cited fff for "content held in memory". fff does
> the opposite for note-sized files. It keeps only its bigram index resident,
> mmaps files of 16 KB or more within a 512 MB budget, and rereads smaller
> files on each grep (`types.rs:258-262`, `:725-727`, `:811-816`;
> `constants.rs:12-17`). fff is evidence for the "no resident text" lever, and
> that lever needs the prefilter first.

Rejected:

- **fff as a dependency.** `fff-search` 0.11 pulls 203 crates, vendored
  libgit2 and LMDB, and 123 `unsafe` sites.
- **Git boosts, LMDB, the SIMD path arena, mmap caching.** They are code
  repository concerns, or they solve a scale that a note vault does not have.
- **Persisted frecency in v1.** The decay formula (`dbs/frecency.rs:373-418`)
  is recorded in Deferred, with its trigger.

## Decisions

### D1. One owner in `inkra-index`, pure functions, no executor

`LinkIndex` is renamed `NoteIndex`, in its own commit, since it no longer
indexes only links. It owns everything keyed by note membership:

| Field | Role |
|---|---|
| `notes: NoteTable` | membership, vault order, link resolution (unchanged) |
| link sources and `by_stem` | backlinks (unchanged) |
| `names: Names` | quick-open candidates (D2) |
| `corpus: Corpus` | saved text (D3) |

Three calls change all four together, with one membership check and one
pruning rule:

```rust
impl NoteIndex {
    pub fn set_notes(&mut self, notes: NoteSet);             // swap membership, prune, reorder corpus
    pub fn insert(&mut self, id: EntryId, source: Source, text: Arc<str>); // ignored for a non-note
    pub fn remove(&mut self, id: EntryId);
    pub fn names(&self) -> NamesSnapshot;                    // Arc clone
    pub fn corpus(&self) -> CorpusSnapshot;                  // Arc clone
}
impl NoteSet {
    pub fn build(entries: Vec<(EntryId, String, Kind)>) -> NoteSet; // NoteTable + Names, off the foreground
}
```

A load that finishes after its note was trashed is ignored by `insert`, for
links and text alike (`crates/index/src/index.rs:44-49`).

> **Corrected.** The draft had `Corpus` and `Names` fed beside `LinkIndex`.
> A late load after a trash would then put the text back, since only
> `NoteTable` knows membership, and it is `pub(crate)`
> (`crates/index/src/notes.rs:21`).

The matchers are pure functions over immutable snapshots:

```rust
// names
pub struct NameQuery;           // NameQuery::new(text, showing: Option<EntryId>, recent: &[EntryId])
pub struct NameMatch { pub id: EntryId, pub score: i32, pub positions: Vec<usize> }
impl NamesSnapshot { pub fn len(&self) -> usize; pub fn chunks(&self, n: usize) -> Vec<Range<usize>>; }
pub fn match_names(names: &NamesSnapshot, range: Range<usize>, query: &NameQuery,
                   cancel: &AtomicBool, limit: usize) -> Vec<NameMatch>;
pub fn merge_names(chunks: Vec<Vec<NameMatch>>, limit: usize) -> Vec<NameMatch>;

// search
pub struct TextQuery;           // TextQuery::new(text, case_sensitive, whole_word) -> Option<TextQuery>
impl TextQuery { pub fn find_in(&self, text: &str, limit: usize) -> Vec<Range<ByteOffset>>; }
pub struct Hit { pub note: EntryId, pub range: Range<ByteOffset>,
                 pub context: String, pub context_match: Range<usize> }
pub struct Searched { pub hits: Vec<Hit>, pub limit_reached: bool }
impl CorpusSnapshot { pub fn chunks(&self, n: usize) -> Vec<Range<usize>>; }
pub fn search(corpus: &CorpusSnapshot, range: Range<usize>, query: &TextQuery,
              cancel: &AtomicBool, limit: usize) -> Searched;
impl Searched { pub fn merge(chunks: Vec<Searched>, limit: usize) -> Searched; }
```

`inkra-index` has no executor and reads no clock. `app` asks a snapshot for
`chunks(n)`, runs each range on gpui's background executor, and calls the
merge. The executor gives cancellation and deterministic tests.

> **Corrected.** The draft cited a "no rayon" rule in `docs/tooling.md`. There
> is none; that file only drops criterion's default `rayon` feature. The
> choice above stands on its own reason.

**This is the seam for evolution.** `app` sees queries, snapshots, opaque
ranges, merges and owned results. It never sees how text is stored, how
chunks are balanced, or how a candidate is rejected. `Hit` carries its own
excerpt, as `SourceLink` does (`crates/index/src/source.rs:19-25`), so no row
reads the corpus. `find_in` is public because two deferred features need it
on a single text (Find in note, search in unsaved buffers), and because the
"no resident text" lever runs it in `app` over loaded candidates.

### D2. The name matcher

Candidates are notes, never folders. A candidate is the note's path as the
vault stores it, NFC, `/`-separated, `.md` stripped. Its stem is the last
component. `NoteSet::build` computes each candidate's folded chars and
`CharBag` once.

The query is NFC-normalized, then split on whitespace into terms. A note
matches when every term is a subsequence of its path, in any term order.
Chars compare by this rule, which the proptest oracle shares:

- Case folds per `char` with `char::to_lowercase`; a char whose lowercase is
  several chars compares by the first.
- Accents fold one way, as nucleo's `Smart`: a query char with no diacritic
  matches a path char whose base is that char. The base is the first char of
  the path char's NFD decomposition, from `unicode-normalization`, already a
  dependency. `ete` finds `été`; `été` does not find `ete`.

Positions index the path's chars as stored, so folding needs no offset map.

For each term, a dynamic program finds the best alignment. Scores are
integers, so ties are exact and the order is deterministic:

| Signal | Effect |
|---|---|
| A match at the start of the stem | bonus |
| A match after `/`, `-`, `_`, space, `.`, or at a lower→upper case change | bonus |
| A match right after the previous match | bonus |
| A gap between matches | penalty that grows with the gap, with a floor |
| Every matched char of a term inside the stem | bonus |
| The stem equals the query, folded | large bonus |
| The note currently showing (`NameQuery::showing`) | minus a quarter of its score, as fff does |

Ties break by MRU rank (`NameQuery::recent`), then shorter path, then vault
order. `match_names` applies the whole order and keeps a bounded top-`limit`
heap. It computes `positions` only for the matches it keeps. `merge_names`
sorts the chunks' matches by the same order and truncates. So splitting never
changes the result.

> **Corrected.** The draft applied the MRU tie-break and the current-note
> penalty in `app`, after each range had already cut to 100. The right note
> could be lost at the cut.

An empty query lists the MRU (D6) without the note showing. A query with
results lists every match, best first, capped at 100 like Zed
(`file_finder.rs:609`). In v1 `app` calls `match_names` once, on
`0..len`; the split is a lever.

### D3. The corpus holds the saved text, in vault order

`Corpus` is an `Arc<Vec<Option<Arc<str>>>>` indexed by the vault order that
`NoteTable` already assigns (`crates/index/src/notes.rs:64-71`). `None` is a
note not yet loaded.

- A save stores the `Arc<str>` that `NoteSaved` carries
  (`crates/app/src/vault/events.rs:11-15`), shared, not copied. A load
  converts its `String` once.
- A load or a save writes one slot through `Arc::make_mut`. The write copies
  the `Vec` of pointers only if a search still holds the previous snapshot.
- Only `set_notes` reorders the `Vec`, in one O(notes) pass.
- A snapshot is an `Arc` clone. No sort, no rebuild.

> **Corrected.** The draft rebuilt a sorted snapshot on the first query after
> a change. During the startup load every load is a change, and the rebuild
> measured 15 to 24 ms at 50,000 notes: a dropped frame per keystroke.

`Corpus::heap_bytes()` returns the sum of text lengths plus a fixed cost per
slot. It is maintained on each write, checked by a unit test, printed by the
bench, and logged when indexing ends. At 50,000 notes of 3 KB on average it is
about 150 MB. That is the price of D3, and the first thing the Evolution
levers reduce.

An inverted index (tantivy, SQLite FTS5) matches words, not substrings, and
adds a large dependency with its own invalidation. It stays a lever.

### D4. The text matcher

`TextQuery::new` NFC-normalizes the query and compiles the escaped literal
once per keystroke to a `regex::Regex`. It returns `None` for an empty query
or one made only of whitespace.

- **Case.** Insensitive by default, with Unicode simple case folding: `été`
  finds `Été`. The toggle makes it sensitive.
- **Whole word.** `\b` is added at each edge whose char is a word char:
  alphanumeric or `_`, which is regex's Unicode `\b`. It is decided on the
  first and last char of the unescaped query. Whole-word `été` matches in
  `l'été,` and not in `étés`. Never `(?-u:\b)`: it treats `é` as a non-word
  char.
- **NFD text.** The corpus stores text as saved. An NFC query does not match
  NFD text in v1. A test asserts it; the fix belongs with accent folding
  (Deferred).

`find_in` runs `find_iter` on one text and returns at most `limit` ranges.
`search` walks its range in vault order and calls `find_in` on each loaded
note, with a limit of 100 per note. It checks `cancel` between notes, and
stops once it holds `limit` hits, reporting `limit_reached`.

A hit's `context` is the hit extended within its line by up to 60 bytes
before and 100 after, cut on char boundaries. It is never shorter than the
hit. `context_match` is the hit's range inside `context`. Offsets are
`ByteOffset`, and never become a char offset.

The delegate clones the `TextQuery` once per range, since a `Regex` shared
across threads contends on its cache for many short haystacks (regex 1.13,
`src/lib.rs:531-547`).

`regex` becomes a direct dependency of `inkra-index`. It is already in
`Cargo.lock` at 1.13.1 through gpui, under MIT or Apache-2.0, which
`deny.toml` allows.

Limits: 1,000 hits in total, 100 per note. `Searched::merge` concatenates in
range order, which is vault order, and truncates. The order does not depend on
thread timing.

A whole-word query over French prose leaves regex's lazy DFA, which cannot
handle a Unicode word boundary on a non-ASCII haystack (regex-automata 0.4.18,
`hybrid/dfa.rs:243-254`). The bench below covers it. If it misses the budget,
`find_in` searches the plain literal and checks the edges in Rust, behind the
same API.

### D5. Keeping the index current: `LinkSync` becomes `IndexSync`

The links spec's reconcile loop stays the one feed (links spec, D6). It is
renamed `IndexSync`, in the same commit as `NoteIndex`.

- **Membership.** On each vault notify, `reconcile` clones the notes as
  `Vec<(EntryId, String, Kind)>` and runs `NoteSet::build` on the background
  executor. The result is applied on the foreground by `set_notes`, unless a
  newer build has started: a generation counter drops the stale one. Then the
  mtimes are diffed and the loads queued, as today.
- **Loads and saves.** Each load yields the `Source` and the text. Each
  `NoteSaved` yields both too. Both go through `NoteIndex::insert`.
- **Progress.** `IndexSync::progress() -> (loaded, total)`.

> **Corrected.** The draft kept `set_notes` on the foreground and left moving
> it to a trigger. That trigger is already met at 50,000 notes: the links spec
> measured 25 ms at 10,000 notes (Performance), about 125 ms at 50,000 if
> linear, on every rescan.

During the startup load, vault search runs over the notes already loaded, and
says so (D9).

### D6. The MRU

`AppShell` keeps the last 20 notes shown, newest first, in memory, updated in
`set_showing`. It passes them to `NameQuery::new`. A note no longer in the
vault is dropped when the list is read. Persistence is deferred.

### D7. The picker

A new `app/src/picker/` module holds `Picker<D: PickerDelegate>`. The
delegate owns the query state and the matches, and renders a row. The picker
owns:

- a `NameField` for the query. `NameField` gains an `Edited` event, emitted by
  `edit_buffer` (`crates/app/src/field.rs:59-72`).
- a `uniform_list` of one-line rows with a scroll handle, as in the sidebar
  (`crates/app/src/sidebar/mod.rs:97-105`).
- the selected row. A delegate can mark rows as not selectable (vault-search
  headers); the selection skips them.

**Keys.** The deepest context wins in gpui (`gpui/src/keymap.rs:150-190`), and
the focused `NameField` already binds `enter` and `escape`
(`crates/app/src/keymap.rs:52-53`). So:

| Key | Context | Effect |
|---|---|---|
| `up`, `down` | `Picker` | move the selection |
| `enter` | `NameField` → `Confirmed` | the picker confirms the selected row |
| `escape` | `NameField` → `Cancelled` | the picker dismisses |
| `alt-cmd-c` | `VaultSearch` | toggle case sensitivity |
| `alt-cmd-w` | `VaultSearch` | toggle whole word |

The picker subscribes to `Confirmed` and `Cancelled`, as the sidebar does
(`crates/app/src/sidebar/edit.rs:152-155`). The picker's root adds the
delegate's key context, `QuickOpen` or `VaultSearch`, so the toggles exist only
in vault search. A later picker key must not collide with a `NameField`
binding. A click confirms a row.

> **Corrected.** The draft bound `enter` and `escape` in `Picker`. Those
> bindings could never fire under the field's.

**Query flow**, per keystroke:

1. `Edited` → `delegate.update_matches(query)`, which returns a `Task`.
2. The delegate sets the previous cancel flag, makes a new one, and bumps its
   search id.
3. If a search is still running, it waits 100 ms first.
4. It takes the snapshot from `NoteIndex` (an `Arc` clone), asks it for
   `chunks(cx.background_executor().num_cpus())`, and runs each range with
   `background_spawn`. Quick open uses one range in v1.
5. On the foreground, it drops the result if its id is stale. Else it merges,
   notifies, and selects the first selectable row.

The picker stores the `Task`, so replacing it cancels the one before. The
delegate sets its cancel flag on `Drop`, so a dismissed picker stops scanning
at once: dropping a `Task` does not interrupt a synchronous scan already
running.

**A changing vault.** The delegate observes `IndexSync`. On a change it re-runs
the query after 200 ms, as Zed's file finder does, and keeps the selected
note if it is still listed. During the startup load, the refresh is throttled
by `BUILDING_REFRESH`, as the backlinks panel does
(`crates/app/src/backlinks/mod.rs:81-92`). Confirming a row whose note is gone
does nothing.

### D8. The modal layer

`AppShell` gains `modal: Option<Modal>`, holding the picker view and the focus
handle that was focused when the first picker opened. It follows Zed's
`ModalLayer` (`workspace/src/modal_layer.rs`):

- Render adds a full-window child, `.absolute().inset_0().occlude()`, over the
  flex row. A left click on it dismisses. Inside, a 600 px container 15 % from
  the top, also `.occlude()`, stops propagation on mouse down. gpui's hit test
  stops only at an occluding hitbox (`gpui/src/window.rs:1106-1123`), so
  without both, a click on a row also reaches the editor below.
- Opening a picker replaces the other one and keeps the stored handle.
  Pressing the key of the picker showing dismisses it.
- On dismiss, the stored handle gets focus back if the modal still holds focus,
  else the shell's handle. The picker also dismisses when focus leaves it.
- `cmd-p` during a sidebar rename moves focus away from the rename field. The
  sidebar keeps the field open, as it does today for any focus change; the
  rename is not cancelled.
- The pickers do not open when no vault is open.
- Accessibility: the modal has a dialog role and an `aria_label` ("Quick
  open" or "Search notes"), the list a list role, and each row an
  `aria_label` with its plain text (`gpui/src/elements/div.rs:1304`, `:1326`).
  The selected row is not announced until `set_active_descendant` is wired;
  `docs/gpui-accessibility.md` records it.

> **Corrected.** The draft had no occlusion and returned focus to the shell,
> which is the editor whenever one exists. From the tree, `cmd-p` then
> `escape` would have moved focus to the editor.

Bindings, in the `AppShell` context so they fire from any focus, as `cmd-n`
does:

| Key | Action |
|---|---|
| `cmd-p` | `QuickOpen` |
| `cmd-shift-f` | `SearchVault` |

None of `cmd-p`, `cmd-shift-f`, `alt-cmd-c` or `alt-cmd-w` is bound today
(`crates/app/src/keymap.rs`).

### D9. Rows and states

**Quick open** rows show the stem with its matched chars highlighted, then the
folder path dimmed, with its own highlights. A long folder path is cut from
its start (`…/projects/2026`), so the nearest folder stays visible.

**Vault search** rows follow the Text Finder. One non-selectable header row
per note shows its name and hit count. One row per hit follows it, showing the
hit's `context` with `context_match` highlighted.

Highlights use `StyledText::with_highlights`, as the backlinks panel does
(`crates/app/src/backlinks/render.rs:29-38`). Colours come from `theme.rs`.
Every row is one line, so `uniform_list` can measure from row 0.

The field's trailer shows the two toggles, `Aa` and `W`, each lit when on.

| State | Quick open | Vault search |
|---|---|---|
| Empty query | the MRU; placeholder "Open a note…" | nothing; placeholder "Search notes…" |
| Empty MRU | "No recent notes" | — |
| No match | "No matching notes" | "No matches" |
| No match while indexing | — | "No matches in 12,340 of 50,000 notes indexed so far" |
| Indexing, with matches | — | footer "Indexing 12,340 of 50,000 notes" |
| `limit_reached` | — | footer "1,000+ results; refine the query" |

Each state is rendered in a shot scene and looked at.

### D10. Opening a hit at the match

Quick open emits `NoteActivated(id)`, the event the sidebar and backlinks
already emit. The shell opens the note as it does today.

Vault search emits `HitActivated { note, range }`. The shell passes the range
through `open_note(id, Option<Range<ByteOffset>>)`:

- Inside the activation update, right after `open_buffer`, the editor sets the
  selection to the range, clipped by `Buffer::clip`, through its `move_cursor`
  gate (`crates/app/src/editor/mod.rs:188-205`). The note may be open with
  unsaved edits, so the saved offsets may not fit the live text. `clip` snaps
  to a char boundary, not a grapheme boundary; the selection may split a
  grapheme, and the next caret move corrects it.
- A superseded activation is a dropped `Task`, so it cannot apply a stale
  range.
- Only the reveal is deferred. `reveal_cursor` needs `last_text_top`, which is
  written in paint (`crates/app/src/editor/element.rs:210-216`), and
  `open_buffer` clears it. `EditorView` gains a `pending_reveal` flag,
  consumed after the first paint with a non-zero viewport. It calls
  `reveal_cursor` and notifies. The note shows at its top for one frame, then
  scrolls. Zed reveals in prepaint instead; recorded here as the alternative
  if the jump is noticed.
- If the hit is in the note showing, the shell selects and reveals without
  rebinding, which would reset the caret and scroll, as `follow` already
  avoids (`crates/app/src/shell.rs`).

> **Corrected.** The draft deferred the selection to the next layout. The
> selection is an input to shaping, since conceals reveal around it, so the
> first layout would have been shaped with the caret at 0.

This also gives the links spec its deferred "open at offset" (links spec,
Deferred, "Scroll to `#heading`…"). Backlinks do not adopt it in this spec.

## Performance and the benches

Two criterion benches join `crates/index/benches/`. Each figure is recorded in
this spec with the machine and its core count. A lever is measured with
`cargo bench -p inkra-index -- --save-baseline before`, then
`--baseline before`, on one machine. Single-thread throughput in bytes per
second (`Throughput::Bytes`) is the figure that travels between machines.

- **`names`**: 50,000 paths, 3 to 5 components, words from a fixed
  vocabulary with accented ones. Cases:
  - `NoteSet::build` at 50,000 notes (background, no budget; recorded).
  - `set_notes` with a built `NoteSet`, including the corpus reorder. Budget
    8 ms, the foreground limit.
  - `match_names` on one core, queries of 1, 3 and 8 chars, two terms, and an
    unaccented query over accented names. Budget 16 ms each.
- **`search`**: 50,000 notes of 1 to 5 KB of generated French and English
  prose, about 150 MB. Cases, each single-thread over the full corpus, in
  bytes per second:
  - a rare word, a common word, a 1-char query that hits the cap;
  - a frequent prefix with no match (`projetzz`), the full scan with no early
    stop;
  - whole-word, a frequent 1-char substring with no whole-word hit (`é`);
  - whole-word, a common word.
  - `Corpus::heap_bytes()` is printed.

  The budget per core follows from the requirement. 150 MB in 100 ms on 4
  cores needs about 375 MB/s per core. Measured in review with regex 1.13.1,
  single thread: a rare word in 17 ms, `(?i)projetzz` in 95 to 130 ms, and
  `(?i)\be\b` in 310 to 339 ms with no hit. The last one misses the budget on
  4 cores, so D4's fallback is likely needed. The bench decides.
- **Snapshot**: `corpus()` and a slot write while a snapshot is held, at
  50,000 notes. Budget 2 ms.

**Startup** cannot be measured headless: gpui's test scheduler is simulated
and `MemFs` has no I/O cost. An `#[ignore]` test generates 50,000 notes in a
temp dir and runs `IndexSync` over `RealFs` in release. The time until the
queue is empty is recorded here. The app also logs that time and
`heap_bytes()` when indexing ends.

**End to end**, a release-only log in each delegate records the time from
`Edited` to `notify`. It is checked once on the generated vault.

## Testing

- **Oracles in `inkra-index`.** Proptest compares each matcher with a naive
  model across random vaults and queries: NFD, mixed case, accents, and the
  fold-special chars `K` (Kelvin sign), `ſ` and `É`.
  - `match_names` returns the same set of notes as "every term is a
    subsequence of the folded path, by D2's char rule". Every position is in
    bounds and on a char boundary. With `showing` and `recent` generated,
    `merge_names` over any split equals one range.
  - `search`, ASCII queries over ASCII text: the same hits in the same order
    as `str::match_indices` over ASCII-lowercased text. Unicode folding makes
    `k` match the Kelvin sign, so this model stops at ASCII.
  - `search`, all inputs: each hit is on char boundaries, and `context`
    contains the match at `context_match`. A query longer than 160 bytes is
    one case.
  - `Searched::merge` over any split equals one range.
  - Whole word: `été` matches in `l'été,`, not in `étés`; a query starting
    with punctuation.
  - An NFC query over NFD text finds nothing (D4).
- **The differential oracle.** The v1 `find_in` (escaped regex, no prefilter)
  is kept as a test-only reference once a lever replaces it. Every later
  `search` must return identical hits in identical order, for all four flag
  combinations, over Unicode text. This, with the benches, is the contract
  for every lever.
- **Cancellation.** A set flag makes each function return early.
- **`NoteIndex`**: a `heap_bytes` test; an `insert` after a `set_notes` that
  removed the note leaves no links and no text.
- **`IndexSync`**, against `MemFs`, extending the `LinkSync` tests: a load, a
  save, a rename, a trash and a rescan each update the corpus as they update
  links; a load finishing after a trash leaves nothing; a stale `NoteSet`
  build is dropped.
- **The picker**, headless:
  - `cmd-p` and `cmd-shift-f` open it; each picker's own key dismisses it.
  - `up`, `down`, `enter`, `escape`, `alt-cmd-c` and `alt-cmd-w` each do what
    D7 says, with a `simulate_keystrokes` test each.
  - `up` and `down` skip headers.
  - A stale result is dropped.
  - With `num_cpus` overridden, the merged result equals one range.
  - A click on a row leaves the editor's selection unchanged. A click on the
    backdrop over a sidebar row dismisses, and does not activate the row.
  - From the tree, `cmd-p` then `escape` leaves the tree focused.
  - Trashing a listed note removes its row.
  - A hit far down a long note opens with the range selected and visible.
- **Shots**: quick open with results, vault search with headers and toggles,
  and each state of D9's table.
- **Mutants**: `cargo mutants` on `inkra-index` after slices 1, 2, 4 and 5.

## Slices

Each slice merges alone with `check.sh` green, and records its bench figures.

1. **`names`**: `NoteSet::build` (names only), `NameQuery`, `match_names`,
   `merge_names`, `chunks`, the char rule, the oracles, the `names` bench.
2. **`NoteIndex` and `IndexSync`**: the two renames in one commit, then
   `NoteSet` holding `NoteTable`, the background build with its generation
   counter, `names()`, `progress()`. The `set_notes` budget is checked.
3. **Quick open**: `NameField::Edited`, `Picker`, the modal layer, the
   quick-open delegate, the MRU, `cmd-p`, the key and focus tests, the
   accessibility roles, the shots.
4. **`search`**: `Corpus`, `TextQuery`, `find_in`, `search`,
   `Searched::merge`, `heap_bytes`, the oracles, the `search` and snapshot
   benches. `regex` checked against `deny.toml`. The whole-word fallback lands
   here if the bench needs it.
5. **The corpus feed**: `NoteIndex::insert` takes the text, `IndexSync` passes
   it on load and save. The startup figure is recorded.
6. **Vault search**: the delegate, headers, toggles, `cmd-shift-f`,
   `HitActivated`, `open_note` with a range, `pending_reveal`, the key tests,
   the shots.

## Evolution

Each lever keeps the D1 API unless stated, passes the oracles and the
differential oracle, and is justified by a bench figure before and after.

| Lever | Trigger | What changes |
|---|---|---|
| Bigram prefilter (fff) | A `search` case over its per-core budget, or a slow real vault | `Corpus` keeps a bitset per bigram of case-folded text; `search` skips notes that lack one. About 30 MB at 50,000 notes, per fff |
| Whole-word check in Rust | The whole-word bench over budget | Inside `find_in` (D4) |
| `memchr::memmem` fast path | A profile shows `regex` in the case-sensitive path | Inside `find_in` |
| Balanced chunks | Ranges finish far apart in the end-to-end log | `chunks` splits by bytes, not by count |
| Parallel quick open | The `names` bench over 16 ms | `app` passes `num_cpus` to `NamesSnapshot::chunks` |
| Streaming results | First results over 100 ms on a real vault | The delegate appends each range's hits in vault order once every earlier range is done, as the Text Finder does; keeps the selection across batches |
| Compressed text (e.g. lz4) | `heap_bytes` over 300 MB on a real vault | `Corpus` stores compressed blocks; `search` decompresses candidates. Best after the prefilter |
| No resident text | Memory still too high after compression | `Corpus` keeps the prefilter only. **Changes the API**: `CorpusSnapshot::candidates(range, &TextQuery)` lists notes, `app` loads them through `Fs` and calls `find_in`. Needs the prefilter first, as fff shows |
| Parallel startup loads | The startup figure over 10 s | `IndexSync` keeps N loads in flight and applies them in batches, one foreground update per K notes |
| Persisted index | Cold start too slow even with parallel loads | **Changes the API**: `index` serializes the prefilter to bytes, `app` writes them through `Fs` and invalidates by mtime |

## Deferred

| Item | Trigger | Notes |
|---|---|---|
| Persisted frecency | The MRU forgets across restarts and a user misses it | fff's decay (`dbs/frecency.rs:373-418`), stored with the vault state |
| Typo tolerance | A user misses a note by a typo | `frizbee` 0.13 (MIT) or a bounded edit distance per term |
| Regex queries | The user asks | `TextQuery` already compiles to a regex; the question is UI and errors |
| `path:`, `tag:`, `-exclude` | The user asks | A small query parser in front of `TextQuery` |
| Accent folding and NFD in note text (`ete` finds `été`) | The user asks | Folds text and query; hit offsets need a map back to the saved text |
| Relevance ranking of content hits | Vault order is not useful | Notes whose name matches first is the cheap step |
| Find in note (`⌘F`) | The user asks | `find_in` over the buffer; Zed splits the buffer into chunks (`editor/src/items.rs:2010-2087`) |
| Search in unsaved buffers | Two seconds of lag is observed | Links spec, D5; `find_in` over open buffers |
| Unlinked mentions | The user asks | A `search` for each note's name |
| `[[` completion | Links spec, Deferred | Reuses `match_names` |
| Backlinks open at their line | Links spec, Deferred | Reuses `open_note` with a range |
| Selected-row announcement | A screen-reader user asks | `set_active_descendant`, see `docs/gpui-accessibility.md` |
