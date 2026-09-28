# Smart quick open — design

Date: 2026-09-25. Status: **reviewed** by the user on 2026-09-25; the implementation
plan is [`../plans/2026-09-25-smart-quick-open.md`](../plans/2026-09-25-smart-quick-open.md). Amends
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

## What Zed does

Checked in slice 7 against `vendor/zed-reference/crates/`. Zed keeps no word
index. Project search rereads the files, and the file finder matches
subsequences of paths, prefiltered by a `CharBag` (`fuzzy/src/char_bag.rs`: a
64-bit set of ASCII letters, digits and `-`). Word completion
(`Buffer::words_in_range`, `language/src/buffer.rs`) rescans the buffer on each
request and keeps no state between requests.

Zed's word rule is `CharClassifier::kind_with`: a char is a word char when it
is alphanumeric or `_`, plus the language scope's extra word chars. S1 uses
`regex_syntax::is_word_character` instead, for two reasons. First, quick open
must agree with D4's whole-word search, which uses the regex rule. Second, a
note has no language scope. The rules differ on combining marks and join
controls: they are word chars for `regex`, not for `is_alphanumeric`. So
`e\u{301}te` is one word here and two in Zed.

**Checked in slice 8** against `file_finder/src/file_finder.rs`,
`picker/src/picker.rs` and `search/src/text_finder/delegate.rs`:

- Zed merges matching history items into the search list by score and lifts the
  file already open to the **top** (`cmp_matches`, `:622-634`); history matches
  on the file name only (`matching_history_items`, `:687`). Inkra keeps D2's
  rule: the note showing ranks after the notes of its group that tie with it
  on the exact name and keys 1–2 (S3 key 3), since opening it again does
  nothing. The MRU is key 4, a tie-break after the group, the exact name,
  whole terms and stem terms or occurrences: a better word match beats a recent
  note. The `Name` group gives Zed's name-only rule by construction.
- Zed keeps the selected match when the query did not change
  (`set_search_matches`, `:1097-1127`); Inkra keeps the selected note by id, as
  since slice 3.
- On a worktree update Zed refreshes after `WORKTREE_UPDATE_REFRESH_DEBOUNCE`,
  200 ms (`:844`), cancels the running search, and extends the old matches when
  a cancelled search's query did not change (`extend_old_matches`, `:1112`).
  Inkra never cancels a running scan on a refresh (D7): it re-runs once the scan
  lands. Zed's matches come as a stream per worktree; ours are ranked over the
  whole vault, so a partial list is not a prefix of the final one.
- Zed's text finder draws a bounded window around a match, trims the leading
  indentation when the window starts at the line start, and highlights the match
  (`render_matched_line`, `:1344-1428`). Inkra takes all three. Zed computes it
  at render time from a rope; Inkra computes it off the foreground, after the
  merge (S4, "Built in slice 8").
- Zed's `PickerDelegate::render_footer` returns `Option<AnyElement>`, `None` by
  default (`picker.rs:408`). Inkra's takes the same shape.

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

The query is split into words as the text is, then each word is folded; each
folded word is a **term**. Repeated terms count once. The query keeps its first
64 chars, as D2's did. **Corrected in slice 7:** the cut never splits a word.
A word the cut falls inside is dropped, since its prefix would count as a
whole word; a lone word is kept, capped, so a long query never becomes empty.

A term **matches a word** when the folded word starts with the folded term. The
term matches **whole** when they are equal.

**Corrected in slice 7 (fold).** `char::to_lowercase` leaves `ς` and `σ`, `ſ`
and `s`, `ϐ` and `β` apart, so a typed `οδυσσευς` missed `ΟΔΥΣΣΕΥΣ`. The fold
now ends with one more step: each char maps to one representative of its
Unicode simple case folding orbit. The orbits come from `regex-syntax`'s public
`hir::ClassUnicode::try_case_fold_simple`, the tables D4's `(?i)` search uses.
So quick open and search fold case alike by construction. The representative
is the orbit's smallest member that is not a combining mark, lowercased when
that gives one char. Greek iota's orbit starts with U+0345, a mark, which is
why marks are skipped: the fold stays idempotent. `ı` has no orbit partner and
stays apart from `i`. The orbits are read once into a table of 1,455 pairs, at
about 1–2 ms on first use; a test checks the table against the orbit rule over
every char. Greek and Cyrillic fold at about 35 ns per char with the table,
against 160 ns with a class per char.

**Known skew.** The Rust standard library's case tables are Unicode 17;
`regex-syntax` 0.8.11's are older. For 28 uppercase letters added in Unicode 17
(U+A7CE–U+A7D5, U+16EA0–U+16EB8), `to_lowercase` joins a case pair that `(?i)`
keeps apart, so quick open finds a match D4 search does not. A test pins the
count; a bump of either crate that changes it fails with a message.

**Built in slice 7 (split, then fold).** The query is split by the same
function as the text before folding, so a word copied from a note always finds
that note, whatever a fold does at a word edge. Terms keep their first 64
folded chars and stored words their first 65: a term still matches every prefix
it can spell, and a capped word never equals a term. A 1 MB base64 word costs
65 chars. A word that folds to nothing is dropped, in the text and in the query.

**Corrected in slice 7 (marks).** Dropping every combining mark was too broad.
It removed Indic vowel signs and Thai vowels: `किताब` and `कातिब` both folded
to `कतब`, and `คน` matched `คืน`. S1 wants accent-insensitivity, not a
different word. The fold now drops a mark only when it is in this class:

```
[\p{M}&&[[\p{Script=Inherited}--[\x{3099}\x{309A}]]\p{Script=Hebrew}]]
```

So a combining mark is dropped when its Script is Inherited, except the kana
voicing marks U+3099 and U+309A. Hebrew points and cantillation are dropped
too. Inherited holds the Latin, Greek, Cyrillic and Vietnamese accents, and
two Arabic ranges: the harakat and hamza (U+064B–U+0655) and U+0670. Every
other mark stays: Indic vowel signs, virama, nukta, Thai vowels and tone marks.
The class spells `Script=` so no reader takes it for `scx`, which differs:
U+064B's `scx` is {Arab, Syrc}.

The other Arabic-script marks have Script Arabic, so they stay: U+0610–U+061A,
U+0656–U+065F, the Quranic marks U+06D6–U+06ED and U+08D3–U+08FF. So pointed
Quranic text still misses an unpointed query.

A dropped mark of combining class 0, like U+034F or a variation selector, stops
NFD's canonical reorder of the marks on either side. When the fold drops one,
it runs NFD again on the result, so the kept marks are in canonical order and
the fold stays idempotent. Only such a word pays for the second pass.

