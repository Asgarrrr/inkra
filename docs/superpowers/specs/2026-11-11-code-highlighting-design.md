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
pass is 3 to 12 times faster than syntect's on the same text. Its
incremental path re-highlights a 5,000-line block in at most 0.82 ms, measured
end to end (The incremental path, below). It passes
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
| 3. After one edit | incremental parse; ≤ 0.82 ms end to end at 5,000 lines (The incremental path) | per-line state checkpoints; ≤ 32 µs |
| 4. Classes | Zed-style capture names (D3) | TextMate scopes: `keyword.control`, `storage.type`, `string.quoted`, `comment.line`, `constant.numeric`, `entity.name`, `meta.*`, `punctuation.*`, `support.*`, `variable.other` |
| 5. Binary size | 24,254,848 B, +5,215,184 B (+27 %) against 19,039,664 B | 21,098,640 B, +2,058,976 B (+11 %) |
| 6. Build time | clean tree-sitter crates: 11 s release, 4 s debug (Clean build, below) | `inkra-app` only: 8 s and 8 s against 8 s and 8 s |
| 7. `cargo deny check` | exit 0; warning kinds identical to `main` | exit 1 in every feature set, below |
| 8. Lints | no `unsafe`, no `unwrap`: `Query::new` and `set_language` return `Result`, `parse` an `Option` | no `unsafe`, no `unwrap`: `parse_line` returns `Result`, `find_syntax_by_token` an `Option` |

Row 5: `stat -f %z target/wt-advisor-014/release/inkra-app` after
`cargo build --release -p inkra-app`. Both spikes link the dependency into
`main.rs` behind a throwaway `INKRA_SPIKE_HL` branch. `nm` shows all eight
`tree_sitter_*` symbols and `ts_query_new` in the tree-sitter binary.

Row 6, syntect: `cargo clean -p inkra-app --release && cargo build --release -p inkra-app`,
wall time, twice per branch, at a load average of 7.5 to 11. This rebuilds
only `inkra-app` and the link, not the grammars. The tree-sitter figure
rebuilds every tree-sitter crate from clean (Clean build, below).

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

### The incremental path

A second spike measures the path that D4 to D6 design, on the chosen library
only. It ran on 2026-11-11 at a load average of 2.1 to 4.3. Each cell is the
median of three runs; each run is itself a median.

Command, on `spike/highlight-tree-sitter` (`04244054`), with
`HL_DIR=$TMPDIR/hl`:
`CARGO_TARGET_DIR=… cargo test --release -p inkra-markdown --test spike_incremental <test> -- --nocapture --test-threads=1`.
The texts are the ones above.

The path per edit: fold the splices into one hull, widen it to whole lines,
`Tree::edit`, reparse with the old tree, then paint a window. The window is
the edited lines joined with `changed_ranges`. Captures run only over the
window, with `QueryCursor::set_byte_range`. The new tokens replace the old
ones inside the window; tokens after it shift by the edit's length.

**Correctness** (`spike_incremental_matches_full`): 300 random steps per text,
one splice per step and three splices every tenth step. Each step compares the
path's tokens with a full pass. A cell counts mismatching steps at 20 and at
500 lines.

| Language | Hull, one step | Hull, chained | Zed-style exact edits | Hull vs exact | Mismatch on an error-free tree |
|---|---|---|---|---|---|
| rust | 0, 0 | 0, 0 | 0, 0 | 0, 0 | 0, 0 |
| python | 0, 0 | 0, 0 | 0, 0 | 0, 0 | 0, 0 |
| javascript | 0, 0 | 0, 0 | 0, 0 | 0, 0 | 0, 0 |
| bash | 0, 37 | 0, 37 | 0, 41 | 0, 4 | 0, 0 |

Every Bash mismatch has an error in the full pass's tree. Zed's exact
per-splice edits diverge there too, so the hull does not cause it.
tree-sitter-bash's incremental reparse of broken text can differ from a fresh
parse. Repairs, below, show that the path converges again.

**Cost** (`spike_incremental_cost`): one character inserted, one deleted, and
one newline, mid-block, median of 20. Time in µs; the window is the insert's.

| Language | Lines | Bytes | First pass | Insert | Delete | Newline | Window bytes |
|---|---|---|---|---|---|---|---|
| rust | 20 | 923 | 83 | 21 | 21 | 21 | 91 |
| rust | 500 | 16,501 | 1,689 | 71 | 69 | 68 | 19 |
| rust | 5,000 | 173,348 | 15,264 | 477 | 447 | 448 | 44 |
| python | 20 | 387 | 31 | 13 | 42 | 13 | 39 |
| python | 500 | 18,586 | 942 | 193 | 261 | 256 | 600 |
| python | 5,000 | 180,278 | 13,403 | 791 | 790 | 818 | 48 |
| javascript | 20 | 359 | 25 | 11 | 10 | 10 | 47 |
| javascript | 500 | 11,448 | 1,092 | 101 | 100 | 101 | 48 |
| javascript | 5,000 | 124,228 | 11,334 | 325 | 310 | 309 | 31 |
| bash | 20 | 991 | 60 | 32 | 32 | 32 | 87 |
| bash | 500 | 21,519 | 1,522 | 207 | 214 | 205 | 124 |
| bash | 5,000 | 158,642 | 7,912 | 779 | 775 | 749 | 42 |

