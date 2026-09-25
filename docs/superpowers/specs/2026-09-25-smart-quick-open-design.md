# Smart quick open — design

Date: 2026-09-25. Status: **draft, for review**. Amends
[`2026-09-24-search-design.md`](2026-09-24-search-design.md) (the search spec):
its D2 matcher stops serving quick open, and its slice list grows by two. Every
other decision of the search spec stands.

## Why

Quick open shipped in slice 3 with the D2 matcher: a note matches when every
typed term is a subsequence of its path. On a real vault of 50,000 notes it is
fast (keystroke to rows: p50 6.3 ms, p99 13.2 ms, release) but noisy. The query
`je ne suis pas sur` listed 100 notes, each built from letters picked across
several folders (`s` in `projets`, `ur` in `lecture`). D2 comes from Zed's file
finder, which matches short code file names; note names are long, human, and
deep in folders, so scattered matches flood the list.

The user asked for a smart search: find a note by its name **or its text**, in
the same box, with results that make sense.

## What the user decided

Taken in conversation on 2026-09-25, one question at a time:

1. `⌘P` searches note names **and** note text, one row per note. `⌘⇧F` stays
   the detailed view of every occurrence (search spec, slice 6 unchanged).
   Search by meaning (embeddings) is wanted later, not now.
2. A typed word matches **the start of a word** in the note, ignoring case and
   accents. Scattered letters never match. Typo tolerance is a later lever.
3. Rows are ranked in three groups: every word in the name; some in the name
   and the rest in the text; only in the text. Ties break by recently opened,
   then recently modified.
4. Content is found through a **word index** built in `inkra-index`, not by
   scanning the text per keystroke and not through an external engine.

## Scope

| In | Out, deferred below |
|---|---|
| Word-start matching on names and text, both case- and accent-insensitive | Typo tolerance, stemming, synonyms |
| A word index in `inkra-index`, kept current by loads, saves and trashes | Search by meaning (embeddings) |
| `find_notes`: one ranked list of notes for `⌘P` | A persisted index |
| Rows with a second line: an excerpt, or the note's first line | Changing `⌘⇧F` (search spec, slice 6) |
| An "indexing" line while the text loads | Relevance tuning beyond the three groups |

## Decisions

### S1. Words and folding

A **word** is a maximal run of chars for which `regex_syntax::is_word_character`
holds, the rule D4 already uses for whole-word search. `l'été` has the words
`l` and `été`; `2026-09` has `2026` and `09`; `snake_case` is one word.

A word is **folded** before it is compared, char by char: lowercase with
`char::to_lowercase`, keeping every char of the result; then NFD; then drop
every combining mark (`unicode_normalization::char::is_combining_mark`). So
`Été`, `ÉTÉ`, `ete` and `e\u{301}te` all fold to `ete`; `İ` folds to `i`; the
Kelvin sign folds to `k`. Compatibility forms are not folded: `ß` stays `ß` (not
`ss`), `ﬁ` stays `ﬁ`. The oracle shares this function; only its callers are
under test.

The fold is **two-way**: `ete` finds `été` and `été` finds `ete`. D2's
one-way accent rule (`été` does not find `ete`) is dropped for quick open; a
note app user types accents inconsistently.

The query is folded the same way, then split into **terms** on non-word chars.
Repeated terms count once. The query keeps its first 64 chars, as D2's did.

A term **matches a word** when the folded word starts with the folded term. The
term matches **whole** when they are equal.

### S2. The word index

`inkra-index` gains a `words` module: a pure block, no executor, no clock, fed
by `NoteIndex` like the corpus.

- **Content.** For each folded word, the notes that contain it and how many
  times. For each note, the list of its distinct folded words, so that
  re-indexing or removing a note touches only its own entries.
- **Fed by `NoteIndex`.** `insert(id, source, text)` re-indexes the note's
  text; `remove` and a note leaving in `set_notes` drop it. A rename changes
  nothing: the text and the id stay. Membership and the late-load rule are
  `NoteIndex`'s (search spec D1): a load for a trashed note indexes nothing.
- **Prefix lookup.** A term finds every indexed word that starts with it, in
  one ordered range, without scanning the vocabulary.
