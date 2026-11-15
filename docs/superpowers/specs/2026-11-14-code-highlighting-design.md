# Code highlighting: our own lexers on the decoration layer

Status: **proposed** 2026-11-14. Replaces the engine choice of
`2026-11-11-code-highlighting-design.md` (tree-sitter), which stays as the
measured evidence for the fallback (D14). Builds on the decoration layer
(`2026-11-13-decoration-layer-design.md`) and the feature registry
(`2026-11-12-feature-registry-design.md`). Read their D3 to D8 and D12 first.

## Why

A fenced block today has one colour for every byte: `md_code_block`. A note
taker pastes code to read it later, and uniform grey code is slow to read.

The 2026-11-11 spec picked tree-sitter. On 2026-11-14 the owner chose to write
our own highlighter instead. The decision weighed these points:

- tree-sitter adds 5,215,184 B (+27 %) to the release binary for eight
  grammars, and a C compiler to the build;
- it needs a waiver of the 12-month rule for `streaming-iterator`, on every
  tree-sitter path, and for six of eight upstream grammar crates;
- a hand-written lexer is pure Rust, tested like `inkra-text`, with no
  dependency and no waiver;
- for reading a note, a lexer gives the colours that carry the reading.
  What it cannot give needs scope analysis, which Zed itself gets from a
  language server, not from tree-sitter.

What a lexer loses is listed in D4. D14 keeps tree-sitter as a fallback, one
language at a time, behind the same output.

## Terms

- **lexer**: the function that turns one language's code into tokens, one
  line at a time.
- **token**: a byte range and its `TokenClass`.
- **line state**: what is still open at a line's end: a block comment, a
  string, a heredoc. The lexer takes it in and hands it out per line.
- **content line**: the code bytes of one block line, without fence, quote
  prefix or list indent.
- **block chunk**: the decoration layer's chunk for one code block, its whole
  range, fences included.

Chunk, publish, splice and edit mark are the decoration layer's terms.

## Scope

In scope:

- the `inkra-highlight` crate: the lexers, `TokenClass`, the language table;
- the `highlighting` built-in feature: one layer, its listener, its open
  function;
- one read added to the decoration layer: the chunks a layer holds (D9);
- eleven theme tokens;
- the first set of eight languages.

Out of scope:

- highlighting inline code and front matter;
- languages described as data by the user (Deferred);
- scope-based colouring: parameters, fields, locals (D4);
- any style other than colour (decoration layer, D2).

## Design

### D1. A new crate, `inkra-highlight`, holds the lexers

It holds the lexers, D7's `plan` and the listener's pure part (deviation 9).
It depends on `inkra-text`, for `ByteOffset`, and on `inkra-markdown`, for the
parse, which gives D3 its content lines; `inkra-app` depends on it. It has no
gpui dependency, no clock and no external dependency of its own. Its tests
also use `inkra-editor`, as a dev-dependency (D12).

`inkra-markdown` would compile every lexer into `inkra-index` and its benches,
against "one role per crate". `inkra-app` would put a `Window`-free algorithm
where its tests need gpui. Both objections still hold with the edge to
`inkra-markdown`: the edge points from `inkra-highlight`, so neither
`inkra-markdown` nor `inkra-index` compiles a lexer. The build adds the crate
to `CLAUDE.md` (Architecture and the mutants line), to `allowed_edges` in
`scripts/check.sh`, and to `examine_globs` and `test_package` in
`.cargo/mutants.toml`.

### D2. The output is tokens on content lines

The crate's one entry point:

```rust
pub fn highlight(
    language: Language,
    lines: impl Iterator<Item = (ByteOffset, &str)>,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
);
```

`lines` yields each content line with its start offset, in order. The lexer
carries the line state from one line to the next. Tokens are absolute buffer
offsets, sorted, disjoint, on char boundaries, and inside a content line. So a
token never covers a quote prefix, a list indent or a fence.

A lexer is a function `fn(LineState, &str, ByteOffset, &mut Vec<…>) ->
LineState`. The language table holds one lexer per language as a function
pointer. There is no trait: a table of functions is the simpler shape for many
implementations of one signature.

`LineState` is a `Copy + Eq` value of fixed size, and it carries everything a
lexer reads across a line end. Nothing resets at a line start, so a generic
list, a template or a JSX element can span lines. It holds:

- what is open: a block comment and its depth, a string and its delimiter, a
  raw string's hash count;
- Bash's pending heredocs, a queue of at most two: each tag as a hash, and
  whether `<<-` strips leading tabs. `cat <<A <<B` reads A's body, then B's. A
  third pending heredoc is not queued, and its body colours as commands;
- the class of the previous significant token, for D4's look-behind;
- a nesting stack of at most 16 entries, one byte each: a bracket, a generic
  list, a template's `${`, a JSX element, an embedded language (HTML).