Python and Bash are the worst on this path: at most 0.82 ms per median, and
no single run over 0.87 ms. The reparse dominates; the window stays under
600 bytes. Large blocks, below, measures the path up to the 512 KiB entry
limit.

### Large blocks

Command: the one above, at `b4c4622`, test `spike_large_steps`, with
`HL_VARIANT=scan|tree` and `HL_SIZES=16,32,64,128` for the small sizes. Each
text repeats the language's 5,000-line text and is cut at a line. Each case is
one keystroke: an insert, a delete or a newline, at the block's start, middle
or end. A case is the median of 15 keystrokes per run, three runs. Every case
equalled a full pass, and both row methods gave the same `InputEdit`.

Two variants of the path ran:

- **scan** is round 2's path. It counts newlines before and after the hull for
  the rows, rebuilds the token vector, and recounts the block's newlines.
- **tree** is D4's path. It reads the rows from the old tree's top-level
  nodes, splices the token vector in place, and reads the counts from the
  root's end position.

Steps at 512 KiB, median over the nine cases, in µs. Load average 9.3 to 11.2
for scan, 7.3 to 10.7 for tree.

| Language | Variant | Rows | `Tree::edit` | Reparse | `changed_ranges` | Captures | Token splice | Recount | Total |
|---|---|---|---|---|---|---|---|---|---|
| rust | scan | 59 | 0 | 672 | 385 | 4 | 141 | 61 | 1,328 |
| rust | tree | 1 | 0 | 750 | 420 | 5 | 13 | 0 | 1,206 |
| python | scan | 59 | 1 | 10,246 | 293 | 5 | 33 | 61 | 10,703 |
| python | tree | 1 | 1 | 10,323 | 300 | 6 | 3 | 1 | 10,636 |
| javascript | scan | 59 | 0 | 1,060 | 80 | 2 | 123 | 60 | 1,389 |
| javascript | tree | 5 | 0 | 1,154 | 84 | 6 | 17 | 0 | 1,327 |
| bash | scan | 59 | 0 | 2,384 | 794 | 3 | 53 | 61 | 3,381 |
| bash | tree | 1 | 1 | 2,869 | 989 | 6 | 7 | 0 | 3,937 |

Total per keystroke, tree variant, in µs: the median over the nine cases,
then the largest single case in brackets. Load average 13.9 to 15.0 for 16 to
128 KiB, 7.3 to 10.7 for 256 and 512 KiB.

| KiB | Rust | Python | JavaScript | Bash |
|---|---|---|---|---|
| 16 | 55 (86) | 67 (95) | 107 (212) | 127 (188) |
| 32 | 106 (185) | 80 (131) | 130 (296) | 257 (409) |
| 64 | 190 (245) | 245 (284) | 213 (302) | 467 (659) |
| 128 | 274 (459) | 647 (787) | 188 (397) | 872 (1,061) |
| 256 | 581 (877) | 3,774 (4,716) | 521 (686) | 2,160 (2,482) |
| 512 | 1,206 (1,644) | 10,636 (13,401) | 1,327 (1,915) | 3,937 (5,114) |

**Why the cost grows.** Scan has three O(block) steps: the two newline counts
for the rows, the token vector's rebuild, and the recount. Tree removes all
three; they cost at most 17 µs together at 512 KiB. Two steps remain, and
both grow with the block at every position, even at its end: the reparse and
`changed_ranges`. tree-sitter probably visits every top-level node of the old
tree to reuse it, and walks both trees' top-level siblings to compare them.
These two steps are O(block) inside tree-sitter, and the design cannot remove
them without splitting a block into several trees. Python's slope steepens
past 180 KB, and the spike did not find why.

**The bound.** The keystroke budget is 1 ms, Zed's synchronous parse timeout
(`buffer.rs:1177-1181`). 64 KiB is the largest size under it in every case:
at most 659 µs. At 128 KiB one Bash case reaches 1,061 µs. D4 therefore
reparses on the main thread only up to 64 KiB.

**Copying a block.** Above 64 KiB, D4 hands the reparse a copy of the block's
text. A standalone `rustc -O` program timed `String::clone` 201 times per size,
three runs, at a load average of 13.2. The copy takes a median of 0.6 µs at
64 KiB and 5.9 µs at 512 KiB; the largest single copy took 34 µs.

### Repairs

Command: the one above, at `b4c4622`, test `spike_repair_converges`, one run;
the figures count steps, not time. Load average 8.4 to 11.5. Each language
gets 200 break-then-repair sequences, chained on one entry, of three kinds:

- a random splice, then its inverse;
- a deleted delimiter (`()[]{}"'`), two letters typed and deleted, then the
  delimiter retyped;
- an opening quote, three letters, the closing quote, then all five deleted.

A sequence the text cannot host is skipped. Each base text is error-free: the
text cut before its first top-level error. JavaScript at 500 lines uses the
5,000-line text's clean prefix instead, because the 500-line cut splits one
top-level call. Each step compares the path's tokens with a full pass. A step
is "clean" when the full pass's tree has no error.

