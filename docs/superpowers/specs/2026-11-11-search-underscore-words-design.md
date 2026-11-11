# Search finds the parts of a word that an underscore wraps or joins

Date: 2026-11-11. Status: **proposed**.

Plan 033. It amends S1 of
[`2026-09-25-smart-quick-open-design.md`](2026-09-25-smart-quick-open-design.md)
(the quick-open spec) and the slice 4 note of
[`2026-09-24-search-design.md`](2026-09-24-search-design.md) (the search spec),
as listed under "Amendments". It changes what
[`2026-09-28-search-word-start-design.md`](2026-09-28-search-word-start-design.md)
and [`2026-09-28-ranking-idf-design.md`](2026-09-28-ranking-idf-design.md)
read as a word, and keeps their rules. The spike lives on the branch
`spike/underscore-word-starts`; it is never merged.

## Problem

Markdown writes emphasis as `_team_` and strong text as `__planned__`. Notes
also hold `_private`, `__init__` and `meeting_notes`. The live preview hides
the `_` marks, so the user sees *team*, types `team`, and gets nothing.

The cause is the word rule. A word is a maximal run of `\w` chars, and `_` is
a `\w` char. So `_team_` is one word that starts with `_`. A term matches only
at a word's start, and no folded `_team_` starts with `team`.

