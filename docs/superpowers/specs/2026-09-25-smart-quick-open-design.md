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
- **A snapshot** is four `Arc`s: ordinals, segment, alive, fresh. A write
  copies only pointer-sized state while a snapshot holds it.
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

```rust
pub struct NoteQuery;       // NoteQuery::new(text, showing: Option<EntryId>, recent: &[EntryId])
pub struct Lookup;          // the query's terms resolved in the word index; cheap to share
impl WordsSnapshot { pub fn lookup(&self, query: &NoteQuery, cancel: &AtomicBool) -> Option<Lookup>; } // None: cancelled (slice 7)
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