| Language | Lines | Base bytes | Sequences | Steps | Error steps | Mismatches, error | Clean steps | Mismatches, clean | Mismatches, last step |
|---|---|---|---|---|---|---|---|---|---|
| rust | 20 | 702 | 200 | 928 | 547 | 0 | 381 | 0 | 0 |
| rust | 500 | 16,501 | 200 | 908 | 625 | 0 | 283 | 0 | 0 |
| python | 20 | 387 | 196 | 860 | 597 | 0 | 263 | 0 | 0 |
| python | 500 | 18,586 | 200 | 940 | 545 | 0 | 395 | 0 | 0 |
| javascript | 20 | 219 | 190 | 860 | 2 | 0 | 858 | 0 | 0 |
| javascript | 500 | 123,467 | 200 | 920 | 650 | 1 | 270 | 0 | 0 |
| bash | 20 | 627 | 199 | 922 | 287 | 0 | 635 | 0 | 0 |
| bash | 500 | 21,374 | 200 | 908 | 424 | 1 | 484 | 0 | 0 |

Over 1,585 sequences, 2 of 3,677 error steps differed from a full pass, and
none of 3,569 clean steps did. Every repair that left the tree error-free
restored the full pass's tokens, so no repair rule is needed. JavaScript at 20
lines is mostly a comment and saw only 2 error steps.

### Query builds and parallel first passes

Command: the one above, at `b4c4622`, tests `spike_query_builds`,
`spike_query_memory` and `spike_parallel_first_pass`, three runs each. The
load average stayed at 13.2 to 15.4; it did not fall during the session.

**Query builds.** `Query::new` per language, median of 7 per run, then the
median of three runs, in µs. "All eight" builds them in eight threads at once.
Live memory is the default malloc zone's `bytes_used` (`mstats`) after each
build, with every query kept; the three runs agreed to the KiB.

