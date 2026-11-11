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

Revision 1, the same day, answers the first review. A part and a term now
drop their trailing connectors, so "whole" is a plain `==` (U1, U3, U4, U7).
U8 bounds a word of many parts. "Cost" is measured again at the spike's new
head, under both build profiles.

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
word's text from a part start to the word's end, without the word's
**trailing connectors**. A word made of connectors only has no part. A word
has at most 64 parts (U8).

A term **matches** a word when the fold of the word's text from a part start
starts with the term. Fold and cap stay those of S1.

One function computes the part starts: walk the word's chars, with a flag
"after a connector" that starts true. Yield a char's offset when it is no
connector and the flag is true; set the flag to whether the char is a
connector. Examples:

| Word | Parts |
|---|---|
| `team` | `team` |
| `_team_` | `team` |
| `__init__` | `init` |
| `meeting_notes` | `meeting_notes`, `notes` |
| `a__b_` | `a__b`, `b` |
| `__` | none |

A part runs to the word's end, so a term that spans a connector still
matches: `notes_old` finds `meeting_notes_old` through its part
`notes_old`. A term never matches inside a part: `jet` finds neither
`projet` nor `un_projet`.

Reason for the trim: a trailing connector is markup, as a leading one is.
The trim makes "the term is the whole part" a plain `==` of two strings, in
the data. No reader needs a second test for it (U4, U7).

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

A typed word becomes one term: its first part, so without its leading and
trailing connectors. Inner connectors stay in the term.

| Typed | Term | Finds |
|---|---|---|
| `team` | `team` | `team`, `_team_`, `my_team` |
| `_team` | `team` | the same as `team` |
| `team_` | `team` | the same as `team` |
| `snake_case` | `snake_case` | `snake_case`, `my_snake_case`; not `snake and case` |
| `__` | no term | the word is dropped, as a word that folds to nothing is today |

Reason: a leading or trailing connector is markup or a sigil; the user who
types `_team` or `team_` looks for *team*. An inner connector narrows the
query, as any typed char does under prefix matching.

What the user loses: a trailing connector no longer narrows. On `main`,
`team_` finds `team_lead` and not `teams`. Under this rule `team_` finds
`teams` and `teammate` too, as `team` does. The narrow query stays one char
away: `team_l` keeps its inner `_` and finds `team_lead` only.

The typed words that S3's exact-name test reads (`NoteQuery::typed`) follow
the same cut.

### U4. The word index

`NoteWords::parse` stores one entry per part: the part, folded, capped at
`WORD_MAX_CHARS`. The entry's count is the number of times the part occurs
in the note. Equal entries from different words merge: in `notes` and
`meeting_notes`, the entry `notes` has count 2. In `team`, `_team_` and
`__team__`, the entry `team` has count 3.

An entry is **whole** for a term when the entry equals the term, as on
`main`. A part holds no trailing connector and a term holds none (U3), so
`==` is the whole test. `words/lookup.rs` and the rank's `equal` do not
change.

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
more decodes one char; `is_word_character` decides whether the word goes on.
A char in U+2000..U+2FFF or U+F000..U+FFFF, the code points of lead byte
0xE2 or 0xEF, then asks `is_connector` whether it joins the word. The test is
one shift of the decoded char. No other char is a connector (U2), so a CJK,
Greek or Cyrillic char never reaches the connector table. So `joined` is
exact.

The word rule does not change, only the loop. The spike tried four loops
before this one ("Cost"). Each cost 4% to 15% on `search` or `read_search` at
50k notes. A build without the flag at all costs the same as this loop
("Cost", R2 bisect), so the flag itself is free.

`each_word` passes the flag with the word's range, and ORs the flags of the
two halves when it joins a word across a block's end.

A word without a connector has one part. The scan calls `Terms::matching` on
it once, exactly as today, with the `starts[128]` first-byte skip. A joined
word calls `matching` once per part start, on the text from that start to
the word's end; each call keeps the first-byte skip. The part's trailing
connectors stay in that text: a term holds none, so they change no match.

`matching` folds a word through `fold_head(word, TERM_MAX_CHARS)`. A word of
at most `4 × TERM_MAX_CHARS` bytes folds whole, as on `main`. A longer word
folds only a head that holds its first `TERM_MAX_CHARS` folded chars, since
no term is longer (U8).

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