- **Shards.** The index is split into a fixed number of shards by note id. A
  snapshot is an `Arc` clone of the shard list. A write copies only the one
  shard it changes, and only when a search still holds the previous snapshot.
  Shards also let `lookup` be split across cores later, if its bench asks.
- **Tokenizing is off the foreground.** `IndexSync` already parses a load on
  the background executor; it tokenizes and folds there too, and passes the
  folded word counts with the text. A save does the same in its parse task.
  The foreground only swaps a shard's entries (budget: 2 ms per note at 50,000
  notes, measured by a bench).

The internal representation (sorted vocabulary, posting layout, shard count) is
the implementer's, behind the API, chosen by bench. The oracle is the contract.

### S3. `find_notes`

```rust
pub struct NoteQuery;       // NoteQuery::new(text, showing: Option<EntryId>, recent: &[EntryId])
pub struct Lookup;          // the query's terms resolved in the word index; cheap to share
impl WordsSnapshot { pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Lookup; }
pub enum Group { Name, NameAndText, Text }
pub struct NoteMatch {
    pub id: EntryId,
    pub path: Arc<str>,            // NFC, `.md` stripped, as NameMatch::path
    pub group: Group,
    pub name_highlights: Vec<Range<usize>>, // byte ranges into `path`, whole graphemes
    // private rank
}
pub fn find_notes(names: &NamesSnapshot, lookup: &Lookup, range: Range<usize>,
                  query: &NoteQuery, cancel: &AtomicBool, limit: usize) -> Vec<NoteMatch>;
pub fn merge_notes(chunks: Vec<Vec<NoteMatch>>, limit: usize) -> Vec<NoteMatch>;
```

A search runs in two steps on the background executor. `lookup` resolves each
term once: the notes whose text has a word starting with it, and how many
times. Then `find_notes` runs one range of `NamesSnapshot` candidates (vault
order, `names.chunks(n)`) per task, reading the shared `Lookup` for the text
side. `WordsSnapshot` comes from `NoteIndex::words()`, an `Arc` clone like
`names()` and `corpus()`.

A note is listed when **every term matches** a word of its name, of its folder
path, or of its text. It is placed in:

| Group | Rule |
|---|---|
| `Name` | every term matches a word of the stem or of the folders |
| `NameAndText` | at least one term matches in the stem or folders, the rest only in the text |
| `Text` | no term matches in the stem or folders; every term matches in the text |

Order: group first (`Name`, then `NameAndText`, then `Text`). Within a group,
in this order:

1. more terms matching **whole** words beat fewer;
2. for `Name`: more terms in the stem beat terms in folders; for `Text` and
   `NameAndText`: more occurrences of the matched words in the text beat fewer;
3. the note showing ranks last in its group (D2's penalty, made simpler);
4. MRU rank (search spec D6), recent first;
5. the most recently modified first;
6. shorter path, then vault order.

The order is total and deterministic, so `merge_notes` over any split equals
one range, as `merge_names` does.

An **empty query** lists the MRU without the note showing, as today.

"Most recently modified" needs each note's mtime: `NoteSet::build` takes it in
its tuple, `(EntryId, String, Kind, Mtime)`. A save updates it.

### S4. The excerpt

For each **visible** row, and only those, the delegate asks for a second line:

```rust
pub struct Excerpt { pub text: String, pub highlights: Vec<Range<usize>> }
pub fn excerpt(text: &str, query: &NoteQuery) -> Option<Excerpt>;
```

- `NameAndText` and `Text` rows: the line around the first word of the text
  that a term matches, cut as a `Hit`'s context (search spec D4: up to 60 bytes
  before and 100 after, on char boundaries, within the line), with every
  matched word in it highlighted.
- `Name` rows: the note's first non-empty line, as a preview, no highlight.
- A note whose text is not loaded yet: no second line.

The text comes from the `CorpusSnapshot`, read for at most the rows on screen
(10). The row stays one fixed height: two lines for every row, so
`uniform_list` can measure from row 0.

### S5. The quick-open delegate

`crates/app/src/quick_open/` changes; the picker, the modal, cancellation, the
debounce and the `NotesChanged` refresh (search spec D7, D8) do not.