| Language | Build | Live memory |
|---|---|---|
| rust | 7,866 | 76 KiB |
| python | 2,076 | 110 KiB |
| javascript | 3,542 | 66 KiB |
| typescript (with JavaScript's query) | 8,944 | 75 KiB |
| json | 4 | 2 KiB |
| bash | 1,276 | 12 KiB |
| html | 5 | 2 KiB |
| css | 124 | 26 KiB |
| sum, one by one | 24,287 | 373 KiB |
| all eight, in parallel | 10,952 wall | 373 KiB |

The process RSS grows by 15,424 KiB across the eight builds (median of 15,344,
16,928 and 15,424). Most of it is probably freed build memory the allocator
keeps resident; the live queries hold 373 KiB.

**Parallel first passes.** One `Parser` per thread; the threads take blocks
from a shared queue, largest first. Median of 7 per run, then the median of
three runs, in µs. The 51-block note is the one from Recognising an unchanged
block; the 50-block note drops its 173 KB block.

| Note | Bytes | Serial | Largest block alone | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|---|---|
| 51 blocks | 274,660 | 35,063 | 18,511 | 23,327 | 19,784 | 19,774 |
| 50 blocks of about 2 KB | 101,312 | 15,106 | 2 KB block, not timed | 8,614 | 5,621 | 3,386 |

The 173 KB block bounds the 51-block note: four threads cannot beat it. On
small blocks four threads cut the wall time by 63 %. At this load the serial
rate is about 149 µs per KiB, 1.6 times round 1's 91 µs at a load of 2 to 4.

### Binary size by grammar

Command, on `spike/highlight-tree-sitter` at `b4c4622`:
`CARGO_TARGET_DIR=… cargo rustc --release -p inkra-app --bin inkra-app -- -C link-arg=-Wl,-map,$TMPDIR/inkra-map.txt`,
then the size of every symbol in the map's `# Symbols:` section, summed per
`.rlib`. One run: a link map is deterministic. The binary is 24,254,848 B, as
in row 5. A second link references `LANGUAGE_TSX` as well; without that
reference the linker loads no TSX object.

| Object | Linked bytes |
|---|---|
| `tree_sitter_typescript`, TypeScript only | 1,413,593 |
| `tree_sitter_bash` | 1,365,332 |
| `tree_sitter_rust` | 1,118,273 |
| `tree_sitter_python` | 453,347 |
| `tree_sitter_javascript` | 410,597 |
| `tree_sitter` (the core) | 170,954 |
| `tree_sitter_css` | 129,577 |
| `tree_sitter_html` | 18,174 |
| `tree_sitter_json` | 5,583 |
| all of the above | 5,085,430 |
| TSX, the second link's binary minus the first | 1,454,448 |

The 129,754 B left of row 5's delta is the spike's own code and padding.
TypeScript, Bash and Rust carry 77 % of the grammars. In the second link's map
the TypeScript crate is 2,853,242 B, the largest by far; Open questions 1
drops TSX.

### First pass by size

Command: the one above, test `spike_first_pass_sizes`. Rust and JavaScript are
the two slowest languages per byte in the full-pass table. Each text is the
5,000-line text cut at a line boundary. Median of 10 per run; three runs. Time
in µs, parse plus query.

| KiB | Rust | JavaScript |
|---|---|---|
| 2 | 244 / 237 / 234 | 185 / 183 / 191 |
| 8 | 804 / 797 / 793 | 749 / 737 / 723 |
| 16 | 1,556 / 1,502 / 1,588 | 1,515 / 1,543 / 1,515 |
| 32 | 3,155 / 3,169 / 3,082 | 2,974 / 2,957 / 2,969 |
| 64 | 6,023 / 5,934 / 5,978 | 5,819 / 5,746 / 5,771 |
| 128 | 11,006 / 10,661 / 10,829 | 11,589 / 11,370 / 11,548 |
| 256 | 21,814 / 21,782 / 22,024 | 23,345 / 23,248 / 23,333 |

The cost is linear, about 91 µs per KiB. The parse takes about 53 % and the
query about 47 %. The largest size whose first pass stays under 2 ms is
16 KiB (D6).

### Recognising an unchanged block

Command: the one above, test `spike_recognise_unchanged`. The note holds 50
blocks of about 2 KB from the 500-line Rust text, plus the 173 KB Rust block
after block 24. One keystroke lands in the prose after block 10. Median of
2,000; three runs gave the same figures.

| Figure | Value |
|---|---|
| Blocks, block bytes, note bytes | 51, 274,660, 277,531 |
| First pass of every block | 30,239 µs |
| Edit-range intersection over 51 blocks (D4) | 83 ns |
| Byte compare against a stored copy of every block | 5,250 ns |
| `DefaultHasher` over every block | 46,833 ns |

### Tree memory

Command: the one above, test `spike_tree_memory`, with `HL_MEM_CASE=<case>`,
one case per process. It keeps 50 trees and reads the RSS growth with `ps`.
Three runs per case gave the same figures within 2 KiB.

| Case | Text bytes | Tree | Nodes | Tokens | Token vector |
|---|---|---|---|---|---|
| rust 500 | 16,501 | 670 to 672 KiB | 5,789 | 2,621 | 96 KiB |
| rust 5,000 | 173,348 | 5,445 KiB | 52,969 | 24,703 | 768 KiB |
| javascript 5,000 | 124,228 | 3,969 KiB | 37,990 | 18,018 | 768 KiB |
| python 5,000 | 180,278 | 4,727 KiB | 41,224 | 9,862 | 384 KiB |

A tree costs about 105 to 118 B per node, 26 to 41 times its text. The token
vector is capacity, at 24 B per `(Range<usize>, u8)`; block-relative `u32`
ranges take 12 B.

### Clean build

Command, on `spike/highlight-tree-sitter` (`0a751aab`):
`cargo clean [--release] -p tree-sitter -p tree-sitter-language -p streaming-iterator -p tree-sitter-{rust,python,javascript,typescript,json,bash,html,css}`,
then `cargo build [--release] -p inkra-app --timings`. Free disk was 33 GiB.

| Profile | Cleaned | Wall | Load | Slowest units |
|---|---|---|---|---|
| release | 274 files, 64.9 MiB | 11 s | 3.07 to 2.91 | `inkra-app` 5.66 s; tree-sitter build script 4.00 s; `tree-sitter-rust` 2.70 s; `tree-sitter-typescript` 2.60 s; `tree-sitter-bash` 2.59 s |
| debug | 266 files, 66.0 MiB | 4 s | 2.91 to 3.56 | `tree-sitter-rust` 1.64 s; `tree-sitter-typescript` 1.45 s; `tree-sitter-bash` 1.41 s; `tree-sitter-json` 1.29 s |

The grammars compile in parallel, so the slowest grammar bounds their share.
`inkra-app` rebuilds after them in any case.

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
   palette both built-ins already use (Palette). S1 checks them against the
   `code-highlighting` scene. A vault theme starts from the built-in of its
   appearance (`theme/file.rs`), so a theme without these keys shows the
   built-in colours. The token count test moves from 30 to 41.

4. **An edit outside a block highlights nothing again; an edit inside a block
   updates that block incrementally.** A per-buffer `Highlights` global sits
   beside `Parses` in `editor/highlights.rs`. It drops with the buffer, as
   `parsed` drops released parses. It holds one entry per block: the
   language, the content range at a splice mark, the `Tree` and the tokens.
   Tokens are stored block-relative, as `u32` ranges, and shifted on read.

   After each parse, `Highlights` reads the splices since its mark (D5). It
   walks them against the entries' ranges in one merge, both sorted by
   offset. An entry that no splice touches keeps its tree and tokens; only
   its offset shifts. This costs 83 ns for 51 blocks (Recognising an
   unchanged block). A byte compare needs a stored copy of every block and
   takes 5.3 µs. `DefaultHasher` takes 47 µs, and a hash cannot find the edit.

   An entry matches a block of the new parse only if three things hold. The
   language is the same. The mapped content range equals the block's new
   content range. No splice crosses the content's start or end. So a fence
   typed, deleted or moved, or a changed info word, gives the block a first
   pass. Every splice inside the content updates the entry incrementally.

   The incremental path folds the block's splices into one hull. It widens
   the hull to whole lines and reads the rows from the old tree's top-level
   nodes, not from newline counts. It calls `Tree::edit`, reparses with the
   old tree, and paints only the edited lines plus `changed_ranges`. The new
   tokens replace the old ones in place; the tail shifts by the edit's
   length. It equals a full pass whenever the full pass's tree has no error,
   and it returns to the full pass's tokens as soon as a repair clears the
   error (Repairs).

   No step of the path scans the block's text. The reparse and
   `changed_ranges` still grow with the block, inside tree-sitter (Large
   blocks). The path therefore runs whole on the main thread only for a
   block of at most 64 KiB: at most 0.66 ms in every measured case, under
   the 1 ms keystroke budget.

   Above 64 KiB, the main thread runs `Tree::edit` and the token shift only,
   at most 17 µs at 512 KiB. A background task gets a clone of the edited
   tree and a copy of the block's text, at most 34 µs at 512 KiB. It
   reparses, reads `changed_ranges`, captures, and returns the new tree and
   tokens. Until it lands, the edited lines show their shifted old tokens,
   and typed bytes keep `md_code_block`. The lag is the reparse: a median of
   0.5 to 3.8 ms at 256 KiB, and up to 13.4 ms for Python at 512 KiB.

   One task runs per block at a time. Edits made while it runs go through
   `Tree::edit` and the token shift on the main thread and wait. When the
   task lands, `Highlights` applies those edits to the returned tree and
   tokens, then starts one task for all of them. Fast typing therefore
   costs at most one reparse in flight per block, never one per keystroke.

   While the full pass's tree has an error, the edited lines can show other
   colours than a fresh parse would. This is bounded: 2 of 3,677 such steps
   differed, and the first error-free step restored the full pass's tokens
   in every sequence measured (Repairs).

   Memory has two bounds. Block text counts toward 4 MiB in total, with no
   entry over 512 KiB, as Zed bounds its cache (`highlight_cache.rs:14-16`).
   These bound the tokens: about 1.7 times the text at 12 B per token. Trees
   get their own LRU budget of 16 MiB, estimated as `descendant_count` ×
   120 B (Tree memory). The tree of the last edited block is never evicted,
   even alone over the budget. An entry whose tree is evicted keeps its
   tokens; its next edit is a first pass. A block over 512 KiB keeps
   `md_code_block`: its tree would cost 13 to 21 MiB.

5. **`inkra-text` keeps a splice log with a monotonic mark.** `Buffer::splice`
   is the one text mutation; `edit`, `undo`, `redo` and `apply_diff` all go
   through it. It appends `Splice { at, removed_len, inserted_len }` to a ring
   of 256 entries and increments a `u64` mark. Two read-only methods expose
   it: `edit_mark()` and `edits_since(mark)`. The second returns `None` once
   the ring has dropped splices past that mark. `None` gives the note's blocks
   a first pass.

   The log adds no mutation path, so `transact_at` stays the only way to edit.
   Undo and redo reach `splice` like any edit, so the log records them as
   forward splices. The mark is not a `Version`: `Version` has no order, and
   undo returns an older one. A multi-range edit logs one splice per range;
   the path folds them into one hull per block. The spike measured the
   folded hull equal to a full pass (The incremental path). `inkra-editor`
   gains nothing. The ring costs 6 KiB per buffer.

   Comparing the old and new text by common prefix and suffix needs no new
   API. It needs a stored copy of every block and a second scan of the block
   text on each keystroke. CLAUDE.md allows one whole-document scan per
   keystroke, and the markdown parse already spends it.

6. **A first pass runs on the main thread only for a block of at most 16 KiB,
   within 2 ms per frame.** The frame is 8.33 ms at 120 Hz. The draw takes
   about 4.5 ms (`docs/startup-floor.md`, lines 60-75). The markdown parse and
   the layout need the rest of the margin, so first passes get 2 ms. The
   largest size under 2 ms is 16 KiB: Rust 1.50 to 1.59 ms, JavaScript 1.52
   to 1.54 ms; 32 KiB costs about 3.1 ms (First pass by size). First passes
   run in document order until the 2 ms is spent. A larger block, or one past
   the budget, goes to a background task, as Zed's markdown does
   (`markdown.rs:1286`). Until the task lands, the block shows its shifted old
   tokens, or `md_code_block` if it has none. The limit is for first passes
   only; every edit inside a block takes D4's incremental path, with D4's
   own 64 KiB limit.

7. **No info word, an indented block, or an unknown word keeps
   `md_code_block`, as today.** The info word resolves case-insensitively
   against each language's name and aliases (Languages). Zed matches the same
   way (`available_languages.rs:218`). An unknown word costs one table lookup.

8. **A note lands with its colours; the main thread does no highlighter work
   before the first frame, by construction.** The first frame holds no note
   text. `NoteStore::open_note` loads the file in `background_spawn` and
   builds the `Buffer` when the task lands (`vault/store.rs`, 174-217). A
   restore waits for the vault walk and lands every open in one update
   (`shell/session.rs`, 217-294). The goal is therefore colours in the same
   frame as the text, not in the first frame.

   **The queries build at process start.** `run()` spawns one background
   task per language before the window opens; each builds that language's
   `Query` into a process-wide cell. All eight take 11 ms of wall time in
   parallel and 24 ms of CPU, and hold 373 KiB (Query builds). The first
   frame comes 100 to 140 ms after `run()` starts (`docs/startup-floor.md`),
   and the restore lands after it. So every query is ready before any open
   needs it. An open that comes earlier waits on the cell, never builds twice.

   **The open's background task gains one step.** It runs
   `inkra_markdown::parse` on the loaded text to find the blocks. It then
   first-passes them in up to four parallel tasks, one `Parser` each,
   largest block first. It returns the trees and tokens with the text.
   `Highlights` takes them as entries at mark 0, so the first frame with the
   text has its colours.

   **The landing-delay budget is 8 ms, one frame at 120 Hz.** An open
   already waits for the file read. Holding the text one frame more, so it
   lands coloured, does not show; grey text that turns coloured a frame
   later does. The budget gives two caps:
   - a block over 32 KiB is not coloured before the landing: its first pass
     takes 3.1 ms at a load of 2 to 4, and about 5 ms at a load of 15;
   - at most 128 KiB of block text is coloured before the landing. Four
     tasks colour 99 KiB of small blocks in 5.6 ms at a load of 15 (Parallel
     first passes).

   Blocks past either cap get a second task after the landing, and show
   `md_code_block` until it lands. This is D6's background pass.

   Four cases follow. The first-pass rate is round 1's 91 µs per KiB, at a
   load of 2 to 4; at a load of 15 every figure is about 1.6 times as long.
   - **A note with 8 KiB of Rust, at launch:** its query is ready. The
     landing waits about 0.8 ms for the first pass.
   - **The same note, opened later:** the same 0.8 ms.
   - **The 51-block note:** its 50 small blocks, 99 KiB, colour in four
     tasks before the landing: 5.6 ms at a load of 15, against 15.1 ms one
     by one. Its 173 KB block exceeds the 32 KiB cap. It lands 18.5 ms after
     the text, at a load of 15, and shows `md_code_block` until then.
   - **A note with five languages at launch, 8 KiB each:** every query is
     ready. Four tasks take five blocks, so one task takes two: about 1.5 ms,
     derived from the rate and not measured as a whole. Without the builds
     at process start, the task would first build five queries: 23.7 ms one
     by one, or about 11 ms in parallel.

   A block over 16 KiB that appears while editing goes to D6's background
   pass: a new or pasted block, a changed info word, or an evicted tree. It
   shows its shifted old tokens or `md_code_block` for one to three frames.
   A pasted 256 KiB block takes about 23 ms.

9. **The label is unchanged.** It shows the info word's source bytes
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
| TypeScript | `tree-sitter-typescript` | `typescript`, `ts` |
| JSON | `tree-sitter-json` | `json`, `jsonc` |
| Bash | `tree-sitter-bash` | `bash`, `sh`, `shell`, `zsh` |
| HTML | `tree-sitter-html` | `html`, `htm` |
| CSS | `tree-sitter-css` | `css` |

`jsx` uses `tree-sitter-javascript`, whose grammar parses JSX, with
`HIGHLIGHT_QUERY` and `JSX_HIGHLIGHT_QUERY` joined. `tsx` keeps
`md_code_block`: `LANGUAGE_TSX` adds 1,454,448 B (Binary size by grammar).
Zed parses JavaScript with the tsx grammar
(`grammars/src/javascript/config.toml:2`); Deviations records why Inkra does
not. TypeScript's query extends JavaScript's; the spike concatenated both, and
the build does the same.

**Adding a language** is one dependency in `inkra-highlight`'s manifest and
one table row: the `LanguageFn`, the highlights query and the info words. Most
grammar crates export `HIGHLIGHTS_QUERY`; Bash and JavaScript export
`HIGHLIGHT_QUERY`. A capture the class table lacks keeps `md_code_block`, so a
new grammar degrades to grey, never to a wrong colour. CI needs a C compiler
for each grammar; macOS runners have one.

## Palette

Every colour is a Flexoki name: a base shade, or the 400 shade on dark and the
600 shade on light. A hex value is given only where `builtin.rs` already uses
it. S1 copies the other values from the Flexoki palette and checks the scene.

| Class | Dark | Light |
|---|---|---|
| `Comment` | base-500, `#878580` | base-500, `#878580` |
| `Keyword` | green-400 | green-600 |
| `String` | cyan-400 | cyan-600 |
| `Constant` | purple-400 | purple-600 |
| `Function` | orange-400 | orange-600, `#BC5215` |
| `Type` | yellow-400 | yellow-600, `#AD8301` |
| `Property` | blue-400 | blue-600, `#205EA6` |
| `Tag` | red-400 | red-600, `#AF3029` |
| `Attribute` | magenta-400 | magenta-600 |
| `Punctuation` | base-300, `#B7B5AC` | base-600, `#6F6E69` |
| `Operator` | base-300, `#B7B5AC` | base-600, `#6F6E69` |

Text without a token keeps `md_code_block`: `#CECDC3` on dark, `#403E3C` on
light. Comments and punctuation sit between it and the background, so the
coloured classes carry the reading. The dark built-in's accents are not
Flexoki today; the syntax tokens do not depend on them.

## Cost

| Figure | Value | Command |
|---|---|---|
| Binary size | +5,215,184 B (+27 %) for eight grammars without TSX; TypeScript 1.41 MB, Bash 1.37 MB, Rust 1.12 MB | `stat -f %z target/…/release/inkra-app` on `spike/highlight-tree-sitter` and on `main`; Binary size by grammar |
| Rebuild of `inkra-app` | no change, 8 s | `cargo clean -p inkra-app --release && cargo build --release -p inkra-app` |
| Clean build of the tree-sitter crates | 11 s release, 4 s debug; slowest grammar 2.70 s in release | Clean build, above |
| Keystroke outside a block | 83 ns for 51 blocks, no parse | `spike_incremental`, `spike_recognise_unchanged` |
| Keystroke inside a block of at most 64 KiB | ≤ 0.66 ms in every case, on the main thread; 71 µs for Rust at 500 lines | `spike_incremental`, `spike_large_steps` and `spike_incremental_cost` |
| Keystroke inside a block over 64 KiB | ≤ 17 µs plus a ≤ 34 µs copy on the main thread; the reparse in the background, 0.5 to 13.4 ms up to 512 KiB | `spike_incremental`, `spike_large_steps`; Copying a block |
| First pass at the 16 KiB limit | 1.50 to 1.59 ms on the main thread | `spike_incremental`, `spike_first_pass_sizes` |
| First pass above the limit | background; about 23 ms at 256 KiB | `spike_incremental`, `spike_first_pass_sizes` |
| Queries, at process start | 11 ms wall and 24 ms CPU in the background; 373 KiB live; RSS +15 MiB, probably freed build memory | `spike_incremental`, `spike_query_builds` and `spike_query_memory` |
| Opening a note | the landing waits at most 8 ms by D8's caps: 0.8 ms for 8 KiB of code; 5.6 ms for the 51-block note's small blocks | `spike_incremental`, `spike_first_pass_sizes` and `spike_parallel_first_pass` |
| Tree memory | about 110 B per node, 26 to 41 times the text; 16 MiB budget | `spike_incremental`, `spike_tree_memory` |
| Launch | no main-thread work before the first frame, by construction (D8) | reading `open_note` and the restore; no run |
| Launch to text, and the larger binary at launch | not measured here; S1, S2 and S3 each block on the maintainer's figure | Slices |

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
  sorted, disjoint, inside the block and on char boundaries. After random
  edits, one or several splices per step, the incremental path gives the
  same tokens as a full pass whenever the full pass's tree has no error.
  The spike's Bash divergence shows why the condition is needed.
- `inkra-highlight`, `proptest`, repairs: the generator breaks then repairs.
  It splices then reverts, deletes a delimiter then retypes it, and opens a
  quote then closes it. Every clean step must equal a full pass, and so must
  the final step.
- `inkra-text`, `proptest`: replaying `edits_since(mark)` on the old text
  gives the new text, across `edit`, `undo`, `redo` and `apply_diff`. The
  mark grows on undo. A mark older than the ring gives `None`.
- `app`, `#[gpui::test]`:
  - a block's runs carry the token colours, and a revealed fence stays
    `md_marker`;
  - an edit outside every block highlights nothing again, counted by a
    test-only counter on the highlighter;
  - an edit inside one block updates that block only, incrementally;
  - a typed fence, a deleted fence or a changed info word gives a first pass;
  - a block over 16 KiB shows its old tokens until the background pass lands;
  - an edit in a block over 64 KiB shifts its tokens on the main thread, and
    the background reparse lands with the edits typed meanwhile reapplied;
  - an open waits on the queries that the process start builds, and builds
    none itself;
  - an opened note's first frame with text already has its colours;
  - an evicted tree keeps its tokens and gets a first pass on the next edit;
  - a vault theme without `syntax_*` keys shows the built-in colours.
- Scene `code-highlighting`, dark and light: one block per language, an
  unknown word, no info word, an indented block. Look at it after each slice.
- `cargo mutants` on `inkra-highlight`: zero missed.

## Slices

Each slice is vertical: crate, type, cache, theme, test, scene.

1. **S1, Rust end to end.** The splice log in `inkra-text`, `inkra-highlight`
   with one grammar, `TokenClass`, the class table, `Highlights` beside
   `Parses` with the incremental path, the eleven theme tokens, `line_runs`
   reading tokens, the scene. First passes run on the main thread within D6's
   budget. A block over 16 KiB stays grey. The Rust query builds at process
   start (D8), but the open does not wait yet, so S1 can show grey before
   colour.
   - Blocking criterion: the release binary grows by at most 1.5 MB against
     `main`, by `stat -f %z`. The spike predicts about 1.29 MB, the core plus
     Rust. The window oracle's median stays within base + 5 ms over 10
     interleaved rounds (`docs/startup-floor.md`).
   - STOP rule: if either figure fails in two separate batches, stop the
     slice and report the figures before S2.
2. **S2, colour on open and large blocks.** D8's step in the open's task, the
   wait on the process-start queries, the background first pass over 16 KiB,
   the background reparse over 64 KiB, the tree budget. The tests that an
   opened note lands coloured and that a large block keeps its old tokens.
   - The slice adds a `text` mark to the `launch:` line
     (`crates/app/src/timeline.rs`): the moment the restored note's text
     lands. No mark records it today.
   - Fixture: a scratch HOME and a vault whose session holds two open notes.
     One is an 8 KiB Rust note in 4 blocks; the other has one block in each
     of five languages.
   - Method: `scripts/launch-figure.sh`, base `main` against S2, 10
     interleaved rounds.
   - Blocking criterion: the `text` median stays within base + 8 ms, the
     `first frame` median within base + 5 ms, and the phys_footprint median
     within base + 4 MiB.
   - STOP rule: if a threshold fails in two batches, try fallback 1: build
     the queries in the open's task instead of at process start. If it still
     fails, try fallback 2: land the text first and colour it after.
   - The maintainer runs this measure. This spec's author could not open a
     window.
3. **S3, the first set.** The seven other grammars, the info-word table, the
   per-language tests, the scene's other blocks.
   - Blocking criterion: the release binary grows by at most 5.5 MB against
     `main`; the spike measured 5,215,184 B. The window oracle's median stays
     within base + 5 ms over 10 interleaved rounds.
   - STOP rule: if either figure fails in two batches, ship the S1 set and
     reopen Open question 1. TypeScript (1.41 MB) and Bash (1.37 MB) are the
     largest grammars, so the question starts with them.

## Deferred

| Item | Trigger |
|---|---|
| TOML, YAML, Markdown and more languages | Asked for, or a fence word the set lacks becomes common in a vault |
| Injections (CSS in HTML, a language inside Markdown) | Asked for. The direct `Query` path has none |
| Highlighting inline code and front matter | Asked for. Out of this spec's scope |
| A per-language colour override in vault themes | Asked for after the eleven tokens ship |
| Persisting tokens across launches | Opening a note with much code measures over one frame later than today |
| Keeping a removed block's entry, so a redo skips its first pass | A redo of a large block shows grey long enough to notice |
| `tsx` blocks, with the TSX grammar (+1,454,448 B) | Asked for |
| Removing grammars at build time behind features | The binary size becomes a complaint |

## Open questions

1. **The language set.** Recommendation: ship the eight above, without the TSX
   grammar; `jsx` uses the JavaScript grammar. The per-grammar sizes do not
   change this answer: the eight cost 5.2 MB, and TSX would add 1.45 MB for
   one info word. S3's STOP rule reopens the question if the size fails.
   Add TOML in S2 only if the maintainer waives the rule for
   `tree-sitter-toml-ng`, last released 2024-12-03. Zed's `native_grammars()` lists no HTML
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
- **Finding the edit by common prefix and suffix.** It adds nothing to
  `inkra-text`, but stores every block twice and scans their text again on
  each keystroke (D5).
- **Recording the session's languages to build queries before the first
  frame.** Building every query at process start (D8) takes 11 ms of wall
  time in the background. The first frame comes about 100 ms after `run()`,
  so every query is ready before it. That needs no session field, and it
  also covers a language the user opens later. A session field would save
  13 ms of CPU and 0.4 MiB at most, for one more format to keep.
- **The TSX grammar for `jsx` and `tsx`.** It adds 1,454,448 B. The
  JavaScript grammar parses JSX and ships a JSX query, so only `tsx` loses
  colour (Deferred).
- **Persisting tokens across launches.** The open's background task colours a
  note before it lands, so a cache file would save nothing visible (D8).
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
- **One hull edit per block, not one `InputEdit` per buffer edit.** Zed's
  `SyntaxSnapshot::interpolate` takes exact edits from
  `anchored_edits_since`, with bytes and points (`syntax_map.rs:422-424`).
  Inkra folds a block's splices into one hull, widened to whole lines, with
  rows from the old tree's nodes. The spike measured it equal to Zed-style
  exact edits on every error-free tree.
- **Rows from the tree, not from newline counts.** Counting newlines reads
  the whole block on each keystroke. The old tree already holds the rows, so
  the path keeps no O(block) step of its own (Large blocks).
- **A 64 KiB size limit, not a 1 ms timeout.** Zed reparses on the main
  thread for at most 1 ms, then in the background (`buffer.rs:1177-1181`).
  The spike measured the cost as linear in block size. A size test picks the
  same blocks before any work, so no cancelled reparse is wasted.
- **`jsx` through the JavaScript grammar, no TSX grammar.** It saves
  1,454,448 B (Languages).
- **Queries built at process start.** Zed builds a language's queries when it
  loads that language. Inkra's eight build in 11 ms in the background, before
  the first frame, so an open never waits for one (D8).
- **A splice log with a monotonic mark, not a version.** Zed's buffer versions
  are ordered; Inkra's `Version` is not, and undo returns an older one.
- **One tree per block, no layers.** Zed's `SyntaxMap` keeps layers for
  injections over the whole buffer. A note's blocks are separate texts, and
  this spec has no injections.
- **A 2 ms first-pass budget and a 16 KiB block limit.** Zed's buffer reparses
  on the main thread with a 1 ms timeout, then in the background
  (`buffer.rs:1177-1181`). Zed's markdown always highlights in a background
  task. Most note blocks are short, and the main thread colours them in the
  same frame.
- **Colour computed in the open's background task.** Zed opens a buffer and
  parses it afterwards. Inkra already loads off the main thread, so the note
  can land coloured.
- **A tree budget beside the text bounds.** Zed's markdown cache keeps tokens,
  not trees. Inkra keeps trees for the incremental path, and a tree costs 26
  to 41 times its text.
- **The cache is per buffer, not one process-wide store.** It follows
  `Parses`, so a closed note frees its entries. The text bounds are Zed's.
- **Entries found by splice ranges, not by a text hash.** Zed keys its cache
  by an `FxHasher` hash of the text (`highlight_cache.rs:20-36`). A hash reads
  every block on each keystroke; `DefaultHasher` took 47 µs where the splice
  walk took 83 ns. `FxHasher` is faster but was not measured.
- **No HTML exclusion.** Zed lists no HTML grammar; Inkra ships one (Open
  questions, 1).