A nesting deeper than 16 stops pushing, and its closers stop popping. Past
that depth the colours degrade; the lexer never panics. CodeMirror 5 keeps
one copyable state per line for the same reason
(`codemirror.net/5/doc/manual.html`, `copyState`).

`Language` is a `Copy` enum. `Language::from_info(word)` resolves a fence info
word case-insensitively against each language's names (Languages). An unknown
word gives `None`. Zed matches the same way (`available_languages.rs:218`).

`TokenClass` is a `Copy` enum of eleven classes (D5). Text without a token
keeps `md_code_block`.

### D3. Content lines come from the markdown parse

A block's content lines are one per physical line between its fences, in
order. A line's content is its span in `Markdown.spans` that carries
`Styles::CODE_BLOCK` and not `Styles::MARKER`. Spans never cross a `\n`
(`parse.rs:13`), and `push` merges touching spans with equal styles, so a line
has at most one such span. A line with no such span, an empty one, is still
yielded, as an empty content line at its line end. Bash needs it: an empty
line ends a heredoc whose tag is empty.

pulldown-cmark hands the code text without container prefixes, so a block in
a quote or a list lexes as the code alone. Tests pin a block in a quote, one
in a list item, and an empty line inside each.

The language comes from `CodeBlock.language`, as the label reads it
(code-blocks spec, D1). An indented block has no language and keeps
`md_code_block`.

### D4. What a lexer colours, and what it does not

Every lexer is a single pass with a one-token look-behind (the previous
significant token) and a small bracket stack. That covers:

| Class | Rule |
|---|---|
| `Comment` | the language's comment syntax, block comments across lines |
| `String` | the language's string syntax, escapes included; raw strings, template strings, heredocs |
| `Constant` | numbers; `true`, `false`, `null`, `None`, `undefined` and the like; an all-caps identifier |
| `Keyword` | the language's keyword list |
| `Function` | an identifier followed by `(` on the same line, past a Rust turbofish `::<…>` that closes on that line; an identifier after a definition keyword (`fn`, `def`, `function`); a Rust macro name before `!` |
| `Type` | an identifier after a type keyword (`struct`, `enum`, `trait`, `type`, `class`, `interface`, `impl`); a capitalised identifier in Rust, TypeScript and Python; a built-in type (`i32`, `str`, `number`, `string`); an identifier inside a generic list |
| `Property` | an identifier after `.` and not followed by `(`; a JSON object key; a CSS property name |
| `Attribute` | a Rust `#[…]`, a Python `@decorator`, an HTML attribute name |
| `Tag` | an HTML or JSX tag name |
| `Punctuation` | brackets, `,`, `;`, `:` |
| `Operator` | the language's operator characters |

Two rules use the look-behind to settle classic ambiguities:

- **Expression start.** After a value end, a `/` is a division and a `<` is a
  comparison. Anywhere else, `/` opens a regex literal (JavaScript,
  TypeScript) and `<` followed by a letter opens a JSX tag where JSX is on
  (Languages). A value end is an identifier, a number, a string, `]`, a
  postfix `++` or `--`, or a `)`, except a `)` that closes the condition of
  `if`, `while`, `for` or `with`; the nesting stack marks that `(`. A keyword
  that takes an expression (`return`, `typeof`, `case`, `yield`, `await` and
  the like) is an expression start. So `return <div>` opens a tag, `if (ok)
  /x/.test(s)` a regex, and `x++ / y` is a division. Tests pin all three.
- **Rust lifetime.** A `'` followed by an identifier and no closing `'` right
  after one char is a lifetime, coloured `Type`. Otherwise it opens a char
  literal.
- **Generic list.** A `<` right after a type keyword's name, after `impl`, after
  `::`, or after a capitalised identifier opens a generic list. Identifiers
  inside it are `Type` until the matching `>`, except a path segment before
  `::`: it takes the rules outside the list, so `std` in `Box<dyn
  std::error::Error>` keeps the plain colour and `T` in `T::Item` stays `Type`.

**What a lexer does not do.** It does not know scopes. So a parameter used in
the body, a field against a local variable, or a type declared elsewhere and
written in lower case keep the plain colour. Zed gets these from a language
server's semantic tokens. They are out of reach for a note app, and a parser
per language would only redo tree-sitter.

**Malformed code.** A lexer is local: an error mis-colours its own token only.
An unclosed string or block comment colours the rest of the block, as it does
in tree-sitter and in every editor.

### D5. Eleven classes, eleven theme tokens