The defect holds for vault search (`⌘⇧F`), for quick open by text (`⌘P`), and
for quick open by name. Ten tests on the spike prove it (commit `49f07edf`,
the tests on `main`'s code). All ten fail there:

| Test | Asserts | On `main`'s code |
|---|---|---|
| `search::underscore_tests::search_finds_team_in_emphasis` | `team` lights `team` in `the _team_ met` | fails |
| `search::underscore_tests::search_finds_planned_in_strong` | `planned` lights `planned` in `it is __planned__` | fails |
| `search::underscore_tests::search_finds_private_after_a_leading_underscore` | `private` lights `private` in `a _private field` | fails |
| `search::underscore_tests::search_finds_init_in_a_dunder` | `init` lights `init` in `def __init__(self)` | fails |
| `search::underscore_tests::search_finds_notes_in_meeting_notes` | `notes` lights `notes` in `see meeting_notes` | fails |
| `find::underscore_tests::quick_open_finds_team_in_emphasis` | `team` finds the note, and its excerpt lights `team` | fails |
| `find::underscore_tests::quick_open_finds_planned_in_strong` | `planned` finds the note | fails |
| `find::underscore_tests::quick_open_finds_private_after_a_leading_underscore` | `private` finds the note | fails |
| `find::underscore_tests::quick_open_finds_init_in_a_dunder` | `init` finds the note, and its excerpt lights `init` | fails |
| `find::underscore_tests::quick_open_finds_notes_in_a_name` | `notes` finds `meeting_notes.md` and lights `notes` in its name | fails |

Six more tests state what must stay true. All six pass on `main`'s code:

| Test | Asserts |
|---|---|
| `search::underscore_tests::stays_an_underscored_query_finds_its_word` | `snake_case` lights `snake_case` |
| `search::underscore_tests::stays_an_underscored_query_skips_its_parts_apart` | `snake_case` finds nothing in `snake and case` |
| `search::underscore_tests::stays_a_term_never_matches_inside_a_part` | `jet` finds nothing in `un projet` or `un_projet` |
| `find::underscore_tests::stays_an_underscored_query_finds_its_word` | `snake_case` finds the note that holds `snake_case` |
| `find::underscore_tests::stays_an_underscored_query_skips_its_parts_apart` | `snake_case` finds no note that holds `snake and case` |
| `find::underscore_tests::stays_a_term_never_matches_inside_a_part` | `jet` finds no note that holds `un projet` |

## What exists

`crates/index/src/words/fold.rs` holds the rule. `word_spans` yields each
maximal run of chars for which `is_word` holds. `is_word` is
`regex_syntax::is_word_character`, with an ASCII fast path
(`is_ascii_alphanumeric() || c == '_'`). Unicode `\w` holds for `_` and for
every other connector punctuation char, such as `‿` (U+203F).

Six readers call `word_spans` outside tests:

| Reader | Role |
|---|---|
| `words/query.rs`, `NoteQuery::new` | The typed text becomes terms. |
| `words/note_words.rs`, `NoteWords::parse` | A note's words and counts: the word index and the cache file. |
| `words/name_words.rs` | The words of a note's path, for quick open by name. |
| `find/terms.rs`, `each_word` | The scan of a note's text for hits, in 64 KiB blocks. |
| `find/preview.rs`, `names` | Whether a line's words are the note's stem: S3's exact name. |
| `find/label.rs`, `word_starts` | The lit ranges of a name or a line. |

Two recorded decisions fix the rule:

- Quick-open S1 says a word is a maximal run of `is_word_character` chars,
  "the rule D4 already uses for whole-word search", and that "`snake_case` is
  one word".
- The search spec's slice 4 note says a word char is regex's own, not
  "alphanumeric or `_`".

Both reasons served the whole-word toggle of D4. The search-word-start spec
removed that toggle on 2026-09-28. So no user reads the rule for the reason
that S1 gives. The rule still does useful work: it keeps `snake_case` one
word, so the query `snake_case` stays exact. This design keeps that property.

The cache file depends on the rule. `persist/fingerprint.rs` derives
`derivation_fingerprint` from the words and counts of three probe notes. A
cache file with another fingerprint is rebuilt.

## Candidates

| | Rule | `team` finds `_team_` | `notes` finds `meeting_notes` | Query `snake_case` |
|---|---|---|---|---|
| A | A maximal run, with the `_` runs at its two edges trimmed | yes | no | one term |
| B | No connector is a word char | yes | yes | two terms, anywhere in the note |
| C | A maximal run, as today; a term may match at each **part start** | yes | yes | one term |
| D | Strip markdown emphasis before the word split | emphasis only | no | one term |

**A** is ruled out by argument. It misses `meeting_notes`. It also breaks the
block join in `each_word`: `foo_` at a block's end, trimmed to `foo`, no
longer touches `bar` at the next block's start. No spike.

**D** is ruled out by argument. It misses `_private`, `__init__` and
`meeting_notes`, which no emphasis parse sees. It also pays a markdown parse
per scanned note. No spike.

**B** finds every "finds" case. It breaks the exact query: `snake_case`
becomes the terms `snake` and `case`, matched anywhere in a note. On its spike
(`af1353f8`), the three `stays_an_underscored_query_*` tests that check it
fail. The search one lights `snake` and `case` apart. B needs no index change
and keeps one entry per word. Its cost is under "Cost". Precision is the
reason it loses: a note app user who types `snake_case` means that word.

**C** finds every "finds" case and keeps every "stays" case. It is the chosen
rule. The decisions below define it.

## Decisions

### U1. The rule

A **word** is a maximal run of `\w` chars, as today. `word_spans` and the
block join of `each_word` do not change.

A **connector** is a char of General Category Pc (U2 lists them).

A **part start** of a word is the byte offset of a char that is no connector,
and that is the word's first char or follows a connector. A **part** is the
word's text from a part start to the word's end. A word made of connectors
only has no part.

A term **matches** a word when the fold of one of its parts starts with the
term. Fold and cap stay those of S1.

One function computes the part starts: walk the word's chars, with a flag
"after a connector" that starts true. Yield a char's offset when it is no
connector and the flag is true; set the flag to whether the char is a
connector. Examples:

| Word | Parts |
|---|---|
| `team` | `team` |
| `_team_` | `team_` |
| `__init__` | `init__` |
| `meeting_notes` | `meeting_notes`, `notes` |
| `a__b_` | `a__b_`, `b_` |
| `__` | none |

A part runs to the word's end, so a term that spans a connector still
matches: `notes_old` finds `meeting_notes_old` through its part
`notes_old`. A term never matches inside a part: `jet` finds neither
`projet` nor `un_projet`.

### U2. Which chars are connectors

All ten chars of `\p{Pc}`: `_` (U+005F), `‿` (U+203F), `⁀` (U+2040), `⁔`
(U+2054), `︳` (U+FE33), `︴` (U+FE34), `﹍` (U+FE4D), `﹎` (U+FE4E), `﹏`
(U+FE4F) and `＿` (U+FF3F).

Reason: `\w` admits exactly these as word chars for one reason, that they
join words. A rule for `_` only would leave `＿`, the low line a CJK keyboard
types, with the defect. The set comes from `regex_syntax`, the crate that
already defines `\w` here, so it follows the same Unicode version.

The nine past ASCII each start with byte 0xE2 or 0xEF in UTF-8, and each is
at U+203F or above. U5 depends on both facts. A test pins the count, ten,
and the lead bytes: `every_connector_past_ascii_has_a_three_byte_lead_of_e2_or_ef`.

### U3. The query

A typed word becomes one term that starts at its first part start. Inner
and trailing connectors stay in the term.

| Typed | Term | Finds |
|---|---|---|
| `team` | `team` | `team`, `_team_`, `my_team` |
| `_team` | `team` | the same as `team` |
| `team_` | `team_` | `team_`, `_team_`, `team_lead`; not `team` |
| `snake_case` | `snake_case` | `snake_case`, `my_snake_case`; not `snake and case` |
| `__` | no term | the word is dropped, as a word that folds to nothing is today |

Reason: a leading connector is markup or a sigil; the user who types `_team`
looks for *team*. A trailing or inner connector narrows the query, as any
typed char does under prefix matching.

The typed words that S3's exact-name test reads (`NoteQuery::typed`) follow
the same cut.

### U4. The word index

`NoteWords::parse` stores one entry per part: the part, folded, capped at
`WORD_MAX_CHARS`. The entry's count is the number of times the part occurs
in the note. Equal entries from different words merge: in `notes` and
`meeting_notes`, the entry `notes` has count 2.

An entry is **whole** for a term when the entry equals the term, or when
only connectors follow the term in the entry and the entry is shorter than
`WORD_MAX_CHARS`. So `team` is whole in `team_`. A capped entry holds more
than it shows, so it is whole only when it equals the term. `is_whole` in
`words/fold.rs` states this once; the lookup and the rank's `equal` call it.

For BM25 (ranking-idf, R1), `tf(t, d)` stays `TermHit::count`: the sum of the
counts of the entries that start with `t`. That is the number of parts in
`d` that `t` starts. `len(d)` stays the sum of `d`'s counts: the number of
parts in `d`. The ranking spec asks for "the same unit as `tf`"; the part is
that unit.

Every word without a connector has one part, its whole self, so its entry
does not change. On the bench vault without connectors, the index and the
cache file keep their size to the byte ("Cost").

### U5. The scan

`word_spans` walks the text's bytes, not its chars. An ASCII byte is
classified in place: a letter or digit continues the word, `_` continues it
and sets a flag **joined**, any other ASCII byte ends it. A byte of 0x80 or
more decodes one char; `is_word_character` decides whether the word goes on,
and `is_connector` whether it is joined. So `joined` is exact, and an ASCII
letter pays no more than today.

The word rule does not change, only the loop. The spike tried three loops
before this one ("Cost"): a connector test per char in `char_indices`, the
same test inlined, and a byte pass over each word after its span. Each cost
6% to 15% on `search` or `read_search` at 50k notes. The byte walk costs
under 6% on `search` and nothing on `read_search`; part of that is the walk
itself, which is faster than `char_indices` on `main` too.

`each_word` passes the flag with the word's range, and ORs the flags of the
two halves when it joins a word across a block's end.

A word without a connector has one part. The scan calls `Terms::matching` on
it once, exactly as today, with the `starts[128]` first-byte skip. A joined
word calls `matching` once per part; each call keeps the first-byte skip.

`NoteWords::parse` keeps the flag with each distinct spelling, and folds a
spelling without a connector once, as today. The readers that hold a word
without the flag (`name_words`, `label`, `preview`) find the part starts with
one byte scan for `_`, 0xE2 or 0xEF. A word with none of them has one part.

### U6. What a hit lights

A hit lights, in the note's own bytes, the range from the part start to the
end of the term's prefix in that part: `word_prefix(text, word.start +
part..word.end, term)`. In `_team_` at offset 4, `team` lights bytes 5..9,
the four chars `team`. It never lights a connector before the part. A
connector inside the term is lit, because the user typed it: `snake_case`
lights `snake_case`.

Within one part, the longest matching term lights, as today. Two parts of
one word light two ranges: `team` in `team_team` lights both, and the query
`meeting notes` lights `meeting` and `notes` in `meeting_notes`, not the `_`
between them.

The same rule holds for the lit name in a quick-open row (`label.rs`) and
for the excerpt's highlights (`excerpt.rs`). A part that starts past the
excerpt's window lights nothing.

The range is in the note's bytes, not in any rendered text. A row renderer
that strips markup maps it with its own offset map.

### U7. Names

A path's words follow the same rule. `name_words` stores one name word per
part, with its byte range from the part start, and a flag `first` for the
word's first part. `notes` finds `meeting_notes.md` and lights `notes`.

S3's exact name compares the typed words with the stem's first parts only,
so `meeting_notes` is no exact name for `notes`. The preview's stem test
(`preview.rs`, `names`) cuts the same leading connectors.

The rank's `equal` (key 1) uses `is_whole`, so `todo` equals the name word
`todo_` of `_todo_.md`. The exact-name test compares the same way, through
`is_whole` per word pair, so `todo` is the exact name of `_todo_.md`. The
spike compares with `==` here; the build changes it under slice 3.

### U8. The cache

The derivation fingerprint makes a stale cache impossible. U4 changes the
entries of `task_one` and `snake_case`, which the probes already hold, so the
fingerprint changes with the rule. The build also adds probe words that
only this rule's edge cases reach: ` _em_ __strong__ __ a‿b` at the end of
the third probe note. A later change to how a leading connector, a
connectors-only word or a non-ASCII connector splits then changes the
fingerprint too.

`FORMAT_VERSION` does not change: the file's layout is the same. The
`golden_bytes` test pins the new fingerprint. Nobody uses Inkra yet, so a
rebuilt cache needs no migration.

### U9. The deviation from Zed

Zed counts `_` as a word char
(`vendor/zed-reference/crates/language/src/buffer.rs:6265-6268`,
`c.is_alphanumeric() || c == '_'`), and so does Inkra's word. Zed is an IDE:
its users search identifiers, and it has no part starts. Inkra is a note
app: markdown wraps words in `_`, and the preview hides the marks. So a term
also matches at a part start. The word, and an exact identifier query, stay
as Zed has them.

### U10. The seam

The rule touches three places, and any later tokenizer change (CJK
segmentation, camelCase starts) meets the same three:

- the part starts, one function in `words/fold.rs` that every reader calls;
- the scan's fast path, the joined flag of U5;
- the cache's derivation fingerprint and its probes.

## Cost

All figures are medians of `--quick` criterion runs on one Apple Silicon
laptop, in µs unless stated. "Base" is `main`'s word code with the spike's
bench corpus (`c689f799`). "C" is the chosen rule at the spike's head
(`aa4068b1`). Each side ran twice, interleaved; "noise" is the larger gap
between base's two runs. "Evidence" gives the commands.

The default bench vault holds no connector. On it, C changes nothing that a
user sees, so every delta is the price of the rule alone.

| Bench, 50k notes | Base runs | Noise | C runs | Delta |
|---|---|---|---|---|
| `search`, rare `zanzibar` | 832 / 862 | 3.4% | 855 / 904 | +3.9% |
| `search`, common `projet` | 1076 / 1158 | 7.4% | 1157 / 1190 | +5.1% |
| `search`, `e`, hits the cap | 938 / 1014 | 7.8% | 1002 / 1057 | +5.5% |
| `search`, `é` | 934 / 1016 | 8.4% | 1004 / 1056 | +5.7% |
| `search`, `projetzz`, no match | 250 / 270 | 7.7% | 255 / 269 | +0.8% |
| `read_search`, resident | 7568 / 8187 | 7.9% | 7470 / 7980 | -1.9% |
| `read_search`, read4 | 14590 / 15501 | 6.1% | 15001 / 15313 | +0.7% |
| `find_notes`, zipf, one core, `e` | 4725 / 5067 | 7.0% | 5025 / 5261 | +5.0% |
| `find_notes`, zipf, one core, `zanzibar` | 1076 / 1160 | 7.5% | 1192 / 1222 | +7.9% |
| `find_notes`, zipf, one core, empty | 88 / 110 | 22.9% | 76 / 78 | -22.4% |
| keystroke to excerpts, zipf, `e` | 2179 / 2206 | 1.2% | 2349 / 2420 | +8.8% |
| `parse`, 3 KB zipf note | 36.0 / 38.5 | 6.8% | 34.8 / 37.9 | -2.6% |
| `parse`, 3 KB greek note | 9.1 / 10.0 | 8.5% | 10.5 / 10.7 | +11.1% |
| `lookup`, settled, `e` | 1344 / 1407 | 4.6% | 1539 / 1583 | +13.5% |
| `lookup`, settled, top word | 73.0 / 77.7 | 6.3% | 79.4 / 80.1 | +5.9% |
| `build`, zipf, foreground (ms) | 16.0 / 17.0 | 5.8% | 15.9 / 17.0 | +0.0% |

Every `search` and `read_search` row stays under 6%. All but `zanzibar`
(+3.9% against 3.4%) lie inside base's own noise.
So the STOP line of plan 033 (more than 10% on `search` or `read_search` at
50k notes) is not reached.

Four rows pass base noise. Quick open on `zanzibar` grows by 0.09 ms, and a
keystroke to excerpts on `e` by 0.17 ms. `parse` on Greek text grows by about
1 µs per 3 KB note. The settled `lookup` of a one-letter term grows by about
0.2 ms, because it calls `is_whole` on each entry that the term starts. We
accept all four. Each stays under 0.2 ms per query, and `build`, which runs
`parse` on every note, does not move.

Index sizes come from the `words` bench's report and the `persist` bench's
cache file. On the default vault each size is the same on both sides, to the
byte: heap 22,836,976, postings 22,008,767, cache file 8,709,437.

The **underscore vault** (`INKRA_BENCH_UNDERSCORE=1`, `c689f799`) is a stress
case: of each 20 words, one is `_w_`, one `__w__` and one `w_v`. There C
finds hits that base cannot, so a slower query does more work, not the same work slower.

| Underscore vault, 50k notes | Base runs | Noise | C runs | Delta |
|---|---|---|---|---|
| `search`, rare `zanzibar` | 796 / 818 | 2.7% | 951 / 1103 | +27.3% |
| `search`, common `projet` | 1311 / 1341 | 2.3% | 1492 / 1474 | +11.8% |
| `search`, `e`, hits the cap | 1226 / 1261 | 2.9% | 1519 / 1509 | +21.8% |
| `search`, `projetzz`, no match | 259 / 269 | 3.5% | 269 / 269 | +2.0% |
| `find_notes`, one core, `e` | 5123 / 5226 | 2.0% | 5927 / 5885 | +14.1% |
| `parse`, 3 KB greek note | 21.2 / 22.2 | 4.5% | 26.3 / 26.2 | +20.8% |
| `lookup`, settled, top word | 251 / 248 | 1.3% | 377 / 382 | +51.9% |
| heap bytes | 34,430,861 | | 35,199,006 | +2.2% |
| posting bytes | 24,841,180 | | 25,649,794 | +3.3% |
| cache file bytes | 14,752,603 | | 14,754,653 | +0.01% |

A real vault sits between the two: most words hold no connector, and the
cost follows the share that does.

**The scan loop.** The word rule stayed fixed while the spike tried five
loops for the joined flag. Each row but the last is one quick pair against
base, on the default vault; the last row is the double run above:

| Loop | Commit | `search` `zanzibar` | `read_search` resident |
|---|---|---|---|
| connector test per char in `char_indices` | `da1043b4` | +14.9% | not compared |
| one class test per char | `8d6148c7` | +11.9% | not compared |
| the same, `#[inline]` | `51be3ce1` | +14.6% | not compared |
| a byte pass over each word after its span | `e8aba78d` | +5.6% | +14.2% |
| the byte walk of U5 | `aa4068b1` | +3.9% | -1.9% |