| Word | Fold |
|---|---|
| `Été`, `e\u{301}te` | `ete` |
| `Nguyễn` | `nguyen` |
| `ёлка` | `елка` |
| `كَتَبَ` | `كتب` (harakat) |
| `أحمد` | `احمد` (hamza above) |
| `שָׁלוֹם` | `שלום` (points) |
| `किताब`, `कातिब` | apart (vowel signs) |
| `คน`, `คืน` | apart (Thai vowel U+0E37) |
| `ไม่`, `ไม้` | apart (tone marks) |
| `が`, `か` | apart (kana voicing marks) |

The class also holds the variation selectors, the enclosing keycap and most
Vedic tone marks (U+0951–U+0954, most of U+1CD0–U+1CF9). A keycap still folds
to its digit. It also holds U+1133B, the Grantha bindu below: a nukta with
Script Inherited, so it is dropped although other nuktas stay. A test checks
that no char of Script Thai or Devanagari is dropped.

Thai tone marks and kana voicing marks are kept: input methods always write
them, so a query has them when the note has them. Arabic and Hebrew points are
dropped: unpointed text is the norm, so an unpointed query must find a pointed
note.

The survey behind the rule:

- Meilisearch charabia, `normalizer/nonspacing_mark.rs`: it removes
  nonspacing marks only for Hebrew, Thai, Arabic, Latin and Greek tokens.
- Lucene `ICUFoldingFilter`, `DiacriticFolding.txt`: it folds `[:Diacritic:]`,
  not every mark.
- Lucene `ArabicNormalizer` removes U+064B–U+0652 and maps the alef variants.
  Lucene `HindiNormalizer` deletes nukta and virama and keeps vowel signs.
- UTR #30: its accent removal covers Latin, Greek and Cyrillic.
- Algolia's normalization guide: it ignores the Arabic fathah, kasrah, dammah
  and sukun.

The rule keeps marks that some sources drop. Lucene's `[:Diacritic:]` also
drops the nukta U+093C, the virama U+094D, the Thai tone marks U+0E48–U+0E4B
and the kana voicing marks; `HindiNormalizer` drops nukta and virama. The fold
keeps them because they tell words apart.

The class is read once, with `regex_syntax::parse`, into a table of 33 ranges
and 705 chars. `regex-syntax` 0.8.11 has Unicode 16 tables;
`unicode-normalization` has Unicode 17, so a mark new in 17 is kept. A test pins
the 705; a crate bump that changes it fails with a message. The table is only
searched for a combining mark. Parse of a 3 KB note moves less than the 5%
run-to-run noise: zipf 38.2 → 37.8 µs, 40 words 10.1 → 10.4 µs, Greek 9.9 →
9.8 µs. The reorder costs zipf 35.6 → 36.3 µs, 40 words 9.9 → 10.3 µs, Greek
9.4 → 9.5 µs; NFD run again on every word cost zipf 11% (39.5 µs).

### S2. The word index

`inkra-index` gains a `words` module: a pure block, no executor, no clock, fed
by `NoteIndex` like the corpus.

- **Content.** For each folded word, the notes that contain it and how many
  times. For each note, the list of its distinct folded words, so that
  re-indexing or removing a note touches only its own entries.
- **Fed by `NoteIndex`.** `insert(id, content)` re-indexes the note's
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

**Corrected in slice 7 (the store).** "Fed by `NoteIndex`" now reads
`insert(id, content)`: a load or a save hands the index one `NoteContent`,
parsed off the foreground, holding the links, the text and the words. The
"Content" and "Shards" bullets above describe the first layout, which slice 7
replaced twice before it shipped:

- The plan's first layout, a `BTreeMap` of posting lists in each of 256 shards
  plus a word list per note, was estimated by the red team at about 625 MB on
  the Zipf stress corpus: every shard repeated the vocabulary, and every
  (shard, word) pair cost its own allocation.
- F, built in `414b676`, kept 256 shards but made each one a frozen CSR part,
  a fresh part and a tombstone set. It cut the estimate to about 153 MB.
- A researcher then compared F with tantivy's segment model, a global
  vocabulary, compressed postings and Meilisearch's prefix postings. A
  simulation of the stress corpus gave about 93 MB for 256 shards with varint
  postings, 60 MB for 64 shards, and 28 MB for one global segment. The
  orchestrator chose the global segment.

**Built in slice 7 (G, one global segment).** Sources: tantivy's
`ARCHITECTURE.md` and `src/indexer/log_merge_policy.rs`, Lucene's
`TieredMergePolicy`, Meilisearch's `words_prefixes_fst.rs`.

- **Ordinals.** The store gives each note a dense `u32` ordinal at its first
  write with words and never reuses it within a session. `EntryId` stays opaque;
  `inkra-vault` gains nothing. A note keeps its ordinal while it is a note of
  the index: a reload, a save, even one that empties it, reuses the ordinal;
  only `remove` or `set_notes` dropping the note frees it.
- **The segment** is immutable and shared by every snapshot. Its words are
  sorted and packed in one string. Each word's postings are varint pairs: the
  gap from the previous ordinal, then the exact count. Only a merge encodes
  them.
- **Alive** is a bitset over ordinals: a bit is set when the segment's
  postings for that ordinal are live. A remove or a rewrite clears it, so a
  trash of 5,500 notes is O(gone).
- **Fresh** holds the notes written since the last merge, each with its own
  sorted words. An ordinal is fresh, alive, or neither (a note with no
  words), never both.
- **A snapshot** is four `Arc`s: ordinals, segment, alive, fresh. The
  ordinals and fresh maps are 64 shards each (`f4d95de`), so a write under a
  held snapshot copies the shard list and one shard.