| Class | Token |
|---|---|
| `Comment` | `syntax_comment` |
| `Keyword` | `syntax_keyword` |
| `String` | `syntax_string` |
| `Constant` | `syntax_constant` |
| `Function` | `syntax_function` |
| `Type` | `syntax_type` |
| `Property` | `syntax_property` |
| `Tag` | `syntax_tag` |
| `Attribute` | `syntax_attribute` |
| `Punctuation` | `syntax_punctuation` |
| `Operator` | `syntax_operator` |

The classes are the 2026-11-11 spec's, so its palette (below) and its capture
table stay valid for the fallback (D14). The tokens join `tokens!` in
`theme/colors.rs` as `name => Variant` pairs, after `md_code_block_background`.
`ThemeColors::NAMES.len()` moves from 30 to 41 (`theme/file/tests.rs:9`).

A vault theme starts from the built-in of its appearance (`theme/file.rs`), so
a theme without these keys shows the built-in colours. The mapping from
`TokenClass` to `ColorToken` lives in `app`; the crate knows no colour.

### D6. One built-in feature, one layer

`highlighting::MANIFEST` joins `BUILTIN` after `backlinks`:

- `id: FeatureId::new("highlighting")`, `default_on: true`;
- `after_first_frame: true`, as decoration layer D8 requires for a feature
  with layers;
- `layers: &[SYNTAX_DECL]`, one layer named `syntax`, with an open function;
- no command, no pane, no button.

`activate` calls `on_text_change(SYNTAX, listener)`. The feature glue lives in
`app/src/highlighting/`. The registry's switch turns it off and on; the layer
clears and recomputes by decoration layer D10.

Every code block is one block chunk, coloured or not. A block with no
language, an unknown one, or over 512 KiB (D10) gets an empty block chunk. A
chunk that no longer is a block is published empty (D7) and stays, with no
marks. So after the listener, and once its background task lands, the layer
holds:

- for each current code block, one chunk of exactly its range;
- elsewhere, only chunks with no marks.

D7's test pins this invariant: the layer's marks equal a fresh colouring of
the current text.

Once highlighting publishes, these `dead_code` expectations stop firing, and
the build removes them: in `decorations/mod.rs`, in `decorations/listen.rs`,
and on `Activation::on_text_change` (`shell/features/activation.rs`).
`decorations/mod.rs` also re-exports `Opened` outside `cfg(test)`, since the
open function returns it. An expectation that stops firing fails the build
anyway.

### D7. A pure plan decides what to clear and what to lex

On each `TextChange`, the listener computes a dirty range:

- `Edited(splices)`: the splices folded into one range, in the current text's
  offsets;
- `Lost`, and `Opened(None)` (D8): the whole text;
- `Opened(Some(_))` (D8): no range.

It then calls `batch` (deviation 9), which calls one pure function, in
`inkra-highlight` (`plan.rs`):

```rust
pub(crate) fn plan(
    blocks: &[(Range<ByteOffset>, u64)],
    held: &[(Range<ByteOffset>, Option<u64>)],
    dirty: Option<Range<ByteOffset>>,
) -> Plan;
```

`blocks` are the current text's code blocks, from
`parses::parsed(buffer).code_blocks`. Each comes with its shape
(`CodeBlock::shape`): a hash of what the lex reads besides the block's bytes.
That is the range of its language word and of each content span, both relative
to the block's start. A container can turn a block from fenced into indented
code, or back, and leave its range as it was. The line that decides it can lie
far above the block: the outermost item's marker, a fence that swallows an
inner marker, or a line between the inner marker and the block that ends both
items. No run of lines before the block catches every case, so the plan
compares what the lex reads instead. The parse computes each shape after
`cut` from the lines `content_lines` yields, so the content-line rule has one
source. That splits each block's bytes on `\n`, a second pass over code text.
`cargo bench -p inkra-markdown` measured `parse` at 2.38–2.46 ms before and
2.41–2.44 ms after on 500 KB, and 24.99–25.51 ms before and 25.52–26.02 ms
after on 4 MB. The parse is cached
per version and the draw needs it too, so this adds no parse. `held` are the
layer's chunks, each with its tag (deviation 7): the shape it was published
with, or `None` once a splice changed its bytes or a publish cut it.
`Plan` holds two sorted lists:

1. **clear**: each held chunk whose range is no block's range, marks or not
   (deviation 6); the listener removes it. It clears a block that a removed or typed fence turned into prose, or that
   merged with a neighbour.
2. **lex**: each block that no held chunk has exactly, or that the dirty
   range meets. A chunk has a block exactly when its range is the block's
   range and its tag is the block's shape. A chunk on the block's range with
   another tag stays, and the lexed block replaces it. Meets counts touching:
   `dirty.start <= block.end && block.start <= dirty.end`. So a deletion
   right before the fence's line relexes the block, even when the core mapped
   an old chunk onto exactly the block's range. A deletion in the fence's
   indent or quote prefix that leaves each content line's code where it was
   changes neither the range nor the shape, and no colour.