The byte walk alone, without parts (`baseS`, built from `c689f799` plus only
the walk), already moves `main`: `read_search` resident -13.0%, `search`
within ±2%, Greek `parse` +22.5%. The last commit, `aa4068b1`, walks a
non-ASCII run in one `char_indices` pass and halves that Greek cost.

**B.** One run of `af1353f8` against two of base, before the method below
existed: `read_search` resident +21.1%, `search` within noise. B lost on
precision ("Candidates"), so the spike did not measure it again.

## Amendments

### Quick-open spec, S1 (`2026-09-25-smart-quick-open-design.md:96-98`)

Replace:

> A **word** is a maximal run of chars for which `regex_syntax::is_word_character`
> holds, the rule D4 already uses for whole-word search. `l'été` has the words
> `l` and `été`; `2026-09` has `2026` and `09`; `snake_case` is one word.

with:

> A **word** is a maximal run of chars for which `regex_syntax::is_word_character`
> holds. `l'été` has the words `l` and `été`; `2026-09` has `2026` and `09`;
> `snake_case` is one word. A word's **parts** start at its first char that is
> no connector (`\p{Pc}`, such as `_`) and at each such char after a
> connector; each part runs to the word's end. A term matches a word when it
> starts the fold of one of its parts
> ([`2026-11-11-search-underscore-words-design.md`](2026-11-11-search-underscore-words-design.md), U1).