A name word's part drops its trailing connectors, as an entry does (U4). So
`_todo_.md` holds the name word `todo`, and `todo` is its exact name through
the data alone. The rank's `equal` (key 1) and the exact-name test keep
`==`; `is_exact` only adds the `first` filter.

### U8. A word of many parts

A word of p parts has p part starts, and the text from each runs to the
word's end. Without a bound, `parse` folds p suffixes and the scan matches p
suffixes, so one word costs O(p × len). One note that is one 64 KiB word
`a_a_…` (32,768 parts) took 266 to 285 ms in `parse` and 276 to 284 ms per
`search` keystroke for `a` on the spike before the bound (`a62a7c74`). Base
takes 0.28 ms and 0.05 ms.

Two bounds, each by construction:

- **64 parts.** `WORD_MAX_PARTS = 64`: the part walk yields the first 64 part
  starts only. A word adds at most 64 entries to the index, and the scan
  calls `matching` at most 64 times per word.
- **A bounded fold head.** `fold_head(word, chars)` folds a word of at most
  `4 × chars` bytes whole. A longer word folds a head: `chars` chars, then
  up to the next char that is no combining mark, since no later mark
  reorders across that char. If dropped marks leave the fold short of
  `chars` chars, the head doubles. `parse` passes `WORD_MAX_CHARS` and then
  caps; `matching` passes `TERM_MAX_CHARS`.

For p parts and a word of `len` bytes, the work is then:

| Step | Work | Distinct entries |
|---|---|---|
| `parse` | the walk, O(len); then min(p, 64) folds, each of at most 260 bytes or of a 65-char head and its marks | at most min(p, 64) |
| one scan keystroke | the walk, O(len); then min(p, 64) `matching` calls, each folding at most 256 bytes or a 64-char head and its marks | none |

A head doubles only past a run of dropped marks. Its worst case reads the
word's rest once per part, for a word that is almost all marks.

What the user loses: a term never matches a part past the word's 64th. A
word with 65 `_`-joined parts is most likely a generated string, not prose. The test
`a_part_past_the_word_s_sixty_fourth_is_no_part` pins the loss, in search
and in quick open.

### U9. The cache

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

### U10. The deviation from Zed

Zed counts `_` as a word char
(`vendor/zed-reference/crates/language/src/buffer.rs:6265-6268`,
`c.is_alphanumeric() || c == '_'`), and so does Inkra's word. Zed is an IDE:
its users search identifiers, and it has no part starts. Inkra is a note
app: markdown wraps words in `_`, and the preview hides the marks. So a term
also matches at a part start. The word, and an exact identifier query, stay
as Zed has them.

### U11. The seam

The rule touches three places, and any later tokenizer change (CJK
segmentation, camelCase starts) meets the same three:

- the part starts and the trim, two functions in `words/fold.rs` that every
  reader calls;
- the scan's fast path, the joined flag of U5;
- the cache's derivation fingerprint and its probes.

## Cost

All figures are medians of `--quick` criterion runs on one Apple Silicon
laptop, in µs unless stated. "Base" is the index code of `c689f799` with the
spike's later benches. "C" is the chosen rule at the spike's head,
`329a6303`. Each side ran twice, interleaved; "noise" is the gap between
base's two runs. "Evidence" gives the method and the commands.

**The default vault.** It holds no connector. On it, C changes no hit and
no entry, so every delta is the price of the rule's code alone.
Placement-stable profile:

| Bench, 50k notes unless stated | Base runs | Noise | C runs | Delta |
|---|---|---|---|---|
| `search`, rare `zanzibar` | 830 / 849 | 2.3% | 907 / 887 | +6.8% |
| `search`, common `projet` | 1140 / 1117 | 2.0% | 1181 / 1152 | +3.4% |
| `search`, `e`, hits the cap | 986 / 988 | 0.2% | 1012 / 1000 | +1.9% |
| `search`, `projetzz`, no match | 269 / 261 | 3.2% | 271 / 268 | +1.6% |
| `search`, folded `ete` | 1612 / 1548 | 4.1% | 1652 / 1615 | +3.4% |
| `search`, `é` | 1033 / 996 | 3.6% | 1005 / 995 | -1.4% |
| `search`, `réunion` | 1928 / 1888 | 2.1% | 1944 / 1907 | +0.9% |
| `search`, `projet réunion` | 1295 / 1279 | 1.3% | 1287 / 1259 | -1.1% |
| `read_search`, resident | 8282 / 7938 | 4.2% | 7998 / 7867 | -2.2% |
| `read_search`, read4 | 15675 / 15598 | 0.5% | 15977 / 15021 | -0.9% |
| `find_notes`, zipf, one core, `e` | 4899 / 4970 | 1.5% | 5086 / 5051 | +2.7% |
| `find_notes`, zipf, one core, `zanzibar` | 1167 / 1143 | 2.0% | 1142 / 1146 | -0.9% |
| `find_notes`, zipf, one core, empty | 92 / 99 | 6.5% | 76 / 76 | -20.2% |
| excerpt, top 100 rows, `zanzibar` | 547 / 532 | 2.7% | 601 / 583 | +9.8% |
| keystroke to excerpts, zipf, `e` | 2220 / 2199 | 1.0% | 2222 / 2178 | -0.4% |
| `persist`, encode | 7484 / 7345 | 1.9% | 7489 / 7357 | +0.1% |
| `persist`, decode | 9475 / 9350 | 1.3% | 9369 / 9218 | -1.3% |
| `parse`, 3 KB zipf note | 36.4 / 37.6 | 3.4% | 38.0 / 36.9 | +1.2% |
| `parse`, 3 KB greek note | 9.4 / 9.8 | 4.4% | 11.1 / 10.7 | +14.3% |
| `parse`, 3 KB sparse accents | 9.3 / 9.8 | 5.0% | 9.3 / 8.8 | -5.0% |
| `build`, greek, foreground (ms) | 14.6 / 12.8 | 13.3% | 12.6 / 12.9 | -7.3% |
| `build`, zipf, foreground (ms) | 17.1 / 16.6 | 3.2% | 17.2 / 17.3 | +2.3% |
| `lookup`, settled, `e` | 1406 / 1389 | 1.2% | 1406 / 1390 | +0.0% |
| `lookup`, settled, top word | 77.4 / 76.3 | 1.4% | 74.8 / 78.7 | -0.2% |
| one 10 MB note, `zanzibar` once at its end (ms) | 34.4 / 33.9 | 1.2% | 37.3 / 36.3 | +7.8% |
| 10 MB of CJK, `项`, 100 hits | 84.3 / 85.1 | 0.9% | 91.3 / 92.9 | +8.8% |
| 10 MB of Cyrillic, `о`, 100 hits | 175.9 / 176.5 | 0.4% | 182.0 / 184.6 | +4.0% |
| 10 MB of Greek, `α`, 100 hits | 286.1 / 280.8 | 1.9% | 288.6 / 293.1 | +2.6% |

No `search` or `read_search` row at 50k notes grows by more than 10%. So
the STOP line of plan 033 is not reached. The first draft's `lookup` cost is
gone: with `is_whole` removed, the settled `lookup` of `e` is flat.

**What is left, as an absolute bound.** Four kinds of row still pass base
noise. Each is the scan or `parse`, never the index:

| Where | Measured bound, placement-stable | The same, default profile |
|---|---|---|
| scan of plain text | +2.7 ms per 10 MB scanned, so about 0.3 ms per MB | +3.7 ms per 10 MB |
| scan of CJK text | +7.4 µs per 100-hit keystroke in 10 MB | +6.5 µs |
| `search`, `zanzibar`, 50k notes | +0.06 ms per keystroke | +0.07 ms |
| excerpts of the top 100 rows, `zanzibar` | +0.05 ms | +0.06 ms |
| `parse`, Greek | +1.3 µs per 3 KB note | +1.1 µs |

`build` on the Greek vault runs `parse` on 50k Greek notes and does not grow
(-7.3%, -0.9%). So the `parse` bound does not add up across a build. The
keystroke-to-excerpts rows, which time what a quick-open keystroke shows,
stay within ±3.2%.

**Where the scan's residual comes from (R2 bisect).** Each build changed one
part of C and ran one quick pair against base, placement-stable. Base's
noise per round was 1% to 10%:

| Build | Change from C | CJK rows | `zanzibar` once at its end | Greek `parse` |
|---|---|---|---|---|
| E1, kept | the code-point gate of U5 | +6% to +12% | +6% to +12% | +6% to +11% |
| E2 | `#[inline]` on `wide_run` | as E1 | as E1 | as E1 |
| E3 | the joined branch in an `#[inline(never)]` function | as E1 | as E1 | as E1 |
| E4 | no joined flag at all | +11% to +12% | +8.6% | not better |
| E6 | `main`'s char walk, no flag | +7% | +8.3% | not compared |
| E7 | plain `fold` in `matching`, no `fold_head` | +4% to +5% | +8.2% | not compared |

No single part causes the residual: removing the flag, the byte walk or
`fold_head` leaves most of it. It is spread over the parts plumbing of the
scan. Open question 6 keeps it open; no build of the spike removed it.

**The default profile (R3).** The same base and C, built with the
workspace's bench profile (16 codegen units), two interleaved runs:

| Bench, 50k notes unless stated | Base runs | Noise | C runs | Delta | Placement-stable delta |
|---|---|---|---|---|---|
| `search`, rare `zanzibar` | 860 / 832 | 3.3% | 924 / 916 | +8.7% | +6.8% |
| `search`, common `projet` | 1145 / 1145 | 0.0% | 1193 / 1172 | +3.3% | +3.4% |
| `search`, `projetzz`, no match | 281 / 279 | 0.8% | 280 / 277 | -0.4% | +1.6% |
| `search`, folded `ete` | 1609 / 1523 | 5.5% | 1662 / 1640 | +5.4% | +3.4% |
| `search`, `réunion` | 1884 / 1856 | 1.5% | 1982 / 1940 | +4.9% | +0.9% |
| `read_search`, resident | 7763 / 7664 | 1.3% | 7746 / 7790 | +0.7% | -2.2% |
| `read_search`, read4 | 15035 / 15061 | 0.2% | 15200 / 15321 | +1.4% | -0.9% |
| `find_notes`, zipf, one core, `zanzibar` | 1244 / 1279 | 2.8% | 1313 / 1329 | +4.8% | -0.9% |
| `find_notes`, zipf, one core, empty | 81 / 71 | 13.9% | 91 / 77 | +10.3% | -20.2% |
| `find_notes`, 40 words, one core, empty | 71 / 91 | 24.4% | 72 / 59 | -19.6% | -12.1% |
| excerpt, top 100 rows, `zanzibar` | 552 / 542 | 1.8% | 610 / 595 | +10.1% | +9.8% |
| `persist`, encode | 7146 / 7070 | 1.1% | 7761 / 7615 | +8.2% | +0.1% |
| `parse`, 3 KB greek note | 9.9 / 9.9 | 0.7% | 10.9 / 11.1 | +11.4% | +14.3% |
| `parse`, 64 KiB `a_a_…` word | 281 / 274 | 2.5% | 251 / 242 | -11.2% | -2.2% |
| one 10 MB note, `zanzibar` once at its end (ms) | 32.3 / 31.9 | 1.3% | 36.0 / 35.7 | +11.6% | +7.8% |
| 10 MB of CJK, `项`, 100 hits | 88.4 / 89.0 | 0.7% | 96.0 / 94.3 | +7.2% | +8.8% |

Code placement moves three rows; the rest agree across profiles within
noise:

- `persist` encode: +8.2% under 16 codegen units, +0.1% under one. The
  encoder's code does not change.
- the empty quick-open query: +10.3% and -19.6% under 16 codegen units, two
  vaults, both inside their noise of 13.9% and 24.4%; -20.2% and -12.1%
  under one.
- the 10 MB scan: +11.6% under 16 codegen units, +7.8% under one, the same
  work.

Before the two hints, placement moved far more. Under 16 codegen units, `projetzz` grew by 51.2% and the empty query by 112%
and 107%; `persist` encode by 13.1%. Two `#[inline]` hints cut these, and
each has a bench row that guards it:

| Hint | Guard row | Before the hint | After |
|---|---|---|---|
| `#[inline]` on `rank`, `find/rank.rs` | `find`, zipf, one core, `empty, 20 recent` | +112% / +107% | -14.8% / -10.3% with the hints, +10.3% / -19.6% at `329a6303` |
| `#[inline]` on `Shards::get`, `words/shards.rs` | `search`, `projetzz`, no match | +51.2% | +1.1% with the hints, -0.4% at `329a6303` |