- `update_matches` runs `lookup`, then `find_notes` over
  `names.chunks(2–4 × num_cpus)`, on the background executor, and merges, as it
  runs `match_names` today.
- The delegate refreshes on `NotesChanged` **and**, during the startup load, on
  `IndexSync`'s notify throttled by `BUILDING_REFRESH`, since text arriving can
  add rows. A refresh never cancels a running scan of the same query (search
  spec D7, slice-3 note).
- While indexing runs, a footer line says "Indexing 12,340 of 50,000 notes".
  Names are searchable at once; text as it loads.
- Rows: line 1 is the stem with `name_highlights`, then the folder dimmed, as
  today; line 2 is the excerpt, dimmed, with its highlights. Colours come from
  `theme.rs`.

`match_names` and the D2 matcher stay in `inkra-index`, tested and benched,
for `[[` link completion (search spec, Deferred), where subsequence matching
suits short targets.

## Performance

| Step | Budget |
|---|---|
| Keystroke to rows, 50,000 notes, release, real vault | 16 ms p99 |
| Foreground work per keystroke | 8 ms |
| Re-indexing one note on load or save, foreground part | 2 ms |
| Building the word index during the startup load | runs with the loads; adds at most 20 % to the 11.25 s measured |
| Word index memory at 50,000 notes | measured and recorded; 80 MB is the alarm line |

A very short term (one letter) may match most notes. If the bench shows a
one-letter query over budget, `find_notes` stops collecting a term's notes past
a named cap and the footer says "refine the query"; the cap and its figure are
recorded here. Not built unless the bench asks for it.

Benches join `crates/index/benches/`: index build over the 50,000-note
generated corpus, one-note re-index with a snapshot held, and `find_notes` for
one letter, a common word, a rare word, two words, and a name-only query. The
end-to-end figure is taken the way slice 4's was: the generated vault on disk
and the release-only log in the delegate.

## Testing

- **Oracle.** A naive model re-reads every note, cuts and folds its words by
  S1, applies S3's rule and order, and must list the same notes in the same
  order as `find_notes`, over random vaults and queries: accents, NFD, `ß`,
  Kelvin, apostrophes, digits, emoji, one-letter terms. 20,000 cases before the
  gate.
- **Updates.** After any sequence of insert, save, remove, `set_notes` and
  rename, the index equals one built from scratch.
- **Split.** `merge_notes` over any split equals one range.
- **Excerpt.** Char boundaries, the highlight on the matched word, the window
  rule, a word at the start and the end of a line.
- **App.** Headless `#[gpui::test]` against `MemFs`: rows by group, the second
  line, the indexing footer, a refresh during the load, every existing quick
  open test still green. A shot per state, looked at.
- Mutants zero missed on the new files.

## Slices

The search spec's slices 1–4 are built. Its remaining slices renumber:

5. **The corpus feed** (unchanged, reduced in slice 4): the `IndexSync` corpus
   tests and the startup figure.
6. **The word index**: S1, S2, the `words` module, the `NoteIndex` and
   `IndexSync` feed, `NoteSet` with mtimes, the oracles and the benches.
7. **Smart quick open**: S3, S4, S5, the shots, the end-to-end figure.
8. **Vault search** (`⌘⇧F`): the search spec's slice 6, unchanged, Task 6.0
   first.

Each slice runs the method of
[`../handoffs/2026-09-25-search.md`](../handoffs/2026-09-25-search.md): slice
lead, red team, blue team, code review, gate, stop before push.

## Evolution

| Lever | Trigger | What changes |
|---|---|---|
| Typo tolerance | The user misses notes because of typos | A term also matches words within one edit, for terms of 5 chars or more; a fourth, lower group |
| Search by meaning | The user asks (option 3 of 2026-09-25) | A second index of note embeddings beside `words`; its matches join a group below `Text`. Needs a local model and its own spec |
| Persisted word index | Startup too slow even with parallel loads | Serialize the shards; invalidate by mtime |
| One-letter cap | The bench shows a one-letter query over budget | See Performance |
| BM25-style weighting | Occurrence counts rank long notes too high | Normalize by note length inside `Text` |