The quick-open spec's lines 539-540 ("`_` inside a word is not emphasis
(CommonMark), so `snake_case_name` stays whole") stay. They state what the
plain text of a row keeps, not what a word is.

### Search spec, slice 4 note (`2026-09-24-search-design.md:480`)

Replace:

> - A word char is regex's own, `regex_syntax::is_word_character`, not
>   "alphanumeric or `_`".

with:

> - A word char is regex's own, `regex_syntax::is_word_character`, not
>   "alphanumeric or `_`". A term matches at each part start of a word, not
>   only at its start: see U1 of
>   [`2026-11-11-search-underscore-words-design.md`](2026-11-11-search-underscore-words-design.md).

The rest of that bullet stays.

## Slices

One build, three slices. Each slice lands with its tests, and slice 1 lands
with its fingerprint change in the same commit.

**Slice 1. Text: the word index and vault search.**
Failing test first: `search_finds_team_in_emphasis`.
Files: `words/fold.rs` (part starts, connectors, `is_whole`, the joined
flag), `words/note_words.rs`, `words/lookup.rs`, `words/query.rs`,
`find/terms.rs` (`Spanned`, `Terms::parts`), `search/run.rs`,
`persist/fingerprint.rs`, `persist/header_tests.rs`, the words and search
models. Done: the five `search_finds_*` tests and the three search `stays_*`
tests pass, `terms_tests` passes, `cargo bench -p inkra-index --bench search
-- --quick` stays within the "Cost" figures.