The listener publishes every clear range empty first, then the lexed blocks.
At one edit mark the later publish wins (`layer.rs`, `publish`), so a lexed
block replaces every clear it overlaps. The reverse order would erase a
merged block right after colouring it.

A changed info word and a typed or deleted fence are caught too: the fence
line lies inside the block chunk, so the splice meets the block. The plan is
one merge of three sorted lists and reads no text: its cost grows with the
number of blocks and chunks, not with the text. A keystroke in prose lexes
nothing, in a list item too.

The listener lexes the blocks of **lex** on the main thread, in order, while
their total size stays within `MAIN_THREAD_BUDGET` bytes (D10). The rest goes
to the buffer's background task (D10). So one listener call is bounded,
whether it follows a keystroke, a `Lost`, an open or an `apply_diff` reload
whose splices span the note.

### D8. A note lands with its colours

The layer's open function, `fn(&str, Instant) -> Opened`, runs in the open's
background task (decoration layer D8). It runs `inkra_markdown::parse` on the
text, then lexes the blocks in document order until the deadline. The parse
cannot stop at the deadline, so a text over `OPEN_PARSE_LIMIT` bytes skips it:
the open function returns no chunk and the seed, and D7 colours the note from
the main thread and the background task. The parse at the limit takes at most
4 ms, half the landing deadline. `cargo bench -p inkra-markdown --bench parse`
measured these medians on 2026-11-15:

| Bytes     | `parse` median |
| --------- | -------------- |
| 500 000   | 2.63 ms        |
| 1 000 000 | 5.65 ms        |
| 2 000 000 | 14.29 ms       |
| 4 000 000 | 28.78 ms       |

So `OPEN_PARSE_LIMIT` is 500 000 bytes, the largest measured size within
4 ms. A block over
`MAIN_THREAD_BUDGET` is skipped there, so one large block cannot spend the
whole deadline. Every block it reaches gets its block chunk, coloured or
empty.

`Opened.chunks` holds the finished block chunks; the core lands them at the
buffer's edit mark (`decorations/open.rs`, `seed`). `Opened.seed` is a unit
value, `Box::new(())`, and carries no data: the listener reads the landed
chunks back through D9. The core hands the seed over only if the text is still
at that edit mark (`Heard::hear`). So:

- `Opened(Some(_))`: the landed chunks are current. D7 runs with no dirty
  range, which lexes only the blocks the open did not reach.
- `Opened(None)`: the text moved before the first hearing, and no splice says
  where. The landed chunks may be stale, and some may lie on prose now. D7
  runs with the whole text dirty, which clears them and relexes every block.
  This happens to a restored inactive tab: it hears its changes only when a
  view binds it (decoration layer, deviation 3).

Decoration layer deviation 7 holds: a note launched alone has no vault, so it
gets no colours.

### D9. The decoration layer gains one read: the chunks a layer holds

```rust
pub(crate) fn chunks(layer: LayerId, buffer: &Entity<Buffer>, cx: &App) -> Vec<(Range<ByteOffset>, Option<u64>)>;
```

It returns the layer's chunk ranges over `buffer`, sorted, each with its tag
(deviation 7). `sync` catches every layer up before it calls a listener
(`decorations/mod.rs`, `sync`), so inside the listener the ranges are in the
current text's offsets. It needs a `Layer::chunks()` iterator in
`inkra-editor`. The core already maps chunks through every splice (decoration
layer D3, deviation 5), so the feature keeps no block list of its own and maps
nothing. This follows decoration layer D3: "the core shifts the marks; the
feature does nothing".

The registry rule for contract growth holds: the read adds nothing to
`Activation`, and no `Manifest` field changes.

### D10. What the main thread does not lex, one background task lexes

`MAIN_THREAD_BUDGET` is the number of block bytes one listener call lexes on
the main thread. A call at the budget takes at most 0.5 ms. The keystroke
budget is 1 ms; the listener's 2 ms log line is the decoration layer's (D11).

`cargo bench -p inkra-highlight --bench listener` measures the listener's pure
part, lex and publish together (deviation 5), in two worse cases: one block of
the size, and 1,000 blocks of 512 bytes with the first blocks that add up to the
size dirty. It measured these medians on 2026-11-15:

| Bytes   | One block | Many blocks |
| ------- | --------- | ----------- |
| 4 KiB   | 15.0 us   | 28.6 us     |
| 8 KiB   | 36.0 us   | 49.8 us     |
| 16 KiB  | 74.7 us   | 96.2 us     |
| 32 KiB  | 154.7 us  | 187.5 us    |
| 64 KiB  | 313.7 us  | 366.8 us    |
| 128 KiB | 638.9 us  | 719.5 us    |