- **Merges run off the foreground.** The foreground captures three `Arc`s and
  starts recording the ordinals written from then on. The background builds a
  new segment. The foreground swaps it in, clears the bits of the ordinals
  written meanwhile, and keeps their fresh entries. It hands back what it
  replaced, freed off the foreground. So a delete during a merge stays
  deleted, and an insert during a merge is visible once after it. One merge
  runs at a time. A liveness token identifies it: an `Arc<()>` held by the
  merge and then by its result, a `Weak` in the store. A result applies only
  when its token is the running one (`Weak::ptr_eq`), so another store's
  result is dropped (this store's own result cannot be stale: it keeps its
  token alive, and a live token is never replaced). A result does not pin
  the segment it was built on.
  One merge runs at a time and the running token is replaced only once dead,
  so the token proves the base. A merge whose job and result were both
  dropped is found dead and cleared, so it never blocks the next one. A merge
  is due when fresh holds enough notes relative to the live ones, or when
  20 % of the segment is dead (Lucene's `deletesPctAllowed`). The bench sets
  the thresholds.
- **`lookup`** reads one ordered range of the segment's words per term, plus
  each fresh note's own range. It sums the counts into a dense vector indexed
  by ordinal, not a hash map. So "one ordered range" holds for the segment.
- **Short prefixes** are measured first. If a one-letter lookup misses its
  budget, the merge precomputes the postings of the 1- and 2-char prefixes
  that 100 words or more share (Meilisearch's rule). This is preferred over
  S3's cap.

### S3. `find_notes`

**Corrected in slice 8:** the block shows the API slice 8 built.

```rust
pub struct NoteQuery;       // NoteQuery::new(text, showing: Option<EntryId>, recent: &[EntryId])
pub struct Lookup;          // the query's terms resolved in the word index; cheap to share
pub struct IndexSnapshot;   // NoteIndex::snapshot(): names, corpus, words
impl IndexSnapshot {
    pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool)
        -> Option<Lookup>; // None: cancelled
    pub fn chunks(&self, n: usize) -> Vec<Range<usize>>;
}
pub enum Group { Name, NameAndText, Text }
pub struct Ranked;          // a note a range kept: its private rank only
pub struct NoteMatch {
    pub id: EntryId,
    pub path: Arc<str>,            // NFC, `.md` stripped, as NameMatch::path
    pub group: Group,
    pub name_highlights: Vec<Range<usize>>, // byte ranges into `path`, whole graphemes
    // private rank
}
pub fn find_notes(snapshot: &IndexSnapshot, lookup: &Lookup, range: Range<usize>,
                  query: &NoteQuery, cancel: &AtomicBool, limit: usize) -> Vec<Ranked>;
pub fn merge_notes(snapshot: &IndexSnapshot, chunks: Vec<Vec<Ranked>>,
                   query: &NoteQuery, limit: usize) -> Vec<NoteMatch>;
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

Order: group first (`Name`, then `NameAndText`, then `Text`), then the exact
name (amended below). Within a group, after the exact name, in this order:

1. more terms matching **whole** words beat fewer;
2. for `Name`: more terms in the stem beat terms in folders; for `Text` and
   `NameAndText`: more occurrences of the matched words in the text beat fewer;
   **amended 2026-09-28**: BM25 over the text terms, not a raw count
   (`2026-09-28-ranking-idf-design.md`);
3. the note showing ranks after the notes of its group that tie with it on
   the exact name and keys 1–2 (D2's penalty, made simpler);
4. MRU rank (search spec D6), recent first;
5. the most recently modified first;
6. shorter path, then vault order.

**Amended 2026-09-27, decided by the user: exact name first.** A note whose
name is the query ranks first in its group, before key 1: the note `plan` for
the query `plan`, `Plan` for `plân`. The name is **exact** when the stem's S1
words, folded and in path order, equal the query's typed words, folded and in
typed order.

- Repeats count: `plan plan` is exact for a stem `plan plan`, not for `plan`.
- Non-word chars and whitespace do not count, on either side: `Plan - B` is
  exact for `plan b`, and `2026-09` for `2026 09`.
- A word that folds to nothing is dropped on both sides, as S1 drops it.
- The typed words are the query's after S1's 64-char cut. They are capped at
  64 folded chars, as terms, and stem words at 65, as stored words. So a stem
  word over 64 folded chars is never exact.
- The folders do not count: `plan/notes` is not exact for `plan`.
- An empty query is never exact.

An exact stem holds every term, so an exact note is always in `Name`. The key
still sits right after the group, so the field order of the rank stays S3's key
order. Keys 1–6 keep their numbers.

The order is total and deterministic, so `merge_notes` over any split equals
one range, as `merge_names` does.

An **empty query** lists the MRU without the note showing, as today.

"Most recently modified" needs each note's mtime: `NoteSet::build` takes it in
its tuple, `(EntryId, String, Kind, Mtime)`. A save updates it.

**Built in slice 8 (S3).** The plan's slice 8 section holds the reasons in full.

- **Mtimes.** A corpus slot holds its text and that text's mtime. A load writes
  its `Loaded::stamp` and a save its `NoteSaved::stamp`, through
  `NoteContent::parse(text, modified)`. A rebuild keeps a slot's mtime when the
  note keeps its text, else takes the scan's. So the mtime always describes the
  text the index holds: after an external edit, a note keeps its old mtime until
  its reload lands. `NoteSet::build` takes `(EntryId, String, Kind, Mtime)`.
  `Mtime` gains no order (the vault rule); key 5 orders the `SystemTime` inside.
  Key 5 is a heuristic: a synced file's mtime need not follow the order of
  edits, and notes imported together share one, so ties fall to key 6.
- **`find_notes` takes the corpus**, for key 5, inside one `IndexSnapshot`:
  `find_notes(snapshot, lookup, range, query, cancel, limit)`. The snapshot
  holds the names, corpus and words, and only `NoteIndex::snapshot()` builds
  it, so the names and corpus always come from one build: no runtime check or
  fallback exists.
- **A term's place.** A term is placed in the name when a word of the stem or
  folders starts with it, else in the text. Key 1 counts the terms matching
  whole in their place. Key 2 counts, for `Name`, the terms matching a stem
  word; for `NameAndText` and `Text`, the occurrences of the terms placed in the
  text. So a `Name` row never ranks by its text. The exact name (amended
  2026-09-27), before key 1, compares the stem words with the typed words,
  repeats kept, not with the terms. `NoteQuery` keeps the typed words beside
  its terms; the rank reads the stem words of `NameWords` in place, only for a
  `Name` row, with no allocation per note.
- **Name words** are folded once per build, off the foreground, and stored
  CSR-style beside the D2 candidates (`NameWords`): one string, one word array,
  one start per note and an end sentinel, no allocation per word. `names`
  stores `NameWords` and exposes it (`words()`), never reads it; `find` reads
  it. `NamesSnapshot::heap_bytes` reports them, and the launch line prints
  `names N bytes`.
- **Highlights** light the matched prefix of each word, widened to whole
  graphemes of the **path**: a word can start inside a grapheme (`🎉\u{301}x`) or
  end inside one (`a🏻`). A range may start before the stem when a grapheme
  straddles the last `/`; the row splits it between folder and stem. Highlights
  are computed only for the rows a range returns.

### S4. The excerpt

For each **visible** row, and only those, the delegate asks for a second line:

**Corrected in slice 8:** the block shows the API slice 8 built.

```rust
pub struct Excerpt { pub text: String, pub highlights: Vec<Range<usize>> }
pub fn excerpt(snapshot: &IndexSnapshot, found: &NoteMatch, query: &NoteQuery,
               cancel: &AtomicBool) -> Option<Excerpt>;
```

- `NameAndText` and `Text` rows: the line around the first word of the text
  that a term matches, cut as a `Hit`'s context (search spec D4: up to 60 bytes
  before and 100 after, on char boundaries, within the line), with every
  matched word in it highlighted.
- `Name` rows: the note's first prose line (amended below), as a preview, no
  highlight.
- A note whose text is not loaded yet: no second line.

The text comes from the `CorpusSnapshot`, read for at most the rows on screen
(10). The row stays one fixed height: two lines for every row, so
`uniform_list` can measure from row 0.

**Built in slice 8 (S4).** `excerpt(snapshot, found, query, cancel) ->
Option<Excerpt>` reads the text at the match's vault order and checks the slot
holds the match's id; the group comes from the match, so no vault order leaks
into the public API.

- **Rows first, then the excerpts of the rows on screen.** The rows apply as
  soon as the merge lands. The picker's `render_rows` passes the rows it draws
  to `PickerDelegate::rows_shown`, a no-op by default. Quick open spawns one
  background task per row not asked for yet, so the rows on screen (10) spread
  across the cores, and each second line fills when its excerpt lands.
  Scrolling asks for the rows it shows. `uniform_list` also draws row 0 alone
  each frame to measure it, so row 0 is asked even when scrolled away: at most
  one excerpt more. A row is two lines tall before and after its excerpt lands,
  so nothing moves; `row_label` is the path alone until then. Zed computes the
  excerpt at render time from a rope with a line index; ours are plain strings,
  so the scan stays off the foreground.
- **Keying.** The rows, and the snapshot and query that found them, form one
  `List` (`quick_open/list.rs`) keyed by the search id. An excerpt lands only on
  the list that asked for it, and only while that list's cancel flag is clear.
  The next user query sets the flag, and so does the list's drop: the next list,
  or the picker closing.
- **A background refresh asks again for the excerpts of the rows it keeps.**
  The refresh runs because the index changed: a note that loads turns a blank
  line into a preview, and a save moves the matched window. `IndexSnapshot`
  cannot tell that a note's text is unchanged without a new public API. Until
  the new excerpt lands, a kept note shows its previous one, so no line blanks.
  The cost is at most the rows on screen per refresh, off the foreground.
- Keystroke to rows is now keystroke to names: Zipf 0.22–3.33 ms, 40 words
  0.17–1.49 ms. Before, the rows waited for 100 serial excerpts; the queries
  matched deep in the text gain most (Zipf word 1,000 2.66 → 0.54 ms, rare word
  2.27 → 0.46 ms; 40 words rare word 1.89 → 0.46 ms). The 10 excerpts on screen
  add up to 0.3 ms after the rows (Performance, "Measured, slice 8").
- The first matching word is searched in the first `EXCERPT_SCAN_BYTES` (1 MiB)
  only, `cancel` read every 64 KB; past the limit the row has no second line.
  The window is built around the word's first `WORD_MAX_CHARS` chars, so a 1 MB
  word gives a short excerpt. The scan stops once the window is known.
- The line's leading whitespace is trimmed when the window starts at the line
  start (Zed). A matched word is lit on its matched prefix, widened to whole
  graphemes of the excerpt.
- A `Name` row, and every row of an empty query, shows the first prose line
  (amended below), cut to 160 bytes, found within the same 1 MiB. A blank note
  has no excerpt; the row keeps an empty second line.

**Amended 2026-09-27, decided by the user: the first prose line.** The
second line shows prose, not markdown syntax.

- A `Name` row, and every row of an empty query, shows the first prose line.
  The walk skips, in order: a leading front matter block, blank lines, lines
  blank once their markup is dropped (`---`, `- `, `>`), and a leading heading
  that names the note. The first remaining line is shown.
- **Front matter** is pulldown-cmark 0.13's YAML metadata block on the first
  line, as the editor styles it: a line of exactly `---` and trailing
  whitespace, up to the next line of exactly `---` or `...` and trailing
  spaces. The line after the opening is neither blank nor a closing fence.
  Otherwise there is no front matter: a first `---` is a thematic break, blank
  once dropped. `inkra-markdown` owns the rule: `front_matter(lines) ->
  Option<usize>`, fed one line at a time and reading none past the closing
  fence, so `index` feeds it lines with its cancel reads. A test checks it
  against pulldown-cmark on generated texts. `index` feeds no line unless the
  text starts with `---`: its one shortcut, so a long first line is not read.
- A heading **names the note** when its S1 words, folded and capped as stem
  words, equal the stem's words in order: S3's exact-name rule, with the
  heading's words in place of the typed words. `# Plan` and `## PLÂN !` name
  `plan`; `# The plan` and `# Projects plan` (for `projects/plan`) do not. Only
  the first line with prose can be that heading, which may sit in a quote or an
  item (`> # Plan`). It is an ATX heading, or a
  setext heading whose underline is the next line (`Plan` then `====`): only
  that one line more is read, and both lines are skipped. A heading that does
  not name the note is shown, its markup dropped.
- **Markup dropped** from the shown line: its leading block markers (a
  heading's `#` runs, `>`, `-`, `*`, `+`, `1.`, `1)`, task boxes, nested or
  combined like `> - [ ]`, a thematic break) and its emphasis, strong and
  code-span delimiters. `_` inside a word is not emphasis (CommonMark), so
  `snake_case_name` stays whole. Link, strikethrough, raw HTML and code-block
  syntax stays.
- `inkra-markdown` owns the rule: `markup(line, at_line_start) ->
  Vec<Range<ByteOffset>>`, one pulldown-cmark pass over one line, with
  `parse`'s options. Between an element's children, the bytes no leaf covers
  are dropped when the element is a heading, quote, list, item, emphasis or
  strong. A code span drops its backtick runs; a thematic break drops whole.
  `index` parses no markdown.
- A **text row's window** stays as it was. Its markup is dropped the same way,
  and each highlight is mapped through the drop, then widened to whole
  graphemes of the shown text. The markup is parsed over the window widened
  within its line by up to 1 KiB each side, from the first non-blank byte when
  that reaches the line start (an indent would read as a code block), so a delimiter whose partner lies
  outside the window drops too; the ranges inside the window are kept, cut at
  its edges. When the widened piece starts inside its line, it has no block
  marker: `markup` parses it after a no-break space, which the emphasis rules
  read as whitespace and which no block marker can follow. A matched word
  inside the markup, like an ordered item's number, is not lit.
- **Bounds.** The 1 MiB scan and the 64 KB cancel reads cover the front matter
  and every skipped line. The first KiB of a line is parsed, and a KiB each side
  of a window; markup closing past it stays, and a line that KiB skips is
  skipped whole. The 160-byte cut
  applies after the drop, on a char boundary, so it never splits a marker. A
  skipped line that reaches the scan limit ends the walk: no second line.
- **Measured** (`find` bench, `--quick`, top 100 rows, before → after): a
  `Name` row's preview 3.5 → 44 µs per 100 rows (`workshop`, `e`), an empty
  query's 20 recent 0.7 → 8.5 µs; a window row within noise to 11 % (Zipf top
  word 325 → 360 µs, the markup parsed over a KiB each side). The 10 rows on
  screen add under 5 µs.

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

~~`match_names` and the D2 matcher stay in `inkra-index`, tested and benched,
for `[[` link completion (search spec, Deferred), where subsequence matching
suits short targets.~~ Amended 2026-09-29: the matcher is deleted 2026-09-29, see [`2026-09-29-names-delete-design.md`](2026-09-29-names-delete-design.md); restore from `fee89d1` or start from Zed's `fuzzy`.

**Built in slice 8 (S5).**

- `update_matches` takes the names, corpus and words in one borrow of the index,
  then runs off the foreground: `lookup` on one task (`None`: cancelled),
  `find_notes` over `names.chunks(num_cpus)` sharing the `Lookup`, then
  `merge_notes` on one task; the excerpts follow (S4). S5 said 2–4 ranges per
  core; the `find` bench measured one per core faster in almost every case,
  since more ranges cost more merge (Performance, "Measured, slice 8").
- `PickerDelegate` gains `render_footer` (`None` by default, as Zed's) and
  `row_height` (`UI_LINE_HEIGHT` by default). Quick open's rows are `2 ×
  UI_LINE_HEIGHT`: the name line, then the excerpt, dimmed, with its highlights,
  or an empty line. The footer reads "Indexing 12,340 of 50,000 notes" while
  `is_building()` and not `awaits_first_build()`, where "Scanning the vault…"
  already speaks (O5).
- **Refresh.** `IndexWatch` (`app/src/index/watch.rs`) holds the `(revision,
  building)` last seen and one throttle task; the backlinks panel and quick open
  share it. Quick open observes `IndexSync`: while it builds, at most one
  refresh per `BUILDING_REFRESH` (100 ms), so text arriving adds rows; after, at
  once. A `NotesChanged` still waits 200 ms. `update_matches` marks the revision
  seen when it takes its snapshots, so one build runs one scan.
- **D7.** A refresh from `NotesChanged` or from a notify, while a scan runs,
  never cancels it: it marks the query dirty, and when the scan lands one re-run
  starts, keeping the selected note, with no wait. A keystroke still cancels and
  replaces the scan and clears the mark. A keystroke waits no debounce behind a
  background run and cancels it; it waits `SEARCH_DEBOUNCE` only behind a user
  search (a keystroke's, or the first names after the scan). The first names
  after the scan still list at once (O5), and a build during that first scan no
  longer cancels it. A confirm clears the mark. A background run's result keeps
  the selected note and the scroll; when the kept note is gone, row 0 is
  selected and scrolled to.
- `row_label` is `path: excerpt`, both through `one_line`; the path alone until
  the excerpt lands (S4).
- **Shots**: `quick-open-groups` (a row per group, each with its second line),
  `quick-open-text-only`, `quick-open-indexing` (the footer), and
  `quick-open-results`, `-empty`, `-no-match`, `-graphemes`, `-scanning`
  re-rendered on two-line rows.

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

### Measured, slice 7

`cargo bench -p inkra-index --bench words` (full run), Apple M5 Pro, 18 cores,
release, load average 1.5–2.3, on the final store (G, `FRESH_MIN` 256,
`FRESH_RATIO` 8). Two generated corpora of 50,000 notes of 1–5 KB: **40 words**,
the startup vault's vocabulary (the realistic case), and **Zipf**, 30,000
generated words with about 250 distinct words per note (the stress case). The
build inserts every note as the app does: merges start when due and apply
between inserts, and one settle merge follows the last insert. The bench runs
each merge inline, so no insert lands during one; in the app merges run on
the background while loads keep landing.

| Case | 40 words | Zipf | Budget | Met |
|---|---:|---:|---:|---|
| `NoteContent::parse`, one 3 KB note (background), after the parse wins | 15.7 µs (was 45.0) | 56 µs (was 76.3) | none | ×50,000: 0.79 s / 2.8 s of CPU, spread over the cores |
| `parse`, a Greek note, after the parse wins | 14.6 µs (was 78.9) | — | none | — |
| build: foreground total, 50,000 inserts | 16.6 ms | 16.7 ms | 20 % of startup | yes |
| build: one insert p50 / p99 / max | 209 ns / 542 ns / 583 µs | 250 ns / 625 ns / 568 µs | 2 ms | yes |
| build: a merge's apply, max | 3.1 µs | 3.6 µs | 2 ms | yes |
| build: background merges, total / largest (31 due, 1 settle), before the seeded hash in the merge | 88.9 ms / 11.0 ms | 777 ms / 88.0 ms | none | — |
| insert one note, a words snapshot held (settled / 6,250 fresh) | 157 ns / 10.6 µs | 181 ns / 10.7 µs | 2 ms | yes |
| insert a new note, a words snapshot held | 46.3 µs | 47.9 µs | 2 ms | yes |
| insert one note, words and corpus snapshots held | 2.20 ms; after the corpus chunks 2.0 µs | 2.12 ms; after 2.8 µs | 2 ms | yes, after the corpus chunks (below; the "after" figures are the chunk run's, whose "before" read 2.30–2.33 ms) |
| insert a 1 MB base64 note, a snapshot held | 522 ns / 18.1 µs | 412 ns / 17.4 µs | 2 ms | yes |
| `set_notes`, nothing gone | 1.97 ms | 1.94 ms | 8 ms | yes |
| `set_notes`, 5,500 gone | 2.53 ms | 2.59 ms | 8 ms | yes |
| `set_notes`, 5,500 gone, a snapshot held | 2.58 ms | 2.65 ms | 8 ms | yes |
| merge 50,000 fresh into an empty segment (background) | 34.9 → 24.2 ms | 360 → 254 ms | none | the merge's hash map moved from SipHash to seeded foldhash (`837e194`) |
| merge 12,500 rewritten into 50,000 (background) | 13.9 → 11.0 ms | 130 → 103 ms | none | same |
| `lookup` settled: `e` | 394 µs | 2.46 ms | 8 ms | yes |
| settled: top word / word 1,000 / rare / two words / no match | 217 µs / — / 30 µs / 338 µs / 36 ns | 126 µs / 41 µs / 29 µs / 189 µs / 75 ns | 8 ms | yes |
| `lookup` with 6,250 fresh (the policy's most): `e` | 941 µs | 4.49 ms | 8 ms | yes |
| 6,250 fresh: top word / word 1,000 / rare / two words / no match | 805 µs / — / 308 µs / 1.46 ms / 249 µs | 1.42 / 1.52 / 1.09 / 3.81 / 0.88 ms | 8 ms | yes |

- **Memory.** `heap_bytes` after the settle: 4.4 MB (40 words), 22.6 MB
  (Zipf), both under the 80 MB alarm line. The bench's RSS grows by 25.5 MB
  and 183 MB across the build, and does not drop when the index is dropped:
  the delta is memory the macOS allocator keeps after the build's peak
  (fresh word lists and merge temporaries), not the live index. The RSS
  alarm stands only for the stress case, and there it measures the peak.
  Lever: bound the peak (a merge's temporaries), measured by the process
  footprint during a merge; delta-coded postings would not change it.
- **Ordinals.** After 5,500 notes removed and inserted again, 55,500 ordinals
  for 50,000 notes. A save keeps its note's ordinal. No compaction is needed.
- **Policy sweep** (quick runs, Zipf / 40 words). `FRESH_RATIO` 2: largest
  merge 174 / 16.5 ms, `e` with the fresh tier full 21.3 / 4.84 ms. 4: 112 /
  12.9 ms, 7.08 / 1.79 ms. **8: 87.5 / 11.2 ms, 4.30 / 0.96 ms.** 16: 64.4 /
  8.3 ms, 3.19 / 0.67 ms, but twice the background CPU of 8. `FRESH_MIN` 1,024
  against 256 changed nothing within noise. 8 halves a lookup made during the
  startup load against 4, for about a third more background merge CPU.
- **Short prefixes.** `lookup` on one letter stays under 8 ms on both corpora
  (2.46 ms settled on Zipf, 4.49 ms while loading), so the prefix postings
  are not built.
- **The miss.** One insert while a words and a corpus snapshot are both held
  costs 2.1–2.2 ms. The words part costs under 11 µs; the rest is
  `Corpus::set` copying its 50,000 slots under a held corpus snapshot (slice 5
  code). Lever: chunk the corpus slots, so a write copies one chunk.
  **After (chunked corpus slots, 256 slots per chunk):** 2.0 µs (40 words)
  and 2.8 µs (Zipf) settled, 26 µs and 28 µs with 6,250 fresh. Budget met.
- **Settle.** When the startup load leaves nothing to load, one merge folds
  every fresh note, and `Indexed` is marked when it applies. Later saves,
  creates and external edits wait in the fresh tier until the policy merges
  them, so none of them re-encodes the whole segment. The settle is asked on
  the turn where the startup load ends, whichever turn ends it (a load or a
  rebuild). A load or a save that lands while the settle runs waits in fresh
  too, and `Indexed` is still marked when the settle applies.
- **Priority.** Merges run at `Priority::Low`, as the loads do and as Zed's
  scanner does, so a query's scan (slice 8) goes first.

**The startup figure**, taken on 2026-09-26 at 19:51 by the slice lead in one
sitting: Apple M5 Pro, release, warm cache, screen unlocked, load average 1.2
at the start and 3.7 at the end (a red-team agent began building near the
end). Four runs each through `/tmp/launch-rss.sh` (`/tmp/launch-figure.sh`
plus the process RSS one second after the launch line); runs 2–4 below.
"Before" is slice 6's release build (`/tmp/inkra-app-final`, which prints
`indexed` to 0.1 s); "after" is this slice's (`1c03887`'s code).

| Vault | Build | First frame | Names ready | Indexed | Longest turn | Words | RSS |
|---|---|---:|---:|---:|---:|---:|---:|
| 50,000 nested | before | 152–157 ms | 275–280 ms | 1.1, 1.2, 1.1 s | 2.3–2.4 ms | — | 321 MB |
| 50,000 nested | after | 144–159 ms | 266–280 ms | 1.32, 1.17, 1.30 s | 2.4 ms | 4.4 MB | 346 MB |
| 50,000 flat | before | 157–163 ms | 362–390 ms | 1.3, 1.2, 1.3 s | 2.4–2.5 ms | — | 329 MB |
| 50,000 flat | after | 140–158 ms | 366–385 ms | 1.39, 1.39, 1.38 s | 2.3–2.5 ms | 4.4 MB | 354 MB |

- **The +20 % budget is met, with little margin on the nested vault.**
  `indexed` now includes the settle merge. Nested: about 1.13 s before (the
  mean of rounded figures) against 1.26 s after, +11 %; the worst pair, 1.1 s
  against 1.32 s, is +20 %. Flat: about 1.27 s against 1.39 s, +9 %.
- **The foreground is unchanged:** the longest turn stays 2.3–2.5 ms, and the
  first frame and names ready do not move.
- **Memory in the app:** the word index holds 4.4 MB (`heap_bytes`, from the
  launch line); the process RSS grows by 25 MB on both vaults, as in the
  bench's 40-word case.
- The cost is background CPU: `NoteContent::parse` (about 45 µs a note in
  the bench, links and words together, when these figures were taken) and
  the merges.
- **After the parse wins** (`efc0a52`–`d911ae6`: each distinct spelling
  folded once, ASCII word chars without a table search, foldhash's seeded
  `fast::RandomState` for the spelling counts), `NoteContent::parse` costs
  about 16 µs a note on the 40-word corpus. The table above predates them.
  The startup figure after them, at the gate (`eb405c3`, taken by the
  orchestrator on 2026-09-26, M5 Pro, warm, idle, runs 2–4 of 4 in one
  sitting, slice 6's `/tmp/inkra-app-final` against `/tmp/inkra-app-slice7`):

  | Vault | indexed, slice 6 | indexed, slice 7 | Longest turn | RSS |
  |---|---:|---:|---:|---:|
  | nested | 949–958 ms | 949–978 ms (+0 to +3 %) | 1.6–1.7 ms | 321 → 347 MB |
  | flat | 1.0 s | 1.03 s (about +3 %) | 1.7–1.8 ms | 329 → 358 MB |

  The +20 % budget is met with a wide margin. The first frame is unchanged
  (125–138 ms both). The words themselves weigh 4.4 MB (`heap_bytes`); the
  rest of the RSS growth is merge peaks the allocator keeps.
- **The spelling counts hash with a per-map seed plus a process seed**
  (foldhash's `fast::RandomState`), because a note is an untrusted file (a
  git clone, a sync). The seed-free Fx hasher tried first
  (`5b8b165`) let crafted words collide: 20,000 colliding 16-char words
  (339 KB) parsed in 446 ms against 2.8 ms with foldhash, and 100,000
  (1.7 MB) in 16.4 s against 15 ms. Zed's `collections` uses rustc-hash
  (Fx); Inkra deviates for flooding resistance. The seed costs Zipf 44 → 56 µs
  a note and nothing measurable on 40 words or Greek. A seed-free foldhash
  measures 43.6 µs on Zipf, so the cost is probably the map's changing
  iteration order before the sort, not the hash. The bench re-parses one
  note, which may favour a fixed order. The standard library's SipHash
  `RandomState` costs 7.6 µs against foldhash's 3.5 µs for the spelling
  counts of 450 words.
- **`foldhash` is accepted as an exception to the dependency rule's 12
  months** (0.2.0 was released 13 months before slice 7): it is already in
  the lock through hashbrown, maintained, and it gives the seeding against
  flooding that the spelling counts need.

### Measured, slice 8

`cargo bench -p inkra-index --bench find`, Apple M5 Pro, 18 cores, release, load
average about 1 at the start. The corpora are slice 7's (50,000 notes, 40 words
and Zipf, words settled), with the `names` bench's long-stem paths. Medians;
each includes the `lookup`.

| Query | Zipf, one core | Zipf, 10 threads, 1 range each | 40 words, one core | 40 words, 10 threads |
|---|---:|---:|---:|---:|
| `e` | 7.88 ms | 3.54 ms | 5.78 ms | 1.52 ms |
| top word | 4.65 ms | 0.94 ms | 5.70 ms | 1.39 ms |
| rare word | 1.69 ms | 0.44 ms | 1.69 ms | 0.43 ms |
| two words | 3.89 ms | 0.79 ms | 7.35 ms | 2.07 ms |
| no match | 2.49 ms | 0.46 ms | 2.54 ms | 0.48 ms |
| a word in names only | 2.29 ms | 1.23 ms | 2.29 ms | 1.24 ms |
| empty query, 20 recent | 0.52 ms | 0.18 ms | 0.43 ms | 0.17 ms |

- **Ranges.** Two and four ranges per thread were slower in almost every case
  (Zipf `e`: 3.54, 3.99, 4.81 ms for 1, 2, 4), since each range returns its own
  top 100 and computes their highlights before the merge. Quick open runs one
  range per core (`f966bf0`).
- **Ranges, re-run** (fix batch, in the "after" run of the pipeline split
  below). Since `96c2f8c` a range returns ranks only, and the highlights are
  built for the merged top only, so the reason above no longer holds. Medians
  for 1×, 2×, 4× ranges per thread:

  | Query | Zipf | 40 words |
  |---|---:|---:|
  | `e` | 3.33 / 3.35 / 3.39 ms | 1.22 / 1.29 / 1.31 ms |
  | top word | 0.92 / 0.93 / 0.97 ms | 1.17 / 1.22 / 1.28 ms |
  | word 1,000 | 0.54 / 0.55 / 0.55 ms | — |
  | rare word | 0.47 / 0.44 / 0.41 ms | 0.46 / 0.44 / 0.41 ms |
  | two words | 0.78 / 0.80 / 0.79 ms | 1.51 / 1.54 / 1.55 ms |
  | no match | 0.49 / 0.49 / 0.48 ms | 0.49 / 0.49 / 0.48 ms |
  | a word in names only | 0.56 / 0.55 / 0.57 ms | 0.56 / 0.56 / 0.57 ms |
  | empty query, 20 recent | 0.22 / 0.21 / 0.22 ms | 0.17 / 0.17 / 0.18 ms |

  The gaps have shrunk to a few percent. 4× wins on the rare word (−13 % and
  −12 %) and by 2 % on no match; 1× wins on `e`, the top word and two words (1–9
  %). Quick open keeps one range per core: 1× still wins on the costliest
  queries.
- **The pipeline**, as the delegate first ran it (lookup, `find_notes` over 10
  ranges, merge, 100 excerpts): Zipf 0.17–3.54 ms, worst `e`; 40 words 0.17–1.96
  ms. Budget 16 ms: met with a wide margin, so S3's one-letter cap is not built.
- **The pipeline split** (fix batch): the rows apply after the merge, and only
  the 10 rows on screen get an excerpt, one thread each. `keystroke to names`
  is lookup, 10 ranges and merge; `keystroke to excerpts` adds the 10 excerpts.
  "Before" is the pipeline above, re-run at `7350bc3`; its names equal the 1×
  ranges group. Load average 1.9 before the "before" run, 1.1 before the
  "after" run; each run raises it to about 6. Medians. The bench spawns a thread
  per excerpt, which the app's executor does not, so the excerpt figures are an
  upper bound.

  | Query | Zipf before: names / rows | Zipf after: names / excerpts | 40 words before | 40 words after |
  |---|---:|---:|---:|---:|
  | `e` | 3.31 / 3.32 ms | 3.33 / 3.46 ms | 1.26 / 1.27 ms | 1.27 / 1.39 ms |
  | top word | 0.91 / 1.18 ms | 0.91 / 1.03 ms | 1.17 / 1.21 ms | 1.17 / 1.27 ms |
  | word 1,000 | 0.54 / 2.66 ms | 0.54 / 0.74 ms | — | — |
  | rare word | 0.46 / 2.27 ms | 0.46 / 0.76 ms | 0.46 / 1.89 ms | 0.46 / 0.68 ms |
  | two words | 0.78 / 1.09 ms | 0.79 / 0.90 ms | 1.48 / 1.49 ms | 1.49 / 1.60 ms |
  | no match | 0.49 / 0.49 ms | 0.50 / 0.50 ms | 0.50 / 0.49 ms | 0.49 / 0.49 ms |
  | a word in names only | 0.55 / 0.56 ms | 0.55 / 0.69 ms | 0.55 / 0.56 ms | 0.56 / 0.71 ms |
  | empty query, 20 recent | 0.19 / 0.19 ms | 0.22 / 0.32 ms | 0.19 / 0.19 ms | 0.17 / 0.27 ms |

  Before, the rows appeared with the last excerpt ("rows"). After, they appear
  with the names, and the second lines fill by "excerpts".
- **Excerpts for 100 rows**: 3–700 µs for `Name` rows (a first line) and up to
  2.1 ms for `Text` rows whose match lies deep in the text. Adversarial: 1 MB of
  punctuation then a 1 MB word, 1.48 ms (a 290-byte excerpt); a 10 MB line with
  the match at its end, 3.77 ms, `None` past the 1 MiB scan limit.
- **The end-to-end figure** (keystroke to rows in the release app on the
  generated vault, the `quick open: N matches` log), taken by the orchestrator
  on 2026-09-27 at `00337b2` through System Events, one key every 500 ms, after
  `indexed`: `roadmap cafe` typed, then erased back to `r`. 23 keystrokes,
  100 matches each: 1.9–6.0 ms from the edit to the rows. The session's first
  query is the exception, 13.2 ms for `r` and 14.1 ms for `e` in a second
  launch: cold, as the same `r` typed again answers in 4.7 ms. Both stay under
  the 16 ms budget; the first query's margin is the thinnest.
- **Startup at the gate** (taken by the orchestrator on 2026-09-27, M5 Pro,
  warm, idle, runs 2–4 of 4 in one sitting, nested 50,000-note vault,
  `/tmp/inkra-app-slice7` against `/tmp/inkra-app-slice8` at `00337b2`):

  | | slice 7 | slice 8 |
  |---|---:|---:|
  | first frame | 148–162 ms | 157–179 ms |
  | names ready | 239–253 ms | 230–266 ms |
  | indexed | 994–1000 ms | 986–1010 ms |
  | longest turn | 1.6–1.8 ms | 1.6–1.7 ms |
  | RSS | 347–350 MB | 353–358 MB |

  Unchanged within the sitting's noise. RSS grows about 6 MB: the name words
  (`names 2233894 bytes`) and the mtimes beside the corpus texts.

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
   tests, the startup timeline and figure, and parallel startup loads.
6. **Open first, scan behind**: the window draws before the vault scan
   ends (own spec). Moved ahead of the word index on 2026-09-25: slice 5's
   timeline measured the first frame at 489–514 ms at 50,000 notes, against
   the 100 ms budget.
7. **The word index**: S1, S2, the `words` module, the `NoteIndex` and
   `IndexSync` feed, `NoteSet` with mtimes, the oracles and the benches.
8. **Smart quick open**: S3, S4, S5, the shots, the end-to-end figure.
9. **A persisted index**: warm start from serialized names, words and corpus
   (the Evolution lever below, brought forward; own spec).
10. **Vault search** (`⌘⇧F`): the search spec's slice 6, unchanged, Task 6.0
   first.

Each slice runs the method of
[`../handoffs/2026-09-25-search.md`](../handoffs/2026-09-25-search.md): slice
lead, red team, blue team, code review, gate, stop before push.

## Evolution

| Lever | Trigger | What changes |
|---|---|---|
| Words in scripts without spaces (Chinese, Japanese, Thai) | A note in one of these scripts is missed by a word in its middle | A word is a maximal `\w` run cut at `WORD_MAX_CHARS` (`words/tests.rs:60-66` pins 30 × `한` as one word), so in such text only a run's first chars are prefix-searchable. Cheap fix: index character bigrams for Han, Kana and Thai runs, as charabia's fallback does; a dictionary segmenter (jieba, lindera) is the costly one. Needs a reindex; the 2026-09-27 audit found it recorded nowhere |
| Typo tolerance | The user misses notes because of typos | A term also matches words within one edit, for terms of 5 chars or more; a fourth, lower group |
| Search by meaning | The user asks (option 3 of 2026-09-25) | A second index of note embeddings beside `words`; its matches join a group below `Text`. Needs a local model and its own spec |
| Persisted word index | Startup too slow even with parallel loads | Serialize the segment; invalidate by mtime. `EntryId` and the store's ordinals are minted per session, so the file keys notes by a stable key (path or `FileIdentity`) and remaps them to the scan's ids at load (slice 7 blue team) |
| One-letter cap | The bench shows a one-letter query over budget | See Performance |
| Bound the merge peak | The process footprint during a merge passes 80 MB on a real vault (the Zipf stress case's RSS grows 183 MB across its build and keeps it) | Merge in word ranges, or encode postings straight from the fresh notes without a whole inverted temporary; measure the footprint during a merge, not the RSS after |
| Chunked corpus slots | An insert with a corpus snapshot held passes 2 ms. **Fired in slice 7:** 2.1–2.2 ms with a words snapshot also held; the lever is not in the plan's list, so the orchestrator decides. **Shipped in slice 7:** 256 slots per chunk, chosen by bench over 256, 1,024 and 4,096 (settled insert, both snapshots held: 2.0–2.8 µs, 14–15 µs, 68–86 µs). Insert with both snapshots held, settled: 2.33 ms → 2.0 µs (40 words), 2.30 ms → 2.8 µs (Zipf); 6,250 fresh: 2.25 ms → 26 µs, 2.07 ms → 28 µs. Insert with a corpus snapshot held (search bench): 2.33 ms → 1.0 µs. `corpus()`, `set_notes` and the full-corpus search scans unchanged within noise | `Corpus` keeps its slots in chunks behind `Arc`s, so a write under a snapshot copies one chunk, not 50,000 slots |
| A shared `Lexicon` | A third packed, sorted word list appears (slice 8's name words, or slice 9's persisted segment) | One `Lexicon` type (packed bytes, bounds, `first_at_or_after`) shared by `NoteWords` and `Segment`, in place of two copies of the same layout |
| One startup state in `IndexSync` | A fourth startup flag, or a bug where `marked_indexed` and `settling` disagree | `enum Startup { Loading, Settling, Done }` absorbs `marked_indexed` and `settling` |
| Compact ordinals | Ordinal churn measured on a real vault: the span passes 2× the live notes | A merge renumbers the live notes densely, so `lookup`'s dense vectors stay sized by the live notes |
| BM25-style weighting | Occurrence counts rank long notes too high | Normalize by note length inside `Text` |
| MRU-only empty query | The `find` bench's empty-query case passes 1 ms at 50,000 notes (0.17–0.22 ms over the cores in slice 8) | An empty query ranks the MRU's notes, found by id, instead of every note of each range |
| One fold per highlighted word | A name or excerpt word long enough that highlighting shows in a profile | `highlight::prefix` folds the word once as it walks its graphemes, instead of folding the run again at each grapheme end (quadratic in the word) |
| `IndexWatch::later` | A third observer needs the throttle, or the spawn after `Refresh::Later` is duplicated a third time | `IndexWatch` spawns the throttled refresh itself; the backlinks panel and quick open stop spawning it |
| A `Seen` struct | A third field joins `IndexWatch`'s `(revision, building)` pair | A named `Seen` struct replaces the tuple |
| One set of test hooks | A third delegate needs the same gates | Quick open's test-only `hold`, `hold_excerpts`, `runs` and `logged` move into one hook struct the delegates share |