**Slice 2. Quick open by text.**
Failing test first: `quick_open_finds_team_in_emphasis`.
Files: `find/excerpt.rs`, `find/rank.rs` (`is_whole` for `equal`, and
`#[inline]` on `rank`), `find/model_cases.rs`, `find/model_tests.rs`. Done:
the four `quick_open_finds_*` text tests and the find `stays_*` tests pass,
and `cargo bench -p inkra-index --bench find -- --quick` shows no `empty, 20
recent` row past base noise.

**Slice 3. Names.**
Failing test first: `quick_open_finds_notes_in_a_name`.
Files: `words/name_words.rs`, `find/label.rs`, `find/preview.rs`,
`find/rank.rs` (`is_exact` through `is_whole`, U7). Done: the name test
passes, and a new test `todo_is_the_exact_name_of_underscored_todo` passes.

Each slice ends with `./scripts/check.sh` green and the mutants run of
"Tests".

## Tests

The sixteen tests of "Problem" move from the spike into the build, under
`search/underscore_tests.rs` and `find/underscore_tests.rs`.

Unit tests in `words/fold/tests.rs`:

- `a_part_starts_the_word_or_follows_a_connector`: the table of U1.
- `a_term_is_whole_when_only_connectors_follow_it`: `team` in `team_`, not in
  `team_lead`, and a capped entry only when equal.