The budget is the largest size where the worse of the two is at most 0.5 ms:
64 KiB, at 366.8 us. It is also the 2026-11-11 spec's main-thread bound. The
budget is not under 16 KiB, so the partial relex of the Deferred table stays
deferred. Until S2, nothing enforces the budget: every lexed block runs on the
main thread.

**One background task per buffer.** The feature keeps a map from a buffer to
its task in flight; an entry leaves the map when its task lands. It holds no
block list. When D7 leaves blocks unlexed:

- with no task in flight, the listener copies those blocks' content lines,
  reads the buffer's edit mark, and spawns one task that lexes them;
- with a task in flight, it does nothing more.

A task lexes its blocks in order and stops before the one that would take its
marks past `PUBLISH_BUDGET`; the first block always goes. The landing's
publish runs on the main thread and costs time per mark (`layer.rs`,
`publish`), so this bounds it. S2 sets the budget so a landing's publish takes
at most 0.5 ms. The blocks left go to the next task through the replan below.

When the task lands, it publishes each block at the edit mark read before the
copy. The core maps the late publish, and deviation 15 keeps any chunk
published since. Then it runs D7 once more, with the edits since that mark as
the dirty range, or the whole text if the log dropped them. That run lexes
what changed during the task and spawns the next task if needed. Fast typing
in a large block therefore costs one lex in flight per buffer, and the colours
catch up when typing stops.

Until a task lands, its blocks show their shifted old marks, and typed bytes
show `md_code_block`.

A block over 512 KiB is not lexed; it gets an empty block chunk (D6). The
bound is the 2026-11-11 spec's, which Zed's highlight cache also uses
(`highlight_cache.rs:14-16`). A mark is `(Range<ByteOffset>, MarkStyle)`, 24
bytes, so marks cost about 3.4 times the block text.

### D11. The label and the fence are unchanged

The label shows the info word's source bytes (code-blocks spec, D1).
`MARKER` wins over every layer (decoration layer D7), so a revealed fence stays
dimmed.

### D12. Each lexer and the plan are tested like `inkra-text`

**Lexers.** Each language gets known inputs with known tokens, one test per
rule of D4 that applies to it, and the ambiguities each lexer settles. A
shared `proptest` runs every lexer on any text, ASCII and Unicode, split into
lines at random. Tokens are sorted, disjoint, inside their content line and on
char boundaries, and the lexer never panics.

**The plan, against the real core.** A model test in `inkra-highlight` drives
a `Buffer`, a `Layer` and `inkra_markdown::parse` through random edits on a
note with several blocks. The edits favour backticks, newlines, fence lines,
fence indents, info words, quote prefixes and undo. After each edit, it runs D7 and publishes
the plan. Then the layer's marks must equal a fresh colouring of the current
text. The test needs `inkra-editor` as a dev-dependency of
`inkra-highlight`; `inkra-markdown` is a dependency (D1, deviation 9). Both
edges join `allowed_edges` in `scripts/check.sh`, which counts every
dependency kind, and CLAUDE.md's Architecture.

`cargo mutants` on `inkra-highlight`: zero missed.

### D13. Adding a language

Adding a language is one lexer module in `inkra-highlight`, one row in the
language table, and its tests. A language that most lexer rules cover with
lists (keywords, comment and string syntax) could become data instead of
code; that is Deferred.

### D14. tree-sitter stays a fallback, one language at a time

The seam is the output: `(Range<ByteOffset>, TokenClass)` per content line.
If a language's lexer proves too weak, that language alone can switch to a
tree-sitter grammar. The `arborium-*` grammar crates (2.18.2, 2026-08-28, MIT)
export `language() -> LanguageFn` and `HIGHLIGHTS_QUERY` at ABI 15, and their
capture names match D5's classes through the 2026-11-11 spec's capture table.
Nothing in `app` or in the theme changes for such a switch.

No engine trait is built ahead of that day: there is one engine.

## Languages

**The first set is Rust, Python, JavaScript, TypeScript, JSON, Bash, HTML and
CSS.** The repo's own `docs/` use `rust` most, then `bash`, then `toml`
(2026-11-11 spec, Languages). Open question 1 asks about TOML.

| Language | Info words |
|---|---|
| Rust | `rust`, `rs` |
| Python | `python`, `py` |
| JavaScript | `javascript`, `js`, `mjs`, `cjs`, `jsx` |
| TypeScript | `typescript`, `ts` |
| TSX | `tsx` |
| JSON | `json`, `jsonc` |
| Bash | `bash`, `sh`, `shell`, `zsh` |
| HTML | `html`, `htm` |
| CSS | `css` |