The profile stays fragile: open question 4 holds the figures.

**A word of many parts (R4).** One note that is one 64 KiB word, before the
bound (`a62a7c74`, one run) and at C (two runs per profile):

| Bench | Base | Before the bound | C, placement-stable | C, default |
|---|---|---|---|---|
| `parse`, `a_a_…` | 252 / 268 | 266,000 to 285,000 | 259 / 249 | 251 / 242 |
| `search` `a`, `a_a_…` | 52.1 / 51.3 | 276,000 to 284,000 | 28.0 / 28.1 | 28.8 / 28.4 |
| `parse`, `aaaa…`, no connector | 67.7 / 66.5 | not run | 34.4 / 34.2 | 34.0 / 34.1 |
| `search` `a`, `aaaa…` | 51.7 / 51.5 | not run | 17.7 / 17.9 | 17.9 / 17.9 |

The bound turns a 0.28 s keystroke back into 28 µs, below base. A word that
holds no connector is faster than base too: `fold_head` folds a bounded head
instead of the 64 KiB word. U8 states the work per part.

**Index sizes.** They come from the `words` bench's report and the
`persist` bench's cache file, at `329a6303`. On the default vault each size
is the same on both sides, to the byte: heap 22,836,976, postings
22,008,767, cache file 8,709,437, 13,663 distinct words.

**The underscore vault** (`INKRA_BENCH_UNDERSCORE=1`) is a stress case: of
each 20 words, one is `_w_`, one `__w__` and one `w_v`. There C finds parts
that base cannot, so a slower row most likely does more work, not the same
work slower. Placement-stable:

| Underscore vault, 50k notes | Base runs | Noise | C runs | Delta |
|---|---|---|---|---|
| `search`, rare `zanzibar` | 814 / 805 | 1.1% | 929 / 947 | +15.9% |
| `search`, common `projet` | 1353 / 1343 | 0.7% | 1329 / 1362 | -0.2% |
| `search`, `e`, hits the cap | 1268 / 1252 | 1.3% | 1225 / 1242 | -2.1% |
| `search`, `projetzz`, no match | 267 / 265 | 0.6% | 265 / 267 | +0.1% |
| `find_notes`, one core, `e` | 5287 / 5212 | 1.4% | 5274 / 5355 | +1.2% |
| `find_notes`, one core, empty | 106 / 103 | 2.7% | 78 / 80 | -24.4% |
| `persist`, encode | 13671 / 14183 | 3.7% | 10661 / 10445 | -24.2% |
| `persist`, decode | 13612 / 13903 | 2.1% | 11192 / 11645 | -17.0% |
| `parse`, 3 KB zipf note | 44.5 / 43.9 | 1.3% | 46.2 / 45.5 | +3.7% |
| `parse`, 3 KB greek note | 22.4 / 22.2 | 0.8% | 25.9 / 25.4 | +15.3% |
| `build`, zipf, foreground (ms) | 17.3 / 17.7 | 2.0% | 17.5 / 17.7 | +0.6% |
| `lookup`, settled, `e` | 1624 / 1615 | 0.5% | 1767 / 1756 | +8.8% |
| `lookup`, settled, top word | 249 / 245 | 1.6% | 248 / 248 | +0.2% |
| heap bytes | 34,430,861 | | 32,618,699 | -5.3% |
| posting bytes | 24,841,180 | | 23,548,118 | -5.2% |
| cache file bytes | 14,752,603 | | 11,279,043 | -23.5% |
| distinct words | 408,947 | | 381,967 | -6.6% |

The index shrinks: `_w_`, `__w__` and `w` merge into one entry `w`, where
base keeps three. The first draft's `lookup` cost on the top word (+51.9%)
is gone with `is_whole`. `search` on `zanzibar` is the one row past 10%, on
this vault only.

A real vault sits between the two vaults: most words hold no connector, and
the cost follows the share that does.

**The scan loops of the first draft.** The word rule stayed fixed while the
spike tried five loops for the joined flag. Each ran one quick pair against
base on the default vault. `search` `zanzibar` grew by 14.9% with a
connector test per char (`da1043b4`), and by 11.9% with one class test per
char (`8d6148c7`). The same, inlined (`51be3ce1`), gave +14.6%. A byte pass
after each span (`e8aba78d`) gave +5.6%, with `read_search` +14.2%. The
byte walk (`aa4068b1`) gave +3.9%. The byte walk alone, on `main`'s rule
(`baseS`), cuts `read_search` resident by 13.0% (open question 5).