- `every_connector_past_ascii_has_a_three_byte_lead_of_e2_or_ef`: ten
  connectors, U2's lead bytes.
- The joined flag: true whenever the word holds a connector, over the
  `PARTS` strategy; and the parts of a joined word without a connector are
  the word alone.
- `a_non_ascii_connector_starts_a_part`: `b` finds `a‿b` (lead 0xE2) and
  `a＿b` (lead 0xEF), and `team` or `été` is not joined. The spike's part
  test covers `a‿b` only; without the 0xEF case, a mutant that drops that
  arm of the byte test survives.

Proptest properties:

- `terms_tests`: `each_word` agrees with `word_spans` across block ends, now
  with the flag; a block edge inside `a_b` keeps the OR of both halves.
- `words::model_tests::lookup_agrees_with_a_model`: the model splits words
  with `str::split`, then cuts parts at each `_` run; `model_hit` counts every
  part a term starts and calls it whole when only `_` follows. The strategy's
  `PARTS` must hold `_` at a word's edge and inside it.
- `find::model_tests` and `search::model_tests`: the models' `words` yield
  parts, their `heads` yield first parts for the exact name.

Mutants: after each slice,
`git diff main... > $TMPDIR/m.diff && cargo mutants -j 4 --in-diff $TMPDIR/m.diff`.
Zero missed is the bar. Mutants the spike's tests must kill: the flag's
initial `true` in the part walk, the `< WORD_MAX_CHARS` bound in `is_whole`,
`first: part == 0`, the `c < '\u{203F}'` early return, each arm of the
joined byte test, and the OR of the flags at a block join.