JSX tags come from D4's expression-start rule, under a JSX flag the language
sets. JavaScript has it on, for every info word: plain JavaScript has no `<`
in expression position that JSX would take. TypeScript has it off, because
`<T>(x: T) => x` is a generic there; TypeScript itself forbids that form in
`.tsx` files for this reason. TSX is TypeScript with the flag on, so it needs
no lexer of its own. TypeScript's lexer extends JavaScript's with type
keywords and the generic list rule. HTML hands `<style>` content to the CSS
lexer and `<script>` content to the JavaScript lexer, with the line state
recording which one is open.

## Palette

Unchanged from the 2026-11-11 spec. Every colour is a Flexoki name: a base
shade, or the 400 shade on dark and the 600 shade on light.

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
light. The look is tuned after it ships, as for code blocks.

## Cost

| Figure | Expected | How it is checked |
|---|---|---|
| Binary size | a few hundred KB for eight lexers, mostly keyword tables; S1a (Rust only): `main` 19,364,544 B, S1a 19,360,464 B, difference -4,080 B | `stat -f %z` on the release binary, `main` against each slice |
| Lex, Rust | S1a medians: 1.71 us for 20 lines (923 B), 54.7 us for 500 lines (16.1 KB), 646 us for 5,000 lines (169 KB) | `cargo bench -p inkra-highlight -- --quick`, the texts of the 2026-11-11 tree-sitter figures |
| Build time | no C, no build script | clean build of `inkra-highlight` |
| Keystroke outside a block | D7's merge; no lex | `app` planted-marks test |
| One listener call | at most `MAIN_THREAD_BUDGET` bytes lexed and published, at most 0.5 ms by D10's definition; S1b: 64 KiB in 313.7 us (one block) and 366.8 us (many blocks) | `cargo bench -p inkra-highlight --bench listener`, D10 |
| Opening a note | the lex shares decoration layer D8's 8 ms deadline | the open-time test; the launch figure |
| Memory | marks only, no trees: about 3.4 times the block text (D10) | — |

## Tests

- `inkra-highlight`: D12's tests, per language, shared and the model test.
- `inkra-highlight`, `#[test]`: every info word of the Languages table
  resolves, in any case; an unknown word resolves to `None`.
- `inkra-highlight`, `#[test]` on `plan`, one per case: deleting the closing
  fence between two blocks clears both old chunks and lexes the merged block;
  deleting a line right before a block's opening fence lexes the block;
  a chunk on a block's range with another tag, or none, is lexed over.
- `inkra-highlight`, `#[test]` on `open_chunks` (D8): every block before the
  deadline, none past it, none over `OPEN_PARSE_LIMIT`, all at the limit.
- `inkra-highlight`, Bash: `cat <<''` ended by an empty line; `cat <<A <<B`
  with both bodies; `<<-` with tab-indented tags.