**B.** One run of `af1353f8` against two of base, before this method
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
Files: `words/fold.rs` (part starts, connectors, `trim_trailing`,
`fold_head`, `WORD_MAX_PARTS`, the joined flag), `words/note_words.rs`,
`words/query.rs`, `words/shards.rs` (`#[inline]` on `get`), `find/terms.rs`
(`Spanned`, `Terms::parts`), `search/run.rs`, `persist/fingerprint.rs`,
`persist/header_tests.rs`, the words and search models. `words/lookup.rs`
does not change. Done: the five `search_finds_*` tests and the three search
`stays_*` tests pass, `terms_tests` passes, and the `search` bench shows no
`projetzz` row past base noise under the default profile.

**Slice 2. Quick open by text.**
Failing test first: `quick_open_finds_team_in_emphasis`.
Files: `find/excerpt.rs`, `find/rank.rs` (`#[inline]` on `rank`),
`find/model_cases.rs`, `find/model_tests.rs`. Done: the four
`quick_open_finds_*` text tests and the find `stays_*` tests pass, and the
`find` bench shows no `empty, 20 recent` row past base noise under the
default profile.

**Slice 3. Names.**
Failing test first: `quick_open_finds_notes_in_a_name`.
Files: `words/name_words.rs`, `find/label.rs`, `find/preview.rs`,
`find/rank.rs` (`is_exact` adds `word.first`, U7). Done: the name test
passes, and `todo_is_the_exact_name_of_underscored_todo` passes.

Each slice ends with `./scripts/check.sh` green and the mutants run of
"Tests".

## Tests

The sixteen tests of "Problem" move from the spike into the build, under
`search/underscore_tests.rs` and `find/underscore_tests.rs`. Revision 1
changes none of the sixteen: `git diff aa4068b1 HEAD` on both files only adds
tests. The rule changed under them, and each still holds, because none of
them types or stores a trailing connector.

Revision 1 adds these tests:

| Test | Asserts |
|---|---|
| `search::underscore_tests::a_trailing_connector_narrows_nothing` | `team_` lights `team` in `the team met` (U3) |
| `search::underscore_tests::a_part_past_the_word_s_sixty_fourth_is_no_part` | `zz` lights the 64th part of `a_…a_zz`, not a 65th (U8) |
| `find::underscore_tests::a_part_past_the_word_s_sixty_fourth_is_no_part` | the same bound in quick open (U8) |
| `find::underscore_tests::todo_is_the_exact_name_of_underscored_todo` | `todo` ranks `_todo_.md` above `todo list.md`, in both orders (U7) |
| `words::fold::tests::a_stored_part_ends_before_trailing_connectors` | the trim of U1's table, and of a non-ASCII connector |
| `words::fold::tests::a_word_is_joined_when_it_holds_a_connector` | the scan's flag equals "holds a connector" (U5) |
| `words::fold::tests::a_head_fold_reads_only_a_head_of_a_long_word` | `fold_head` on a 64 KiB `a_a_…` word, and across dropped marks (U8) |
| `words::fold::tests::a_head_fold_is_a_head_of_the_fold` (proptest) | `fold(w)` starts with `fold_head(w, c)`, and both cap to the same `c` chars |