## Evidence

**Method.** With the default 16 codegen units, one release build of the same
source moved the empty quick-open query by 115% (`fdd17ef2`, one core), and
`sample` showed `rank` no longer inlined into `find_in`. Code placement, not
the rule, decided such rows. So every figure in "Cost" comes from builds with
one codegen unit and aligned functions and blocks:

```sh
export CARGO_TARGET_DIR=…/target/wt-advisor-033
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=6"
export CARGO_PROFILE_BENCH_CODEGEN_UNITS=1
cargo bench -p inkra-index --no-run          # at c689f799 (base) and at aa4068b1 (C)
cd crates/index
for b in words search read_search find persist; do $BIN/$b --bench --quick; done
INKRA_BENCH_UNDERSCORE=1 $BIN/$b --bench --quick   # the underscore vault
```

The runs went base, C, base on the underscore vault, C on it, twice.

**Tests.** On the spike head:

```sh
cargo test -q -p inkra-index
test result: ok. 582 passed; 0 failed; 0 ignored
```

The ten "finds" tests fail at `49f07edf`, which adds them to `main`'s code.
The three `stays_an_underscored_query_*` tests that check B fail at `af1353f8`.

**Commits on `spike/underscore-word-starts`.** `c689f799` the underscore
corpus; `49f07edf` the tests; `fdd17ef2` C; `af1353f8` B; `16a3573b` back to
C; `22d22345` to `da1043b4` the plain-word fast path and `rank` inlined;
`8d6148c7` to `aa4068b1` the scan loops of "Cost". `c56dc7f6` does not build
its tests; `6a2dcbc9` fixes them.

## Open questions

1. **Should camelCase or digit boundaries also start a part?** `team` would
   then find `myTeam`. Recommendation: no. Notes rarely hold camelCase, and a
   case boundary makes `jet` find `unProjet`-like words that the user did not
   mean. If a user asks for it, it meets the seam of U10 and needs its own
   spike.
2. **Should the typed `_team` keep its `_` and find only `_team…` words?**
   Recommendation: no, keep U3. The user who types `_team` copies markup or a
   sigil. The exact identifier stays reachable: `_team_x` keeps its inner
   connector and narrows the query.
3. **Should a `⌘⇧F` row strip the emphasis marks around a lit part?** The
   hit's line is the note's raw line, so `_team_` shows with its marks, and
   `team` is lit inside them. Recommendation: keep the raw line in this build.
   The lit range is right in the note's bytes (U6); a later row renderer that
   strips markup maps it with its own offset map, as the quick-open excerpt
   already does through `Plain`.
4. **Should `_todo_.md` be the exact name for `todo` (U7)?** Recommendation:
   yes. The note's only word is *todo*, wrapped in marks. A strict `==` would
   rank it below `todo list.md`, for no reason the user can see.
5. **Should `rank` keep `#[inline]`?** On the spike, `is_whole` grew `rank`
   past the size that `find_in` inlines, and the empty query doubled
   ("Evidence", `fdd17ef2`).
   Recommendation: yes, keep `#[inline]` with its one-line reason, and let the
   find bench guard it. Do not reshape `is_whole` for codegen.
6. **Should the build keep the byte walk if the rule were dropped?** It cuts
   `read_search` by 13% on `main` alone ("Cost", `baseS`). Recommendation: yes,
   land it in slice 1 with the rule, and keep `parse` on Greek text in the
   words bench as its guard.