- `inkra-editor`: `Layer::chunks` returns the mapped chunk ranges, sorted,
  each with its tag, after random edits and publishes
  (`proptest`, beside the layer's own).
- `app`, headless `#[gpui::test]` against `MemFs`:
  - a block's runs carry the token colours; a revealed fence stays
    `md_marker`;
  - a keystroke in prose lexes nothing: every block keeps its planted marks;
  - a keystroke in one block lexes that block only;
  - a typed fence, a deleted fence and a changed info word recolour the right
    blocks, and leave no mark on prose;
  - a block in a quote and one in a list item colour their code only;
  - an opened note's first frame with text has its colours;
  - a note edited before its first hearing (`Opened(None)`) loses the marks
    its open landed on prose;
  - a listener call lexes at most `MAIN_THREAD_BUDGET` bytes, and the rest
    lands from the background task;
  - a block over the budget shows its old marks until the task lands; edits
    during the task start one more task when it lands, never two at once;
  - switching the feature off removes the colours on the next frame; on
    restores them;
  - a vault theme without `syntax_*` keys shows the built-in colours.
- Scene `code-highlighting`, dark and light: one block per language, an
  unknown word, no info word, an indented block, a block in a quote. Look at
  it after each slice.

## Slices

Each slice is vertical and ends green on `./scripts/check.sh`.

1. **S1a, Rust on screen.** `inkra-highlight` with the Rust lexer,
   `Language`, `TokenClass` and the lexer tests; the eleven theme tokens; the
   feature with a listener that, on every change, lexes the whole note and
   publishes it as one chunk over the whole text; the scene with its Rust
   block. No D7, D8 or D9 yet, so a keystroke lexes every block.
   - Measure: `cargo bench -p inkra-highlight` on Rust texts of 20, 500 and
     5,000 lines from this repo, and the binary size against `main`.
   - Shot comparison: the scene's Rust block next to the same block on
     `spike/highlight-tree-sitter`. The owner judges.
   - STOP rule: if the owner finds the lexer's shot clearly worse, stop and
     reopen D14 for Rust. Nothing of D7 to D10 is built before this verdict.
2. **S1b, block chunks.** `plan` (D7) and its tests, `Layer::chunks` and
   `decorations::chunks` (D9), the open function and the seed (D8), the main
   thread budget, the model test (D12). The whole-text listener of S1a goes.
   Bench the whole listener call and set `MAIN_THREAD_BUDGET` (D10); bench the
   parse and set `OPEN_PARSE_LIMIT` (D8). Every block over the budget still
   lexes on the main thread until S2.
3. **S2, the background task.** D10's one task per buffer, `PUBLISH_BUDGET`,
   the replan on landing, the 512 KiB bound, D8's skip of large blocks at open. The tests
   for a block over the budget.
4. **S3, Python, JSON and Bash.** Three lexers, their info words, their
   scene blocks. Bash carries the heredoc state.
5. **S4, JavaScript, TypeScript, TSX, CSS and HTML.** In this order: HTML
   embeds CSS and JavaScript. The expression-start rule, the JSX flag, regex
   literals, JSX tags, template strings.

## Deferred

| Item | Trigger |
|---|---|
| TOML, YAML, Markdown, Go, SQL and more | Asked for, or a fence word the set lacks becomes common in a vault |
| Languages described as data in the vault, for the user to add | The extensions tier asks for user-addable content |
| A partial relex from the edited line, using the line state | S1b's bench sets `MAIN_THREAD_BUDGET` under 16 KiB, so common blocks colour from the background |
| A `Definition` class, distinct from `Function` and `Type` | Asked for; the lexer already knows a definition from its keyword |
| tree-sitter for one language (D14) | A lexer proves too weak for that language |
| Highlighting inline code and front matter | Asked for |
| A per-language colour override in vault themes | Asked for after the eleven tokens ship |
| Persisting marks across launches | Opening a note with much code measures over one frame later than today |

## Open questions

1. **TOML in the first set.** It is the third fence word in this repo's docs.
   A TOML lexer is small: comments, strings, keys, tables, values.
   Recommendation: add it to S3.
2. **The `Definition` class.** Recommendation: not in the first version. The
   name at a definition and at a call share `Function`, as in the 2026-11-11
   spec's capture table.

## Rejected alternatives

- **tree-sitter with upstream grammar crates.** The 2026-11-11 spec's pick:
  +5.2 MB, a C compiler, and waivers for `streaming-iterator` and six stale
  grammar crates. Its measurements stay in that spec.
- **tree-sitter with `arborium-*` grammars.** It removes the grammar waivers,
  not the `streaming-iterator` one, the C compiler or the size. Kept as the
  per-language fallback (D14).
- **syntect with two-face.** `cargo deny` fails on bincode in every feature
  set; its full pass is 3 to 12 times slower than tree-sitter's (2026-11-11
  spec, Measurements).
- **giallo.** TextMate grammars in pure Rust, released 2026-11-04, but under
  EUPL-1.2, which `deny.toml` does not allow, and regex-based like syntect.
- **tree-house (Helix).** Its highlighter takes `ropey` text, and it brings
  the same C grammars.
- **lumis, syntastica, inkjet.** lumis pulls the stale upstream grammar
  crates and a WASM runtime; syntastica (2025-06) and inkjet (2024-09) are
  stale themselves.
- **Our own incremental parser per language.** Months per grammar, to redo
  tree-sitter less well.
- **Lexing inside `inkra_markdown::parse`.** Every keystroke would pay for
  every block, and every lexer would compile into `inkra-index`.
- **A block list kept by the feature.** It would map ranges through splices a
  second time, beside the core. D9's read keeps one source of truth.

## Deviations from Zed

- **Lexers, not tree-sitter.** Zed parses every language with tree-sitter. A
  note's code is read, not edited at scale; the size, the C build and the
  waivers outweigh the scope-free precision a parser adds (Why, D4).
- **Eleven classes, not a theme of free capture names.** A note app needs a
  short, fixed token list that a vault theme can set in full.
- **The whole block relexed per keystroke, not an incremental reparse.** Zed
  reparses incrementally because its buffers are whole files. A note's block
  is small; D10 sends the large ones to the background, as Zed's markdown
  highlights in a background task (`markdown.rs:1286`).
- **Colour computed in the open's background task.** Zed opens a buffer and
  parses it afterwards. Inkra already loads off the main thread, so the note
  lands coloured (D8).

## Deviations from this spec

The build differs from the design above in these points. Code cites them by
number.

1. **`Plan.lex` holds indices into `blocks`, not ranges.** The listener needs
   each lexed block's `CodeBlock`, for its language and its content lines. An
   index reaches it without a search; a range would need one per block.
