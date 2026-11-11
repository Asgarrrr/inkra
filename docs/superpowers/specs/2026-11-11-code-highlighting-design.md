# Code highlighting: tree-sitter in fenced blocks

Status: **proposed** 2026-11-11. Builds on `2026-11-07-code-blocks-design.md`,
which leaves "tree-sitter or syntect" to this spec.

## Why

A fenced block today has one colour for every byte: `md_code_block`. A note
taker pastes code to read it later, and uniform grey code is slow to read.

This spec picks the library, names the token classes and their theme tokens,
and fixes the cost rule. Highlighting colours a block on top of
`md_code_block`; nothing else in the block's look changes.

## The choice

**Recommendation: tree-sitter, with one grammar crate per language.** Its full
pass is 3 to 12 times faster than syntect's on the same text, and its
incremental path re-highlights a 5,000-line block in under 1 ms. It passes
`cargo deny check` as the repo's `deny.toml` stands, and syntect fails it in
every feature set tried. Zed uses it, so its queries and its capture mapping
are a reference we can read.

**What would change it.** A rule that the binary must not grow by 5 MiB would
favour syntect (+2.0 MiB). A maintainer ruling that accepts bincode 1.3.3's
unmaintained advisory would remove syntect's deny failure. A wish for many
languages at once, with no crate per language, also favours syntect's bundled
set.

**The 12-month rule.** Six of the eight grammar crates in the first set had no
release in the last 12 months. Adopting them needs the maintainer to waive
their own rule for these crates. This spec does not grant that waiver; Open
questions asks for it.

### Registry

Source: `curl -sA inkra-dependency-check https://crates.io/api/v1/crates/<crate>`,
fields `max_stable_version`, that version's `created_at` and `license`, read
2026-11-11. Dependencies: `/api/v1/crates/<crate>/<version>/dependencies`.
Archive state: the repository's `archived` field on the GitHub API; no repo is
archived. **STALE** marks a crate with no release since 2025-11-11.