The unit tests of the first draft stay:
`a_part_starts_the_word_or_follows_a_connector` (U1's table) and
`every_connector_past_ascii_has_a_three_byte_lead_of_e2_or_ef` (U2). The
first draft's `a_term_is_whole_when_only_connectors_follow_it` is gone with
`is_whole`.

Proptest properties:

- `terms_tests`: `each_word` agrees with `word_spans` across block ends, now
  with the flag; a block edge inside `a_b` keeps the OR of both halves.
- `words::model_tests::lookup_agrees_with_a_model`: the model splits words
  with `str::split`, cuts parts at each `_` run, and trims trailing `_`;
  `model_hit` counts every part a term starts, and calls it whole on `==`.
- `find::model_tests` and `search::model_tests`: the models' `words` yield
  trimmed parts, their `heads` yield first parts for the exact name.

On the spike head, `cargo test -q -p inkra-index` passes 589 tests.

Mutants: after each slice,
`git diff main... > $TMPDIR/m.diff && cargo mutants -j 4 --in-diff $TMPDIR/m.diff`.
Zero missed is the bar. Mutants the build's tests must kill:

- the flag's initial `true` in the part walk, and `.take(WORD_MAX_PARTS)`;
- `first: part == 0`, and `word.first` in `is_exact`;
- the `c < '\u{203F}'` early return in `is_connector`;
- each arm of `0x2 | 0xF` and the `&&` of the code-point gate;
- the trim's condition in `trim_trailing`;
- `word.len() <= chars * 4`, and each of the three exits of `fold_head`;
- the OR of the flags at a block join.

One mutant is equivalent: `may_join_byte` returning `true` only makes
`trim_trailing` and `starts` walk a word that has no connector, with the same
result. The build marks it with the reason, or drops the byte test if the
words bench shows no cost without it.

## Evidence

**Method.** Two build profiles, each run twice per side, interleaved. "Noise"
is the gap between base's two runs, over their mean.

- **Placement-stable:** one codegen unit, functions and blocks aligned to 64
  bytes. Every table of "Cost" but the default-profile one uses it.
- **Default:** the workspace's own bench profile, 16 codegen units. "Cost"
  gives one table for it (R3).

```sh
export CARGO_TARGET_DIR=…/target/wt-advisor-033
# placement-stable only:
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=6"
export CARGO_PROFILE_BENCH_CODEGEN_UNITS=1
cargo bench -p inkra-index --no-run          # at base, and at 329a6303 (C)
cd crates/index
for b in words search read_search find persist; do $BIN/$b --bench --quick; done
INKRA_BENCH_UNDERSCORE=1 $BIN/$b --bench --quick   # the underscore vault
```

Base is the index code of `c689f799`: `main`'s, plus the underscore corpus,
built with the spike's later benches. The final runs went base, C, base
default, C default, base underscore, C underscore, twice over.

**Tests.** On the spike head:

```sh
cargo test -q -p inkra-index
cargo test: 589 passed (2 suites)
```

The ten "finds" tests fail at `49f07edf`, which adds them to `main`'s code.
The three `stays_an_underscored_query_*` tests that check B fail at `af1353f8`.

**Commits on `spike/underscore-word-starts`.** `c689f799` the underscore
corpus; `49f07edf` the tests; `fdd17ef2` C; `af1353f8` B; `16a3573b` back to
C; `22d22345` to `da1043b4` the plain-word fast path and `rank` inlined;
`8d6148c7` to `aa4068b1` the scan loops of "Cost". `c56dc7f6` does not build
its tests; `6a2dcbc9` fixes them. Revision 1: `a62a7c74` trimmed parts and
terms; `da9b61c6` the new bench rows; `82ed9c37` a first capped fold;
`30a53057` the 64-part bound; `eeeb2603` the lead-byte gate; `03031f89`
`fold_head`; `46570f84` the two `#[inline]` hints; `329a6303` the code-point
gate. The R2 bisect builds (E2 to E7 in "Cost") were measured from a dirty
tree and never committed.

## Open questions

1. **Should camelCase or digit boundaries also start a part?** `team` would
   then find `myTeam`. Recommendation: no. Notes rarely hold camelCase, and a
   case boundary makes `jet` find `unProjet`-like words that the user did not
   mean. If a user asks for it, it meets the seam of U11 and needs its own
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
4. **Should the bench gate run under the default profile?** Under 16 codegen
   units, LLVM's inlining moved rows by more than the rule did: before the two
   hints, `projetzz` grew by 51% and the empty quick-open query by 107% to
   112% ("Cost", R3). Recommendation: keep both hints with their guard rows,
   and compare future builds under both profiles. A row that moves under one
   profile only is placement, not work.
5. **Should the build keep the byte walk if the rule were dropped?** It cuts
   `read_search` by 13% on `main` alone ("Cost", `baseS`). Recommendation: yes,
   land it in slice 1 with the rule, and keep `parse` on Greek text in the
   words bench as its guard.
6. **Is the scan's residual cost on long notes removable?** See "Cost", R2.
   No single part of the rule causes it. Recommendation: land the rule with
   the bound that "Cost" states, and keep `zanzibar once at its end` and the
   CJK rows in the search bench as its guard.