2. **The splices fold in a pure `inkra_highlight::dirty`, beside `plan`, not
   in the listener.** The model test and the listener bench then fold the same
   way, and mutants reach the fold. A pure deletion folds to an empty range at
   its offset; `plan`'s meets counts touching, so it still relexes a block it
   touches.
3. **`content_lines` lives in `inkra-markdown`, not in `app`.** The model test
   (D12) and the listener then read the same content lines from one function.
   A copy in the test could drift from the app's.
4. **The model test emulates the core's `Heard`.** `Lost` is a clear and a
   whole dirty range, because the splice log drops edits only past a bound a
   short run never reaches. `Opened(None)` is edits, then a whole dirty range.
   `Opened(Some(_))` is a fresh layer seeded with the first k blocks, then no
   dirty range.
5. **The listener bench measures the pure part, not the gpui glue.** It runs
   `catch_up`, `chunks`, `plan`, the lex and `publish` on a `Layer` in
   `inkra-highlight`. It does not measure `parses::parsed`'s cache lookup,
   `decorations::publish`'s entry lookup and `refresh_windows`, or
   `decorations::chunks`' collect. It excludes the parse, which the draw shares
   through the cache. `app` is out of reach of a `highlight` bench. A
   `TokenClass` mark and a `MarkStyle` mark are both 24 bytes, so `publish`
   copies the same amount.
6. **A clear removes the chunk; the layer holds only block chunks.** D6 and D7
   publish a clear empty, so the chunk stays, shifts with edits, and counts as
   held. A block that comes back on exactly its range, as when `<div>` or
   `<!--` above a fence is removed, then matched it and was never lexed. An
   unknown-language block's empty chunk left on prose did the same once its
   info word changed. Each clear also added a chunk for good. So the core gains
   `Layer::remove` and `decorations::remove`, the publish path with no new
   chunk, and **clear** holds every held chunk that is no block's range. After
   the listener, the chunks are exactly the blocks' ranges, and an empty one
   means "lexed, nothing to colour". D9's read and `plan` drop the mark flag:
   no caller reads whether a chunk holds a mark any more. Deviation 7 puts a
   tag in its place.
7. **Each chunk carries an opaque tag, and D9's read returns it.** D7's first
   build gave each block a reach start, the first line whose bytes could
   change its kind, and lexed the block when the dirty range met that line. In
   a list item the reach went back to the outermost item's marker, so a
   keystroke in nested prose relexed every later block of the item. A
   narrower reach missed cases: a line between the inner marker and the block
   that ends both items, or a fence above that swallows the inner marker. So
   `Layer::publish` takes a `u64` tag and keeps it with the chunk, and
   `Layer::chunks` and `decorations::chunks` return it beside the range. The
   layer reads nothing in it. It drops the tag to `None` when a splice removes
   one of the chunk's bytes or inserts strictly inside it, and when a publish
   cuts the chunk to a piece. The syntax layer passes `CodeBlock::shape`;
   other layers pass 0. `Opened.chunks` carries the tag too, so D7 matches a
   seeded chunk the same way.
8. **One publish per listener call, a batch of clears and chunks.** Deviation
   6's listener called `decorations::remove` once per clear and
   `decorations::publish` once per lexed block. Each call repeated the rank
   lookup, the entry lookup, the catch-up, the read of the splices since its
   edit mark, the global update and the redraw. A `Lost`, an `Opened(None)` or
   an `apply_diff` reload on a note with hundreds of blocks made hundreds of
   calls. So `Layer::publish` and `decorations::publish` take a batch: the
   ranges to empty, then the chunks with their tags and marks, all at one edit
   mark. They do the lookups, the catch-up, the splice read and the redraw
   once. `Layer::remove` and `decorations::remove` go; a feature with one
   chunk passes a batch of one. The removals apply first and in order, then
   the chunks: a later change still wins, as D7 requires. Once one change of
   the batch moved marks, the later ones skip the before-and-after comparison.
   A loss drops the whole batch. The seed lands each layer's chunks as one
   batch too.
9. **The listener's pure part lives in `inkra-highlight`, which depends on
   `inkra-markdown`.** The listener, the model test (D12) and the listener
   bench each built the block list, called `plan`, read the content lines and
   lexed the blocks, in three copies. So the model test and the bench did not
   run the code that ships. `batch(text, &Markdown, held, dirty)` now returns
   the clears and the lexed block chunks, and `block_marks` lexes one block
   over its content lines. `open_chunks` takes the text and the deadline, and
   parses inside, with the same `OPEN_PARSE_LIMIT` check and the same deadline
   check before each block. `plan` is crate-private: `batch` is its one
   caller. The app keeps the dirty range of D7, the map from `TokenClass` to
   `ColorToken` and the publish. D3 already read the content lines from the
   parse, and `inkra-app` already depends on `inkra-markdown`, so the edge
   adds no crate to the app's build. D1's objections still hold.