| Crate | Version | Released | Licence | Allowed | Build needs | Languages | Mark |
|---|---|---|---|---|---|---|---|
| tree-sitter | 0.27.2 | 2026-11-07 | MIT | yes | build script, C compiler (`cc`) | grammar crates | |
| tree-sitter-language | 0.1.9 | 2026-10-08 | MIT | yes | none | the `LanguageFn` seam | |
| streaming-iterator | 0.1.9 | 2023-01-06 | MIT OR Apache-2.0 | yes | none | (pulled by tree-sitter) | STALE |
| tree-sitter-highlight | 0.27.2 | 2026-11-07 | MIT | yes | none | not used (D2) | |
| tree-sitter-rust | 0.24.2 | 2026-03-27 | MIT | yes | build script, C compiler | Rust | |
| tree-sitter-bash | 0.25.1 | 2025-12-02 | MIT | yes | build script, C compiler | Bash | |
| tree-sitter-python | 0.25.0 | 2025-09-11 | MIT | yes | build script, C compiler | Python | STALE |
| tree-sitter-javascript | 0.25.0 | 2025-09-01 | MIT | yes | build script, C compiler | JavaScript | STALE |
| tree-sitter-css | 0.25.0 | 2025-09-28 | MIT | yes | build script, C compiler | CSS | STALE |
| tree-sitter-typescript | 0.23.2 | 2024-11-11 | MIT | yes | build script, C compiler | TypeScript, TSX | STALE |
| tree-sitter-json | 0.24.8 | 2024-11-11 | MIT | yes | build script, C compiler | JSON | STALE |
| tree-sitter-html | 0.23.2 | 2024-11-11 | MIT | yes | build script, C compiler | HTML | STALE |
| tree-sitter-toml-ng | 0.7.0 | 2024-12-03 | MIT | yes | build script, C compiler | TOML (not in the set) | STALE |
| tree-sitter-yaml | 0.7.2 | 2025-10-07 | MIT | yes | build script, C compiler | YAML (not in the set) | STALE |
| syntect | 5.3.0 | 2025-09-27 | MIT | yes | none with `regex-fancy`; C compiler with `regex-onig` | bundled Sublime syntaxes, or files at run time | STALE |
| two-face | 0.5.2+bat-0.26.1 | 2026-08-07 | MIT OR Apache-2.0 | yes | none | bat's extra syntaxes (TypeScript) | |
| onig / onig_sys | 6.5.3 / 69.9.3 | 2026-04-27 | MIT | yes | C compiler | (syntect's default regex) | |
| fancy-regex | 0.20.1 (syntect wants ^0.16.2) | 2026-11-09 | MIT | yes | none | (syntect's pure-Rust regex) | |
| bincode | 1.3.3 locked (3.0.0 latest) | 2025-12-16 for 3.0.0 | MIT | yes | none | (syntect's dump format) | RUSTSEC-2025-0141 |
| yaml-rust | 0.4.5 | 2021-01-03 | MIT/Apache-2.0 | yes | none | (syntect's `yaml-load`) | STALE, RUSTSEC-2024-0320 |

No core crate is deprecated. RustSec flags no tree-sitter or syntect crate; it
flags two of syntect's dependencies as unmaintained. tree-sitter's grammars
move slower than its core: four of them last released on 2024-11-11.

TypeScript is not in syntect's default syntax set. `find_syntax_by_token("ts")`
finds it only in two-face's `extra_newlines()` set.

### Measurements

Both spikes ran back to back on 2026-11-11. The machine's load average was 7
to 11 throughout, from other builds; read every figure as ±40 %. A first
tree-sitter set read 20 to 40 % lower than the back-to-back set below. Each
cell is the median of three runs; "full" is itself the median of 20.

Commands:

- tree-sitter: `CARGO_TARGET_DIR=… cargo test --release -p inkra-markdown --test spike_highlight -- --nocapture`
  on `spike/highlight-tree-sitter` (`0a751aab`);
- syntect: `CARGO_TARGET_DIR=… cargo test --release -p inkra-markdown --test spike_syntect -- --nocapture`
  on `spike/highlight-syntect` (`d62807d5`).

Texts: `$TMPDIR/hl/<language>-{20,500,5000}.txt`, real code. Rust and Bash come
from this repo. Python comes from Zed's `eval_cli`. JavaScript comes from the
grammar crates' `grammar.js`. TypeScript comes from web-tree-sitter 0.27.2.
HTML comes from freetype's docs, CSS from rustdoc's stylesheets, and JSON from
`cargo metadata`.

Time in µs. **Cold**: build the highlighter for one language from nothing.
**Full**: `(range, class)` pairs for the whole block. **Edit**: the pairs
again after one character is inserted mid-block.

| Language | Lines | tree-sitter cold | tree-sitter full | tree-sitter edit | tree-sitter local | syntect cold | syntect full | syntect edit |
|---|---|---|---|---|---|---|---|---|
| rust | 20 | 10,941 | 132 | 63 | 23 | 1,608 | 370 | 10 |
| rust | 500 | 10,941 | 2,222 | 979 | 79 | 1,608 | 6,951 | 5 |
| rust | 5,000 | 10,941 | 18,270 | 8,938 | 537 | 1,608 | 71,044 | 16 |
| python | 20 | 3,182 | 33 | 23 | 13 | 1,346 | 200 | 14 |
| python | 500 | 3,182 | 1,183 | 654 | 247 | 1,346 | 8,273 | 31 |
| python | 5,000 | 3,182 | 16,698 | 7,458 | 788 | 1,346 | 98,343 | 20 |
| javascript | 20 | 4,164 | 27 | 20 | 11 | 1,477 | 89 | 1 |
| javascript | 500 | 4,164 | 1,324 | 803 | 139 | 1,477 | 6,579 | 28 |
| javascript | 5,000 | 4,164 | 14,154 | 6,914 | 325 | 1,477 | 73,291 | 19 |
| typescript | 20 | 11,836 | 106 | 95 | 74 | 7,126 | 263 | 0 |
| typescript | 500 | 11,836 | 1,388 | 675 | 48 | 7,126 | 15,161 | 1 |
| typescript | 5,000 | 11,836 | 13,824 | 6,106 | 160 | 7,126 | 161,162 | 24 |
| json | 20 | 18 | 33 | 23 | 15 | 263 | 90 | 3 |
| json | 500 | 18 | 552 | 263 | 22 | 263 | 2,079 | 5 |
| json | 5,000 | 18 | 6,280 | 2,644 | 38 | 263 | 20,911 | 4 |
| bash | 20 | 2,078 | 65 | 46 | 34 | 1,333 | 310 | 29 |
| bash | 500 | 2,078 | 1,924 | 886 | 253 | 1,333 | 7,620 | 19 |
| bash | 5,000 | 2,078 | 8,162 | 3,220 | 724 | 1,333 | 29,367 | 3 |
| html | 20 | 20 | 22 | 11 | 5 | 455 | 101 | 17 |
| html | 500 | 20 | 491 | 188 | 14 | 455 | 2,138 | 32 |
| html | 5,000 | 20 | 6,264 | 2,325 | 80 | 455 | 22,980 | 0 |
| css | 20 | 273 | 19 | 11 | 4 | 1,853 | 76 | 0 |
| css | 500 | 273 | 996 | 507 | 61 | 1,853 | 4,651 | 13 |
| css | 5,000 | 273 | 9,342 | 4,578 | 263 | 1,853 | 44,652 | 15 |

Paths:

- tree-sitter cold is `Parser::new`, `set_language` and `Query::new` on the
  grammar's highlights query; `Query::new` dominates. Full is a parse from
  nothing and every capture of the tree.
- tree-sitter edit is `Tree::edit`, an incremental `parse(text, Some(&old))`,
  then every capture of the tree. Local is the same parse, then captures only
  in `changed_ranges` plus the edited line. The incremental parse alone is
  16, 66 and 428 µs on the three Rust texts.
- syntect cold is loading the syntax set, finding the syntax and parsing one
  line. Full is `ParseState::parse_line` per line. Edit restarts at the edited
  line from a stored `(ParseState, ScopeStack)` and stops when the state equals
  the old one; every edit stopped after one line. It needs one stored state
  per line of every block.

| Row | tree-sitter | syntect |
|---|---|---|
| 1. Cold start | 18 µs (JSON) to 11.8 ms (TypeScript), table above | 0.3 to 7.1 ms, table above |
| 2. Full pass | table above; 500 lines ≤ 2.2 ms, 5,000 lines 6 to 18 ms | table above; 500 lines ≤ 15 ms, 5,000 lines 21 to 161 ms |
| 3. After one edit | incremental parse; ≤ 0.8 ms with the local query | per-line state checkpoints; ≤ 32 µs |
| 4. Classes | Zed-style capture names (D3) | TextMate scopes: `keyword.control`, `storage.type`, `string.quoted`, `comment.line`, `constant.numeric`, `entity.name`, `meta.*`, `punctuation.*`, `support.*`, `variable.other` |
| 5. Binary size | 24,254,848 B, +5,215,184 B (+27 %) against 19,039,664 B | 21,098,640 B, +2,058,976 B (+11 %) |
| 6. Build time | 8 s and 9 s against 8 s and 8 s | 8 s and 8 s against 8 s and 8 s |
| 7. `cargo deny check` | exit 0; warning kinds identical to `main` | exit 1 in every feature set, below |
| 8. Lints | no `unsafe`, no `unwrap`: `Query::new` and `set_language` return `Result`, `parse` an `Option` | no `unsafe`, no `unwrap`: `parse_line` returns `Result`, `find_syntax_by_token` an `Option` |

Row 5: `stat -f %z target/wt-advisor-014/release/inkra-app` after
`cargo build --release -p inkra-app`. Both spikes link the dependency into
`main.rs` behind a throwaway `INKRA_SPIKE_HL` branch. `nm` shows all eight
`tree_sitter_*` symbols and `ts_query_new` in the tree-sitter binary.

Row 6: `cargo clean -p inkra-app --release && cargo build --release -p inkra-app`,
wall time, twice per branch, at a load average of 7.5 to 11. This rebuilds
only `inkra-app` and the link, not the grammars, so it shows no difference.
The full clean build was skipped: the shared disk and the parallel builds
did not allow it. The build's PR measures it.

Row 7, syntect, `cargo deny check` with syntect and two-face in `inkra-app` and
`inkra-markdown`:

| syntect features | two-face features | Exit | Errors |
|---|---|---|---|
| default | default | 1 | bincode 1.3.3 RUSTSEC-2025-0141; yaml-rust 0.4.5 RUSTSEC-2024-0320 |
| `default-fancy` | `syntect-default-fancy` | 1 | the same two |
| `default-syntaxes`, `regex-onig` | `syntect-onig` | 1 | bincode RUSTSEC-2025-0141 |
| `default-syntaxes`, `regex-fancy` | `syntect-fancy` | 1 | bincode RUSTSEC-2025-0141 |

bincode cannot leave: syntect's `parsing` feature needs `dump-load`, which
needs bincode. Only a `deny.toml` ignore would pass it, and that is the
maintainer's decision.

## Design

1. **A new crate, `inkra-highlight`, holds the highlighter.** It depends on
   `inkra-text` only, for `ByteOffset`; `inkra-app` depends on it. It has no
   gpui dependency and no clock. `inkra-markdown` would compile C grammars
   into `inkra-index` and its benches, against "one role per crate". `inkra-app`
   would put a `Window`-free algorithm where its tests need gpui, against "a
   test that needs no `Window` stays out of `app`". The build adds the crate
   to `CLAUDE.md` (Architecture), to `allowed_edges` (`scripts/check.sh:21`),
   and to `examine_globs` and `test_package` in `.cargo/mutants.toml`.

2. **The output type is a sorted vector of `(Range<ByteOffset>, TokenClass)`
   per block.** Ranges are absolute buffer offsets, disjoint, sorted by start,
   inside the block's range, on char boundaries. `TokenClass` is a `Copy` enum,
   not a `Styles` bit: four bits remain, and eleven classes do not fit.
   `line_runs` takes the block's tokens beside `spans`, with a second cursor.
   A token cuts a run as a span does, and its colour replaces `md_code_block`.
   `MARKER` still wins, so a revealed fence stays dimmed. The crate calls
   `Query` and `QueryCursor` directly, as Zed's `compute_resolved_highlights`
   does (`language.rs:1191-1250`). `tree-sitter-highlight` adds HTML output and
   injections we do not need.

3. **Eleven token classes, eleven theme tokens.** A capture maps to a class by
   its longest dotted prefix in one table, as Zed's `highlight_id` does
   (`syntax_theme.rs`, near line 87). A capture with no entry gets no token and
   keeps `md_code_block`.

   | Class | Token | Captures from the spike |
   |---|---|---|
   | `Comment` | `syntax_comment` | `comment`, `comment.documentation` |
   | `Keyword` | `syntax_keyword` | `keyword`, `label`, `variable.builtin` |
   | `String` | `syntax_string` | `string`, `string.special`, `escape` |
   | `Constant` | `syntax_constant` | `constant`, `constant.builtin`, `number` |
   | `Function` | `syntax_function` | `function`, `function.builtin`, `function.macro`, `function.method` |
   | `Type` | `syntax_type` | `type`, `type.builtin`, `constructor` |
   | `Property` | `syntax_property` | `property`, `string.special.key` |
   | `Tag` | `syntax_tag` | `tag`, `tag.error` |
   | `Attribute` | `syntax_attribute` | `attribute` |
   | `Punctuation` | `syntax_punctuation` | `punctuation.bracket`, `punctuation.delimiter`, `punctuation.special` |
   | `Operator` | `syntax_operator` | `operator` |
   | none | `md_code_block` | `variable`, `variable.parameter`, `embedded` |

   The tokens join `tokens!` in `colors.rs` after `md_code_block_background`.
   Each gets a dark and a light value in `builtin.rs`, from the Flexoki
   palette both built-ins already use. S1 picks the values against the
   `code-highlighting` scene. A vault theme starts from the built-in of its
   appearance (`theme/file.rs`), so a theme without these keys shows the
   built-in colours. The token count test moves from 30 to 41.

4. **An edit outside a block highlights nothing again; an edit inside a block
   highlights that block only.** A per-buffer `Highlights` global sits beside
   `Parses` in `editor/highlights.rs`, dropped with the buffer as `parsed`
   drops released parses. Its entries are keyed by `(language, block text)`,
   as Zed keys its cache by content (`language.rs:1162-1180`). A version key
   would miss every block on every keystroke. A text key survives an edit
   above a block, which shifts its range but not its bytes; the tokens are
   stored block-relative and shifted on read. The lookup compares bytes, one
   pass over the block text; S1 measures it against the keystroke median.
   Memory is bounded as Zed bounds it (`highlight_cache.rs:14-16`): 4 MiB of
   block text in total, and no entry over 512 KiB. A block over the entry cap
   keeps `md_code_block`.

5. **Highlighting runs on the main thread up to 32 KiB of block text, and in
   the background above it.** The 500-line full pass is at most 2.2 ms, within
   the 8 ms frame. The 5,000-line pass is 6 to 18 ms, over it. Rust measured
   2.2 ms at 16.5 KB and 18 ms at 173 KB, so 32 KiB is about 4 ms. Above the
   threshold, a background task parses and queries the block, as Zed's markdown
   does (`markdown.rs:1286`). Until it lands, the block shows its previous
   tokens, clamped by `line_runs`, or `md_code_block` on the first pass. An edit
   inside a large block keeps the old `Tree` and finds the edit by common
   prefix and suffix. The incremental parse plus the local query then took
   ≤ 0.8 ms. That path is S3.

6. **No info word, an indented block, or an unknown word keeps
   `md_code_block`, as today.** The info word resolves case-insensitively
   against each language's name and aliases (Languages). Zed matches the same
   way (`available_languages.rs:218`). An unknown word costs one table lookup.

7. **No highlighter work runs before the first frame.** A language's `Query`
   is built on its first block, in the background: `Query::new` costs up to
   11.8 ms (TypeScript), over a frame. Until it is ready, that language's
   blocks keep `md_code_block`. Launch is 1 to 11 ms above the AppKit floor
   (`docs/startup-floor.md`, lines 26-29); nothing here adds to it. A query is
   built once per language and kept for the process.

8. **The label is unchanged.** It shows the info word's source bytes
   (code-blocks spec, D1). Highlighting reads the same range to pick the
   language and never rewrites the label.

## Languages

**The first set is Rust, Python, JavaScript, TypeScript, JSON, Bash, HTML and
CSS.** These are the languages a note taker pastes most. The repo's own `docs/`
use `rust` 488 times (538 with variant spellings), `bash` 96, `toml` 21, `markdown` 5, `json` 3 and `sh` 1
(fence info words counted with `grep`). TOML is the third most used here and
is not in the set; Open questions asks.

| Language | Crate | Info words |
|---|---|---|
| Rust | `tree-sitter-rust` | `rust`, `rs` |
| Python | `tree-sitter-python` | `python`, `py` |
| JavaScript | `tree-sitter-javascript` | `javascript`, `js`, `mjs`, `cjs`, `jsx` |
| TypeScript | `tree-sitter-typescript` | `typescript`, `ts`, `tsx` |
| JSON | `tree-sitter-json` | `json`, `jsonc` |
| Bash | `tree-sitter-bash` | `bash`, `sh`, `shell`, `zsh` |
| HTML | `tree-sitter-html` | `html`, `htm` |
| CSS | `tree-sitter-css` | `css` |

`tsx` and `jsx` use `LANGUAGE_TSX`, as Zed parses JavaScript with the tsx
grammar (`grammars/src/javascript/config.toml:2`). TypeScript's query extends
JavaScript's; the spike concatenated both, and the build does the same.

**Adding a language** is one dependency in `inkra-highlight`'s manifest and
one table row: the `LanguageFn`, the highlights query and the info words. Most
grammar crates export `HIGHLIGHTS_QUERY`; Bash and JavaScript export
`HIGHLIGHT_QUERY`. A capture the class table lacks keeps `md_code_block`, so a
new grammar degrades to grey, never to a wrong colour. CI needs a C compiler
for each grammar; macOS runners have one.

## Cost

| Figure | Value | Command |
|---|---|---|
| Binary size | +5,215,184 B (+27 %) for eight grammars | `stat -f %z target/…/release/inkra-app` on `spike/highlight-tree-sitter` and on `main` |
| Rebuild of `inkra-app` | no change, 8 s | `cargo clean -p inkra-app --release && cargo build --release -p inkra-app` |
| Full clean build | not measured | the build's PR runs it |
| Keystroke outside a block | one byte compare per block, no parse | S1 measures it in the keystroke median |
| Keystroke inside a ≤ 32 KiB block | ≤ 2.2 ms at 500 lines | `spike_highlight`, full pass |
| Keystroke inside a larger block | ≤ 0.8 ms on the main thread with S3 | `spike_highlight`, local path |
| First block of a language | up to 11.8 ms, in the background | `spike_highlight`, cold |
| Launch | 0 ms | nothing runs before the first frame (D7) |

`markdown_parse` stays within noise: highlighting never runs inside
`inkra_markdown::parse`.

## Tests

- `inkra-highlight`, `#[test]`, one per language: a known input gives known
  classes at known ranges. For example, in Rust `fn main() {}` gives `fn` as
  `Keyword` and `main` as `Function`.
- `inkra-highlight`, `#[test]`: every info word in the Languages table
  resolves, in any case; an unknown word resolves to nothing; the class table
  maps each capture of every bundled query, or leaves it out on purpose.
- `inkra-highlight`, `proptest`: on any text, in any language, ranges are
  sorted, disjoint, inside the block and on char boundaries. After a random
  edit, the incremental path gives the same tokens as a full pass.
- `app`, `#[gpui::test]`:
  - a block's runs carry the token colours, and a revealed fence stays
    `md_marker`;
  - an edit outside every block highlights nothing again, counted by a
    test-only counter on the highlighter;
  - an edit inside one block highlights that block only;
  - a block over 32 KiB shows its old tokens until the background pass lands;
  - a vault theme without `syntax_*` keys shows the built-in colours.
- Scene `code-highlighting`, dark and light: one block per language, an
  unknown word, no info word, an indented block. Look at it after each slice.
- `cargo mutants` on `inkra-highlight`: zero missed.

## Slices

Each slice is vertical: crate, type, cache, theme, test, scene.

1. **S1, Rust end to end.** `inkra-highlight` with one grammar, `TokenClass`,
   the class table, `Highlights` beside `Parses`, the eleven theme tokens,
   `line_runs` reading tokens, the scene. Main thread only; a block over
   32 KiB stays grey.
2. **S2, the first set.** The seven other grammars, the info-word table, the
   per-language tests, the scene's other blocks.
3. **S3, large blocks.** The background pass over 32 KiB and the incremental
   path with the local query. Only if S1's keystroke median shows the need;
   the measurements above say it does for 5,000-line blocks.

## Deferred

| Item | Trigger |
|---|---|
| TOML, YAML, Markdown and more languages | Asked for, or a fence word the set lacks becomes common in a vault |
| Injections (CSS in HTML, a language inside Markdown) | Asked for. The direct `Query` path has none |
| Highlighting inline code and front matter | Asked for. Out of this spec's scope |
| A per-language colour override in vault themes | Asked for after the eleven tokens ship |
| Persisting tokens across launches | The background pass shows on a cold open as visible flicker |
| Removing grammars at build time behind features | The binary size becomes a complaint |

## Open questions

1. **The language set.** Recommendation: ship the eight above. Add TOML in S2
   only if the maintainer waives the rule for `tree-sitter-toml-ng`, last
   released 2024-12-03. Zed's `native_grammars()` lists no HTML
   (`grammars.rs:20-45`); keep HTML anyway, because notes quote markup more
   often than Zed's users edit it.
2. **crates.io releases or Zed's git pins.** Zed pins `tree-sitter` by git rev
   (`Cargo.toml:946`), several grammars by git, and patches
   `tree-sitter-language` (`Cargo.toml:1039-1043`). Recommendation: crates.io
   releases. They pass deny as it stands, and Inkra needs no fix Zed carries.
3. **A grammar as a git dependency.** `deny.toml` sets `unknown-git = "deny"`.
   Recommendation: no git grammar in the first version. A language only
   available from git waits for a release, or gets an `allow-git` entry with a
   reason, which the maintainer decides.
4. **Vault themes and the token colours.** Recommendation: yes, from S1. The
   eleven tokens are ordinary `tokens!` entries, so `theme/file.rs` reads them
   with no change, and a theme without them keeps the built-in values.
5. **The waiver for the six stale grammar crates.** `tree-sitter-python`,
   `-javascript`, `-css`, `-typescript`, `-json` and `-html` had no release in
   12 months. Recommendation: waive the rule for them as one group. A grammar
   is a generated C parser plus a query; it changes when its language does.
   None is archived, and each repository had pushes in September 2026.
6. **The waiver for `streaming-iterator`.** It last released 2023-01-06 and
   comes with tree-sitter itself. Recommendation: waive it. It is a small
   trait crate that `tree-sitter` 0.27.2, released this month, still requires.
7. **The waiver for syntect, if the maintainer picks it over this
   recommendation.** syntect 5.3.0 is from 2025-09-27, and yaml-rust is from
   2021. Recommendation: do not waive; syntect also fails deny on bincode in
   every feature set, so it needs a second exception.

## Rejected alternatives

- **syntect with two-face.** Its full pass is 3 to 12 times slower: 71 ms on
  the 5,000-line Rust text against tree-sitter's 18 ms, and 15 ms on the
  500-line TypeScript text against 1.4 ms. Its cheap edit needs one stored
  `(ParseState, ScopeStack)` per line of every block. TypeScript needs
  two-face's second syntax set, 7.1 ms cold. `cargo deny check` fails in all
  four feature sets on bincode's unmaintained advisory. It costs half
  tree-sitter's binary size (+2.0 MiB) and needs no C compiler with
  `regex-fancy`; neither outweighs the deny failure.
- **`tree-sitter-highlight`.** It adds HTML rendering and injection handling.
  Zed's markdown path uses `Query` directly; so do we (D2).
- **`tree-sitter-md` for the whole note.** Already rejected for parsing:
  measured 20 times slower than pulldown-cmark (live-preview spec, X1).
- **Highlighting inside `inkra_markdown::parse`.** It would make every
  keystroke pay for every block, and pull C grammars into `inkra-index`.

## Deviations from Zed

- **crates.io releases, not git pins.** `deny.toml` denies unknown git
  sources, and no fix in Zed's pins is needed for a note's code block.
- **Eleven classes, not a theme of free capture names.** Zed's syntax theme
  keys colours by any capture name. A note app needs a short, fixed token
  list that a vault theme can set in full.
- **A 32 KiB synchronous threshold.** Zed's markdown always highlights in a
  background task. Most note blocks are short, and the main thread gives them
  colour in the same frame, with no grey flash.
- **The cache is per buffer, not one process-wide store.** It follows
  `Parses`, so a closed note frees its entries. The bounds are Zed's.
- **No HTML exclusion.** Zed lists no HTML grammar; Inkra ships one (Open
  questions, 1).
