# Code highlighting: our own lexers on the decoration layer

Status: **proposed** 2026-11-14; D10 rewritten and deviations 10 to 12 added
for S2 on 2026-11-15. Replaces the engine choice of
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
language, an unknown one, or over 256 KiB (D10) gets an empty block chunk. A
chunk that no longer is a block is published empty (D7) and stays, with no
marks; deviation 6 removes it instead. So after the listener, and once its background task lands, the layer
holds:

- for each current code block, one chunk of exactly its range;
- elsewhere, only chunks with no marks.

D7's test pins this invariant: the layer's marks equal a fresh colouring of
the current text.

Once highlighting publishes, these `dead_code` expectations stop firing, and
the build removes them: in `decorations/mod.rs`, in `decorations/listen.rs`,
and on `Activation::on_text_change` (`shell/features/activation.rs`).
An expectation that stops firing fails the build anyway.

### D7. A pure plan decides what to clear and what to lex

On each `TextChange`, the listener computes a dirty range:

- `Edited(splices)`: the splices folded into one range, in the current text's
  offsets;
- `Lost`, and `Opened { fresh: false }` (D8): the whole text;
- `Opened { fresh: true }` (D8): no range.

Deviation 10 takes the range out of `plan`: S2a reads none, and S2b's
`batch` reads it again for D10's split.

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
   range meets; deviation 10 drops the second clause. A chunk has a block exactly when its range is the block's
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

The listener lexes on the main thread the blocks of **lex** that the dirty
range meets, in order, while they fit in `MAIN_THREAD_BUDGET` bytes (D10).
The rest goes to the buffer's background task (D10). So one listener call is bounded,
whether it follows a keystroke, a `Lost`, an open or an `apply_diff` reload
whose splices span the note.

### D8. A note lands with its colours

The layer's open function, `fn(&str, Instant) -> Vec<Chunk>`, runs in the open's
background task (decoration layer D8). It runs `inkra_markdown::parse` on the
text, then lexes the blocks in document order until the deadline. The parse
cannot stop at the deadline, so a text over `OPEN_PARSE_LIMIT` bytes skips it:
the open function returns no chunk, and D7 colours the note from
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
4 ms. A lexable block over
`MAIN_THREAD_BUDGET` gets no chunk there, so one large block cannot spend the
whole deadline; the first hearing finds it inexact and sends it to the
background task (D10). Every other block it reaches gets its block chunk,
coloured or empty (deviation 11).

The open function returns the finished block chunks; the core lands them at
the buffer's edit mark and records that mark (`HeardLayer::land`). The listener
reads the landed chunks back through D9. The first hearing is fresh only if the
text is still at that edit mark and the layer was not lost since
(`HeardLayer::hear`). So:

- `Opened { fresh: true }`: the landed chunks are current. D7 runs with no
  dirty range, which lexes only the blocks the open did not reach.
- `Opened { fresh: false }`: the text moved before the first hearing, and no splice says
  where. The landed chunks may be stale, and some may lie on prose now. Plan's
  **clear** removes those on prose (deviation 6). It relexes
  every block the edits left inexact; the core already mapped the landed
  chunks through those edits (deviation 10).
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
size held inexact: the same marks under another tag (deviation 10). The
many-blocks column was re-measured in S2a with that setup. It measured these medians on 2026-11-15:

| Bytes   | One block | Many blocks |
| ------- | --------- | ----------- |
| 4 KiB   | 15.0 us   | 29.2 us     |
| 8 KiB   | 36.0 us   | 51.8 us     |
| 16 KiB  | 74.7 us   | 99.7 us     |
| 32 KiB  | 154.7 us  | 194.4 us    |
| 64 KiB  | 313.7 us  | 378.8 us    |
| 128 KiB | 638.9 us  | 743.7 us    |

The budget is the largest size where the worse of the two is at most 0.5 ms:
64 KiB, at 366.8 us. It is also the 2026-11-11 spec's main-thread bound. The
budget is not under 16 KiB, so the partial relex of the Deferred table stays
deferred.

**What the main thread lexes.** `batch` splits D7's **lex** in three
(deviations 10 and 11):

- a block with no known language, or over `BLOCK_LIMIT` bytes, gets its empty
  block chunk at once. It costs no lex, so it counts against no budget;
- of the others, each block the dirty range meets, in document order, is
  lexed on the main thread if it fits in what is left of `MAIN_THREAD_BUDGET`;
- every other block is pending: `Batch.pending` holds its index.

A block over the budget is always pending. A dirty block that does not fit
does not stop the ones after it, as at open (D8).

**One background task per buffer.** The listener closure owns a map from a
buffer's `EntityId` to its task in flight, an `Rc<RefCell<HashMap<EntityId,
Task<()>>>>`. Zed keeps one reparse task per buffer the same way
(`language/src/buffer.rs:1943-2019`). After its publish, when `pending` is not
empty:

- with no task in flight for the buffer, the listener copies a backlog, reads
  the buffer's edit mark `at`, and spawns one task;
- with a task in flight, it does nothing more.

`backlog(text, &Markdown, &pending) -> Backlog` copies the pending blocks'
content lines, in order, with each block's range, shape and language. It stops
before the block whose range would take the backlog past `TASK_BYTES` block
bytes; the first block always goes. It counts whole ranges, fences included,
as `MAIN_THREAD_BUDGET` does, so a block with no content still counts and the
number of chunks a landing publishes is bounded too. A `Backlog` owns its bytes, holds no `Rc<Markdown>`,
and is `Send`. `lex_backlog(Backlog) -> Vec<BlockChunk>` lexes it on the
background executor. Both are pure, in `inkra-highlight`.

A mark is never empty and marks never overlap, so a block holds at most one
mark per content byte, and a block is at least one fence line long. The byte
bound on the backlog therefore bounds the landing's marks and chunks. It is checked before the lex, so no lexed block is
thrown away. A block lands exact, so each pending byte is copied once, unless
an edit changes it again.

**The landing.** On the main thread, in this order:

1. Upgrade the map's `Weak`. If it is gone, the feature went off: stop.
2. Remove the buffer's entry and release the `RefCell` borrow. A replan that
   still saw the entry would spawn nothing, and a borrow held across it would
   panic. Zed clears its task before it reparses again (`buffer.rs:2013-2015`).
3. Upgrade the buffer's `WeakEntity`. If it is gone, the note closed: stop.
4. If `edits_since(at)` holds, publish the chunks at `at`. The core maps the
   late publish, and decoration layer deviation 15 keeps every chunk published
   since. If the log dropped the splices since `at`, drop the chunks and
   publish an empty batch at the current edit mark instead. Publishing the
   chunks at `at` would clear the whole layer (`layer.rs`, `publish`), newer
   chunks included, though the layer itself still follows the log. Either
   publish catches the layer up, as D9's read requires. If the layer itself
   is behind the log, its catch-up clears it either way, and its listener
   hears `Lost` at the next sync, as any lost layer does.
5. Run `batch` on the current text with no dirty range, publish its clears and
   empty chunks, and spawn the next task if blocks are still pending.

A block that changed during the flight lost its tag in the core, so step 5
finds it inexact and sends it to the next task. A chunk that landed on prose is
a held chunk that is no block's range, so step 5 clears it. Step 5's parse is
the one the next frame draws with: `parses::parsed` caches it per version. The
landing never calls `decorations::sync`.

The task holds the buffer by `WeakEntity` and the map by `Weak`. A strong
handle to the map would keep the closure's state alive past its subscription.
Switching the feature off drops the closure, then the map, then each `Task`,
which cancels it. Fast typing in a large block costs one lex in flight per
buffer, and the colours catch up when typing stops.

Until a task lands, its blocks show their shifted old marks, and typed bytes
show `md_code_block`.

**`TASK_BYTES`.** `cargo bench -p inkra-highlight --bench landing` measures a
landing's publish: a backlog's chunks published at an older edit mark, with
200 splices since, scattered over the note, as an `apply_diff` reload during
the flight would leave. The splice log keeps 256 (`buffer/log.rs`). Three worse
cases: one block of the size; blocks of 512 bytes, both of dense code at about
one mark per byte, such as `(),;` repeated; and empty fenced blocks, the most
chunks per byte. `TASK_BYTES` is the largest size where the worst of the three
takes at most 0.5 ms. Scattered splices reach most blocks, which then land
nothing (S2b deviation 1), so as built each case also runs with the splices
in a prose paragraph above the blocks, and the worst of the six counts (S2b
deviation 10). The listener bench gains the backlog's copy at
`TASK_BYTES`; like the rest of that bench, it measures the pure part only
(deviation 5).

A block over `TASK_BYTES` lands alone, as the first block of its backlog. The
same bench measures one block at `BLOCK_LIMIT`; its landing must take at most
4 ms, half a frame at 120 Hz. Otherwise `BLOCK_LIMIT` drops to the largest
measured size within 4 ms, and this paragraph records it. A block that a
splice since reached lands nothing (S2b deviation 1), so the bound is measured
with no splice reaching the block: 200 splices in a prose paragraph above it,
or none (S2b deviation 10). One block landed in 3.31 ms at 256 KiB and in
6.35 ms at 512 KiB, so `BLOCK_LIMIT` dropped to 256 KiB.

**`BLOCK_LIMIT`.** A block over 262,144 bytes (256 KiB) is not lexed; it gets
an empty block chunk (D6). The 2026-11-11 spec's bound, 512 KiB, which Zed's
highlight cache also uses (`highlight_cache.rs:14-16`), failed the 4 ms
landing above. `MAIN_THREAD_BUDGET` must not pass it, and a `const` assertion
in `batch.rs` checks that. A mark is
`(Range<ByteOffset>, MarkStyle)`, 24 bytes, so marks cost about 3.4 times the
block text.

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
text.

From S2a every hearing runs with no dirty range, so exactness alone must
carry the invariant (deviation 10). From S2b the model drains `pending` as
D10 does, through `backlog` and `lex_backlog`. A step lands a backlog at once,
or holds it across later edits and lands it at its old edit mark, then runs
`batch` with no dirty range, as D10's landing does. The notes include blocks with
no content. At the end of each case, and after any step the generator picks,
the model drains with no edit. Each landing must land at least one block, and
the drain must end within one landing per pending block. Then the marks must
equal a fresh colouring. A drain that does not end is a livelock, and fails
the test. The test needs `inkra-editor` as a dev-dependency of
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
| Binary size | a few hundred KB for eight lexers, mostly keyword tables; S1a (Rust only): `main` 19,364,544 B, S1a 19,360,464 B, difference -4,080 B; S3 (Python, JSON, Bash): `main` 19,717,024 B, S3 19,752,496 B, difference +35,472 B; S4 (JavaScript, TypeScript, TSX, CSS, HTML): `main` 19,751,424 B, S4 19,805,072 B, difference +53,648 B | `stat -f %z` on the release binary, `main` against each slice |
| Lex, Rust | S1a medians: 1.71 us for 20 lines (923 B), 54.7 us for 500 lines (16.1 KB), 646 us for 5,000 lines (169 KB) | `cargo bench -p inkra-highlight -- --quick`, the texts of the 2026-11-11 tree-sitter figures |
| Lex, Python | S3 medians: 1.91 us for 20 lines (657 B), 49.7 us for 500 lines (16.1 KB), 505 us for 5,000 lines (162 KB) | `cargo bench -p inkra-highlight --bench lex -- --quick`, group `lex_python`: a sample repeated to the Rust texts' line counts (S3 deviation 10) |
| Lex, JSON | S3 medians: 0.42 us for 20 lines (471 B), 10.5 us for 500 lines (11.8 KB), 105 us for 5,000 lines (118 KB) | the same, group `lex_json` |
| Lex, Bash | S3 medians: 0.95 us for 20 lines (531 B), 24.1 us for 500 lines (13.3 KB), 241 us for 5,000 lines (133 KB) | the same, group `lex_bash` |
| Lex, JavaScript | S4 medians: 2.84 us for 20 lines (636 B), 65.2 us for 500 lines (14.7 KB), 653 us for 5,000 lines (146 KB) | the same, group `lex_javascript` (S4 deviation 13) |
| Lex, TypeScript | S4 medians: 2.12 us for 20 lines (426 B), 59.0 us for 500 lines (11.2 KB), 592 us for 5,000 lines (111 KB) | the same, group `lex_typescript` |
| Lex, CSS | S4 medians: 0.85 us for 20 lines (450 B), 21.2 us for 500 lines (11.3 KB), 211 us for 5,000 lines (113 KB) | the same, group `lex_css` |
| Lex, HTML | S4 medians: 1.04 us for 20 lines (390 B), 26.0 us for 500 lines (9.8 KB), 260 us for 5,000 lines (98 KB) | the same, group `lex_html` |
| Lex, one long line | linear in its length: S4 medians for unclosed generic lists, TypeScript `a<` 2.07 ms at 32 KiB and 4.18 ms at 64 KiB, Rust `a::<` 0.49 ms and 1.10 ms. With the line's pairing (deviation 16), measured beside main on 2026-11-16: TypeScript `a<` 4.32 ms at 64 KiB against main's 4.20 ms, Rust `a::<` 1.14 ms against 1.01 ms, and nested closed lists, TypeScript `a<…>`, 1.62 ms at 32 KiB and 3.25 ms at 64 KiB | the same, groups `lex_typescript_long_line`, `lex_typescript_nested_line` and `lex_rust_long_line` (S4 deviation 16) |
| Build time | no C, no build script | clean build of `inkra-highlight` |
| Keystroke outside a block | D7's merge; no lex | `app` planted-marks test |
| One listener call | at most `MAIN_THREAD_BUDGET` bytes lexed and published, at most 0.5 ms by D10's definition; S1b: 64 KiB in 313.7 us (one block) and 366.8 us (many blocks) | `cargo bench -p inkra-highlight --bench listener`, D10 |
| One landing's publish | at most 0.5 ms at `TASK_BYTES` with 200 splices since; at most 4 ms for one block at `BLOCK_LIMIT` | `cargo bench -p inkra-highlight --bench landing`, D10 |
| Opening a note | the lex shares decoration layer D8's 8 ms deadline | the open-time test; the launch figure |
| Memory | marks only, no trees: about 3.4 times the block text (D10) | — |

## Tests

- `inkra-highlight`: D12's tests, per language, shared and the model test.
- `inkra-highlight`, `#[test]`: every info word of the Languages table
  resolves, in any case; an unknown word resolves to `None`.
- `inkra-highlight`, `#[test]` on `plan`, one per case: deleting the closing
  fence between two blocks clears both old chunks and lexes the merged block;
  deleting a line right before a block's opening fence keeps the block's
  chunk and lexes nothing (deviation 10);
  a chunk on a block's range with another tag, or none, is lexed over.
- `inkra-highlight`, `#[test]` on `batch` (D10): an unknown-language block and
  one over `BLOCK_LIMIT` get empty chunks outside the budget; dirty blocks
  fill the budget in order, and one that does not fit goes pending while the
  next still fits; an inexact block the dirty range misses goes pending.
- `inkra-highlight`, `#[test]` on `backlog`: it stops before the block past
  `TASK_BYTES`; a first block over it still goes; `lex_backlog` gives the
  marks `block_marks` gives on the same blocks.
- `inkra-highlight`, `#[test]` on `open_chunks` (D8): every block before the
  deadline; past it, only the empty chunks of unknown-language blocks and of
  blocks over `BLOCK_LIMIT` (deviation 11, S2b deviation 11); none over `OPEN_PARSE_LIMIT`, all at the limit;
  none for a block over `MAIN_THREAD_BUDGET`, which spends no deadline check, and one at the budget.
- `inkra-highlight`, Bash: `cat <<''` ended by an empty line; `cat <<A <<B`
  with both bodies; `<<-` with tab-indented tags.
- `inkra-editor`: `Layer::chunks` returns the mapped chunk ranges, sorted,
  each with its tag, after random edits and publishes
  (`proptest`, beside the layer's own).
- `inkra-editor`: `follow` maps a chunk as the fold of `map_chunk` over every
  splice since (deviation 12), and a publish equals its changes placed one by
  one with reached late chunks skipped (S2b deviation 1), after random edits
  (`proptest`).
- `app`, headless `#[gpui::test]` against `MemFs`:
  - a block's runs carry the token colours; a revealed fence stays
    `md_marker`;
  - a keystroke in prose lexes nothing: every block keeps its planted marks;
  - a keystroke in one block lexes that block only;
  - a typed fence, a deleted fence and a changed info word recolour the right
    blocks, and leave no mark on prose;
  - a block in a quote and one in a list item colour their code only;
  - an opened note's first frame with text has its colours;
  - a note edited before its first hearing (`Opened { fresh: false }`) loses the marks
    its open landed on prose;
  - a listener call lexes at most `MAIN_THREAD_BUDGET` bytes, and the rest
    lands from the background task;
  - a block over the budget shows its old marks until the task lands, and its
    colours after `run_until_parked`, with no edit;
  - pending blocks over `TASK_BYTES` in total all colour after
    `run_until_parked`, with no edit: each landing spawns the next;
  - edits during a flight start one more task when it lands, never two at
    once;
  - 257 edits during a flight, each heard by a frame, drop the splice log
    since the task's edit mark; the landing keeps the chunks the main thread
    published since, and the next run colours the rest;
  - switching the feature off during a flight leaves no mark once the task
    would have landed;
  - switching it off and on before the old task lands: the old task never
    publishes, one new task runs, and the final colours equal a fresh
    colouring;
  - closing the note during a flight, then opening another: the old task
    lands into nothing, and the new note colours with its own task;
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
3. **S2a, exactness.** Deviation 10 in `plan` and its tests; the model test
   hears with no dirty range (D12), the gate of deviation 10. Deviation 11:
   `BLOCK_LIMIT`, the empty chunks outside the budget, in `batch` and in
   `open_chunks`, and their tests. An `app` test pins that a line deleted
   right before a fence leaves the block's planted marks. The dirty range
   leaves `plan`, `batch`, the listener, the model test and the listener
   bench (deviation 10); the bench's many-blocks case holds every block but
   the first `n` exactly, so those `n` are inexact.
   - Gate: if the model test fails with no dirty range, stop; deviation 10
     goes, and the touching rule of D7 stays.
4. **S2b, the backlog.** Deviation 12 in `Layer::replace` and its
   `proptest`. `Batch.pending`, with the dirty range back in `batch` and the
   listener for D10's split; `backlog`, `lex_backlog` and their tests. The
   model test's late landings and bounded drain (D12). The landing bench: set
   `TASK_BYTES`, check `BLOCK_LIMIT` at 4 ms; the listener bench gains the
   copy. The app's listener still lexes every pending block on the main
   thread, backlog after backlog until none is pending, as S1b did; an `app`
   test pins that a note with pending blocks over `TASK_BYTES` colours in one
   listener call.
5. **S2c, the background task.** D10's map, spawn and landing, in its order;
   the main thread budget enforced; D8's skip of blocks over the budget at
   open. The `app` tests of D10's task, lifecycle races included.
   `docs/buffer-model.md`'s Rope entry gains its two new consumers (Deferred).
6. **S3, Python, JSON and Bash.** Three lexers, their info words, their
   scene blocks. Bash carries the heredoc state.
7. **S4, JavaScript, TypeScript, TSX, CSS and HTML.** In this order: HTML
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
| Pending blocks in view first, in place of document order | A large note's visible block colours measurably later than a block above it |
| Rust generic lists across lines (`impl<`, a split `where` list) | A rustfmt-formatted block shows an unlit split list (S4 deviation 18) |
| A rope snapshot in place of the backlog's copy (`docs/buffer-model.md`, Rope) | The rope lands for its own trigger; the copy is bounded by `TASK_BYTES` and never triggers it alone |

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
- **No synchronous attempt before the background.** Zed's `Buffer::reparse`
  blocks the main thread up to `sync_parse_timeout`, 1 ms, and spawns the
  parse only past it (`buffer.rs:1177-1181`, `1967-1985`). A lex cannot
  stop midway, so Inkra splits by bytes before it lexes (D10); a byte budget
  also gives the same split in every test run.
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
   way, and mutants reach the fold. From S2a only its unit test calls it;
   S2b's listener calls it again (deviation 10). A pure deletion folds to an empty range at
   its offset; `plan`'s meets counts touching, so it still relexes a block it
   touches.
3. **`content_lines` lives in `inkra-markdown`, not in `app`.** The model test
   (D12) and the listener then read the same content lines from one function.
   A copy in the test could drift from the app's.
4. **The model test emulates the core's `Heard`.** `Lost` is a clear, then a
   hearing, because the splice log drops edits only past a bound a short run
   never reaches. A hearing reads no dirty range (deviation 10), so
   `Opened { fresh: false }` is a plain edit step. `Opened { fresh: true }` is
   a fresh layer seeded with the first k blocks, then a hearing.
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
   other layers pass 0. An open function's chunks carry the tag too, so D7
   matches a seeded chunk the same way.
8. **One publish per listener call, a batch of clears and chunks.** Deviation
   6's listener called `decorations::remove` once per clear and
   `decorations::publish` once per lexed block. Each call repeated the rank
   lookup, the entry lookup, the catch-up, the read of the splices since its
   edit mark, the global update and the redraw. A `Lost`, an `Opened { fresh: false }` or
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
   run the code that ships. `batch(text, &Markdown, held)` now returns
   the clears and the lexed block chunks, and `block_marks` lexes one block
   over its content lines. `open_chunks` takes the text and the deadline, and
   parses inside, with the same `OPEN_PARSE_LIMIT` check and the same deadline
   check before each block. `plan` is crate-private: `batch` is its one
   caller. The app keeps the map from `TokenClass` to `ColorToken` and the
   publish; the dirty range returns to `batch` in S2b (deviation 10). D3 already read the content lines from the
   parse, and `inkra-app` already depends on `inkra-markdown`, so the edge
   adds no crate to the app's build. D1's objections still hold.
10. **`lex` holds the blocks no held chunk has exactly; the dirty range only
    picks, in `batch`, which of them the main thread lexes first.** D7 also lexed a block the dirty range met, touching
    included, even when a chunk had it exactly. A chunk keeps its tag only
    while no splice removed one of its bytes or inserted strictly inside it,
    and no publish cut it. So a chunk exact on a block holds marks lexed from
    the same bytes, the same range and the same shape: a lex gives the same
    tokens, whatever happened in between. The dirty range added only work, and
    D10 needs exactness alone: a landing replans with no dirty range, and a
    buffer no view shows, or a layer `declare` reset, hears no splice at all.
    The dirty range now picks which inexact blocks the main thread lexes
    first (D10). Deviation 2's touching rule survives only in that choice.
    `plan` takes no dirty range, and its `lex` stays in document order. In
    S2a nothing reads the dirty range, so it leaves `batch` and the listener
    too; S2b brings it back into `batch` with `Batch.pending`.
    D12's model test is the gate: every hearing runs with no dirty range, and
    late landings interleave with the edits.
11. **A block with nothing to lex gets its empty chunk outside every budget.**
    D6 gave such a block an empty chunk, and D10 counted block bytes against
    the budget. A block with no known language, or over `BLOCK_LIMIT`, now
    gets its empty chunk from `batch` at once, whatever the dirty range and
    the budget, and from `open_chunks` too, past its deadline included.
    `BLOCK_LIMIT` is now lower than `OPEN_PARSE_LIMIT`, so `open_chunks`
    checks both, through the same `unlexed` as `batch` (S2b deviation 11).
    Left pending, a block over
    `BLOCK_LIMIT` would never become exact, and each landing would spawn a
    task for it, forever.
12. **A late publish maps a chunk only through the splices that reach it.**
    `Layer::replace` folded `map_chunk` over every splice since the publish,
    and `map_chunk` rebuilt every mark even for a splice outside the chunk. A
    landing after an `apply_diff` reload of 200 hunks then cost its marks 200
    times over. A splice that removes none of the chunk's bytes and inserts
    nowhere strictly inside it now moves the chunk's start, or nothing when it
    lies after the chunk. The marks are relative to the start, so they stay,
    as `Layer::shift` already does for the chunks a splice does not reach.

## As built (S2b)

Code cites these as "S2b deviation N".

1. **A late chunk that a splice since reached does not land.** `Layer::publish`
   maps each chunk of a late publish through the splices since its edit mark
   `at`. When one of them reaches the chunk, by deviation 12's rule (it
   removes one of the chunk's bytes or inserts strictly inside it), the chunk
   is dropped: its marks were lexed from bytes that changed. The layer keeps
   what it holds there, the old marks the core already shifted. Where a
   splice reached that held chunk too, its tag went at the catch-up. Where
   none did, it still has the range or the tag that made the block pending.
   Either way the next replan lexes the block again, unless a held chunk has
   it exactly, and an exact chunk holds marks lexed from the same bytes
   (deviation 10). A chunk that no splice reached still maps through
   `follow`. With no
   splice since `at`, everything lands as before. The returned span covers
   only the changes that landed, so a dropped chunk moves no mark. Removals
   still map through `follow`: dropping one would leave a chunk the publisher
   meant to clear. The rule is the core's, so it holds for every layer, not
   only `syntax`: the test feature's late whole-text chunk now lands nothing
   once an edit reached it (`a_late_publish_across_an_off_on_switch_skips_the_chunk_an_edit_reached`).
   Phase 1 of S2b mapped every reached chunk through the rest of the splices,
   and the landing bench failed its gate (figures below).
2. **One pass over the chunk list per publish.** The list-wide `replace`
   drained and spliced the layer's whole `Vec` once per change. A `Cursor`
   (`layer/cursor.rs`) now splits the list at a point: the chunks before it in
   a `Vec`, the rest in a `VecDeque`. Each change seeks from where the last one
   left the point and puts its pieces back in front of the rest. Changes in a
   batch may overlap and come in any order, since a later change wins, so the
   point may move back; in document order, as the listener and a landing
   publish, each chunk moves a bounded number of times. `follow_all` maps one
   change: it moves only the chunk's start until the first splice that reaches
   it, then folds `follow` from there, or drops a fill chunk there (deviation
   1). It is not one walk merged over the changes and the splices: that walk
   needs sorted, disjoint changes, and `publish` accepts overlapping ones, so
   it would be a second path. Per change the cost is one comparison per
   splice, with no allocation and no mark rebuilt for a splice that does not
   reach it. A reference `proptest` (`layer/cursor/tests.rs`) checks a publish
   against the list-wide `replace` with `map_chunk` folded over every splice:
   a dropped chunk keeps the old mapped marks, the others equal the fold.
3. **The deviation-12 `proptest` sits in the unit tests of `layer/map.rs`**
   (`layer/map/tests.rs`), not beside `tests/layer_proptest.rs`. Its
   reference is the private `map_chunk`.
4. **The drain is bounded per landing, not by the pending count at its
   start.** D12 asked for at most one landing per block pending when the drain
   starts. The model test requires instead that each landing copied at the
   current edit mark leaves strictly fewer blocks pending, and that the drain
   ends with none. A stale first landing can make an exact block inexact: a
   pending block's range can map onto a block whose held chunk is exact, and
   the landing replaces that chunk under the old shape's tag. Every landing
   after the first shrinks the pending set, so the drain still ends.
5. **The listener walks the backlog without replanning.** It advances through
   `pending` by the number of chunks each `lex_backlog` returns, one per block
   taken, instead of D10 step 5's replan. Within one call the edit mark does
   not change, so a replan would find the same remainder.
6. **`BLOCK_LIMIT` is `pub`.** The landing bench reads it instead of restating
   it.
7. **The listener bench lexes the backlogs past the budget.** Without that,
   its sizes over 64 KiB would measure no lex, because `batch` now caps the
   main thread's lex. The copy is timed on its own, in `listener_backlog_copy`.
8. **`lex_pending` holds the backlog loop.** The listener, the layout bench's
   `Typing` and the listener bench each lexed every pending block, backlog
   after backlog, in three copies. `inkra_highlight::lex_pending` is that loop
   once. D10's task lexes one backlog per flight, so it needs only
   `lex_backlog`.
9. **The landing bench holds its stale blocks on a twin buffer.** It held them
   by a publish at the old edit mark, which deviation 1 now skips for every
   block a splice reached. A second buffer with the same text, never edited,
   takes that publish; the layer then catches up to the scattered buffer,
   as the landing finds it.
10. **The landing bench measures splices that reach no block.** Scattered
    splices reach most blocks, and a reached block lands nothing (deviation
    1), so D10's scattered cases mostly time a skip. `landing_above` puts the
    200 splices in a 4 KiB prose paragraph above the blocks, so that none
    reaches one, for the same three cases; `TASK_BYTES` takes the worst of
    both. `landing_block_limit` measures one block from 32 KiB to 512 KiB
    with the splices above it, and with none since: a landing with no edit
    since lands every chunk whole.
11. **`open_chunks` gives a block over `BLOCK_LIMIT` its empty chunk.** At
    512 KiB the bound was over `OPEN_PARSE_LIMIT`, so the open function
    never met such a block (deviation 11). At 256 KiB it can, and it lexed
    it. `unlexed` in `batch.rs` now decides for both: no known language, or
    over `BLOCK_LIMIT`. Such a block spends no deadline check.
12. **An empty publish returns before it touches the chunk list.** The
    syntax listener publishes an empty batch on every keystroke in prose,
    and deviation 2's cursor moved the whole list for it. `Layer::publish`
    still catches up and still reports `Lost`, then returns `Ok(None)`.

The dirty range comes from `inkra_text::Splice::compose` over the heard
splices; deviation 2's `inkra_highlight::dirty` is gone. A pure deletion
composes to an empty range at its offset, which `batch` still meets, as
touching counts.

**`TASK_BYTES` is 32,768 (32 KiB); `BLOCK_LIMIT` is 262,144 (256 KiB).**
`cargo bench -p inkra-highlight --bench landing` measured these medians on
2026-11-16. With D10's 200 scattered splices since the landing's edit mark,
which mostly time a skip (deviation 10):

| Bytes   | One block | Dense blocks | Empty blocks |
| ------- | --------- | ------------ | ------------ |
| 4 KiB   | 11.4 us   | 13.4 us      | 39.3 us      |
| 8 KiB   | 21.6 us   | 26.0 us      | 97.4 us      |
| 16 KiB  | 41.3 us   | 53.0 us      | 198.4 us     |
| 32 KiB  | 82.4 us   | 104.6 us     | 404.3 us     |
| 64 KiB  | 161.2 us  | 214.1 us     | 806.1 us     |
| 128 KiB | 341.3 us  | 671.1 us     | 1.64 ms      |

With the 200 splices in a prose paragraph above the blocks, reaching none
(`landing_above`):

| Bytes   | One block | Dense blocks | Empty blocks |
| ------- | --------- | ------------ | ------------ |
| 4 KiB   | 38.7 us   | 46.6 us      | 53.2 us      |
| 8 KiB   | 73.0 us   | 92.5 us      | 108.9 us     |
| 16 KiB  | 142.7 us  | 188.9 us     | 212.5 us     |
| 32 KiB  | 288.5 us  | 393.7 us     | 427.4 us     |
| 64 KiB  | 571.6 us  | 756.4 us     | 853.0 us     |
| 128 KiB | 1.20 ms   | 1.51 ms      | 1.71 ms      |

The largest size where the worst of the six is at most 0.5 ms is 32 KiB, at
427.4 us (empty blocks, splices above).

One block, with no splice reaching it (`landing_block_limit`):

| Bytes   | Splices above | No splice |
| ------- | ------------- | --------- |
| 32 KiB  | 286.6 us      | 284.4 us  |
| 64 KiB  | 573.3 us      | 582.1 us  |
| 128 KiB | 1.21 ms       | 1.23 ms   |
| 256 KiB | 3.31 ms       | 3.28 ms   |
| 512 KiB | 6.34 ms       | 6.35 ms   |

The largest size within 4 ms is 256 KiB. With scattered splices, one block at
512 KiB took 1.36 ms, because the splices reached it and it landed nothing.

Before deviations 1 and 2, the scattered bench measured one block at 7.74 ms
at 4 KiB and 248 ms at 128 KiB, empty blocks at 448 us at 4 KiB and 109.7 ms
at 128 KiB, and one block at 512 KiB at 1.002 s.

`cargo bench -p inkra-highlight --bench listener`, the same day, with
`TASK_BYTES` at 32 KiB:

| Bytes   | One block | Many blocks |
| ------- | --------- | ----------- |
| 4 KiB   | 19.2 us   | 26.0 us     |
| 8 KiB   | 44.0 us   | 45.7 us     |
| 16 KiB  | 89.8 us   | 85.0 us     |
| 32 KiB  | 176.9 us  | 164.7 us    |
| 64 KiB  | 368.4 us  | 327.6 us    |
| 128 KiB | 730.0 us  | 658.4 us    |

`MAIN_THREAD_BUDGET` stays 64 KiB, at 368.4 us. Past 64 KiB the call lexes
its backlogs too (deviation 7). `listener_backlog_copy` copies one backlog at
`TASK_BYTES` in 13.3 us as one block and in 12.8 us as blocks of 512 bytes.

## As built (S2c)

Code cites these as "S2c deviation N".

1. **The map is a `Flights` struct, not a bare `HashMap`.** It holds the map
   D10 names, plus a spawn count compiled only in tests. The `app` tests read
   that count to assert one task per buffer, and their `land` helper ticks
   the executor until the task in flight has landed and no further. A test
   that heard a buffer by calling `decorations::sync` keeps its task in
   flight until it ticks, so each race runs in a fixed order.
2. **The listener's replan and the landing's step 5 are one function,
   `relex`.** The listener passes the dirty range of its `TextChange`; the
   landing passes none. `listener(flights)` builds the closure, so
   `activate` and the tests wire the same code with their own map.
3. **The task is one foreground task that awaits one background lex.** The
   map holds the foreground `Task`; dropping it drops the background `Task`
   it awaits, which cancels both. The landing removes its own entry first
   (step 2) and drops it once the `RefCell` borrow is released. Zed's
   `Buffer::reparse` has the same shape (`buffer.rs:1987-2019`).
4. **The `app` tests use plain `Buffer` entities, not `MemFs`.** The syntax
   listener reads no file. Closing a note is releasing its buffer, a reload
   is `apply_diff`, and a layer lost in flight is 257 edits no frame heard.
   `task/tests.rs` holds the races; it shares `tests.rs`' helpers.
5. **A livelock fails at once instead of hanging the suite.** A landing
   that never makes its blocks exact spawns the next task forever, and
   `run_until_parked` would never return. Two checks fail the first such
   landing in a test build. `backlog` panics on an empty `pending`, so a
   task is never spawned for nothing. A landing with no edit since `at`
   asserts that every block it carried is now held exactly (S2b deviation
   4). Every `highlighting` test also waits through `settle`, which panics
   after 1,000 executor ticks.
6. **`lex_pending` serves the benches only.** The app no longer calls it,
   and the layout bench's keystroke leaves a pending block to the task, as
   the listener does.
7. **D8's skip spends no deadline check,** as an `unlexed` block does not.
   So one block over the budget cannot stop the blocks after it.
8. **A landing on a layer switched off stops after step 2.** The publish
   would land nothing, and step 5 would then find the same blocks pending,
   forever. Step 1 alone does not cover it: a layer can be off while its
   listener, and so its map, still lives. `decorations::enabled` reads the
   switch; with the entry removed, a later switch-on spawns again.

No figure changed: S2c moves work between threads and adds no lex. The
landing bench and the listener bench measure the same pure parts as in S2b.

## As built (S3)

Code cites these as "S3 deviation N". Where D4 is silent, each lexer follows
Zed's `highlights.scm` for its language (`vendor/zed-reference/crates/grammars/src/`),
mapped to D5's classes by the 2026-11-11 spec's capture table.

1. **Each new language has its own scene.** The viewport of a shot holds one
   block of this size, so blocks added after the Rust one in
   `code-highlighting` would not be seen. `code-highlighting-python`,
   `code-highlighting-json` and `code-highlighting-bash` show one block each;
   `code-highlighting` keeps its fixed Rust text for S6.
2. **The lexers share `scan.rs`.** The Rust lexer's word, space, number and
   all-caps scans, an escaped quoted string, and the line's token sink moved
   there, since two or three lexers need each. Rust's escaped string now calls
   the shared `quoted`; its raw string and nested block comment stay its own.
   `Open::Quoted` carries a quoted string across lines for Python and Bash.
   Every lexer runs its steps through `scan::drive`, which asserts that each
   step advances: a stalled step would hang the lexing task, so it panics
   at once instead.
3. **A heredoc also records whether its body expands.** D2's entry holds the
   tag's hash and the `<<-` flag. An unquoted tag's body lights its `$`
   parameters, as in a double-quoted string; a quoted tag's body is one
   `String`, as Zed shows both. The tag hash is FNV-1a over the tag without
   quotes and backslashes, folded byte by byte, so a body line hashes the same
   way. `<<<` is a here-string and queues nothing. A `<<` inside `((` or
   `$((` is a shift.
4. **A heredoc body starts on the line after its operator.** Bash starts it
   after the logical line, so a `\` at that line's end delays it one more
   line. The lexer does not: the case is rare, and it would cost a flag in the
   line state.
5. **Bash, by Zed.** A command's name is `Function`, its argument words are
   `String`, and a word that starts with `-` or is all digits is `Constant`.
   A reserved word is a `Keyword` only where a command could start, and `in`
   only after `for NAME` or `case WORD`. A case pattern up to its `)` is
   `String`. Inside `[` and `[[`, `-f` and the like are `Keyword`, and `==`,
   `!=`, `=~`, `=` and `!` are `Operator`. A parameter's `$`, `${` and `}` are
   `Punctuation` and its name is plain, as Zed's `variable` is. An
   assignment's name is plain and its `=` an `Operator`; a `[…]` subscript
   before it has `Punctuation` brackets and lit parameters. A `(` right after
   the `=` opens an array, whose words are arguments up to its own `)`, across
   lines. `for NAME` takes `do` with no `in`, on its line or the next. `$(`
   and a backtick open a command, `$((` and `((` arithmetic, whose names are
   plain. The look-behind holds the parse position: `Prev` gains Bash's
   states, and the nesting stack gains `Arith`, `Backtick` and `Array`, and a
   `Paren` for each `$(`, so an array closes on its own `)`.
6. **Bash strings light simple parameters only.** In a double-quoted string
   and an unquoted heredoc body, `$name`, `${…}`, `$1` and the special
   parameters are lit; `$(…)`, `$((…))` and backticks stay `String`. A
   shebang stays a `Comment`: Zed's `keyword.directive` needs the block's first
   line, which the line state does not know.
7. **Python, by Zed.** `match`, `case` and `type` are keywords only at a line's
   first word, followed by a space and not by `=`, `.`, `:`, `,` or `)`.
   `print` and `exec` are builtins, not keywords; `self` is plain. A built-in
   type is `Type` everywhere, not only in a type or a call. Leading
   underscores do not count for the all-caps and capitalised rules, as in
   Zed's regexes. A lone capital, such as `T`, is a `Constant`: Zed's constant
   regex takes one letter, where Rust's needs two. `.` is `Punctuation`, as in Zed, where Rust has `Operator`;
   both have one colour. `...` is a `Constant`. A decorator, `@` and its dotted
   name, is `Attribute` (D4) where Zed has `function.decorator`; an `@` that
   starts no line, or no name, is the matrix-product `Operator`.
8. **Python strings are one token per line.** An f-string's `{…}` stays
   `String`; Zed lights it as embedded code. A string prefix is any of `r`,
   `u`, `b`, `f`, `t` and their two-letter pairs, in any case. Only a
   triple-quoted string spans lines; a single-quoted one ends at the line end,
   a backslash continuation included.
9. **JSON and JSONC are one language.** Both info words lex comments, as
   tree-sitter-json does. A string is a key when a `:` follows it on its line;
   a key whose `:` sits on the next line colours as a value. A block comment
   does not nest, and a `-` belongs to the number after it.
10. **The lex bench gains one group per language.** The repo has no Python,
    JSON or Bash text of the Rust texts' sizes, so each group repeats a sample
    of about 20 lines up to 20, 500 and 5,000 lines. The Rust group `lex` is
    unchanged. Python lexes about as fast as Rust per byte; JSON and Bash run
    faster, with fewer look-aheads.
11. **The model test is unchanged.** It enumerates no languages: it checks
    exactness, which holds for any lexer that reads only its block's bytes.
    The shared `proptest` runs all four lexers on fragments that add triple
    quotes, decorators, heredoc operators, expansions and case words.
12. **The code-highlighting scenes list in their own module.** The three new
    scenes would push `shot/scenes.rs` past its line ceiling, which only
    lowers. So `code_highlighting::SCENES` holds all five, and `catalogue()`
    chains it after the rest, in the same order as before.

## As built (S4)

Code cites these as "S4 deviation N". Where D4 is silent, each lexer follows
Zed's `highlights.scm` for its language, as in S3.

1. **TypeScript and TSX are a dialect of the JavaScript lexer.** `Dialect`
   holds two flags, `jsx` and `types`. JavaScript sets `jsx`, TypeScript sets
   `types`, and TSX sets both. `typescript.rs` holds TypeScript's soft
   keywords and built-in types, and calls `javascript::lex_dialect`; no rule
   is copied.
2. **Templates, JSX and embedded languages live on the nesting stack.**
   `Nest` gains `Condition`, `Template`, `JsxTag`, `JsxClosingTag`, `JsxText`,
   `Alias`, `Script` and `Style`, all without data. The top entry picks the mode: a
   template's text, a JSX tag, JSX children, or code. `${` and a JSX `{` push a
   `Brace`, and its `}` returns to the mode below. `LineState` stays 56 B and
   holds no heap. Past the stack's 16 entries, a line lexes as code.
3. **`Prev` gains the parse positions these lexers need.** JavaScript adds
   `ValueEnd`, `Condition`, `KeyStart`, `Param` and `Annotation`; CSS adds
   `Selector`, `Declaration` and `AtRule`; HTML adds `HtmlTag`, `HtmlScript`
   and `HtmlStyle`. A `}` is not a value end, so `}` then `/re/` opens a
   regex.
4. **D4's `<` rule, as built.** Where JSX is on, a `<` at an expression start
   opens a tag when a letter or a `>` follows, so `<>` opens a fragment. In
   TypeScript with JSX off, a `<` and a letter at an expression start open a
   generic list, so `<T>(x: T) => x` is a generic arrow. TSX reads `<T,` and
   `<T extends ` as type parameters, as TypeScript does in `.tsx` files. A
   type alias's `=` and an annotation's `:` start a type, where `<` opens type
   parameters in both dialects: `Alias` marks `type Name` up to its `=` and
   crosses a line only under a nesting opened after it (deviation 18). Outside
   deviation 18's case, a generic list opens only where it closes on its line
   (`Generics::end`), so `Count < limit` compares. A name before such a list
   and `(` is a `Function` in TypeScript.
5. **A TypeScript annotation is lit in five places only.** After a
   parameter, after a name that `let`, `const` or `var` declares, after a
   member's `?:`, after a member's `:` in a type body (deviation 19), and
   after a function's or a method's `):` (deviation 20), the next name is a
   `Type`. An object key's value and an arrow's return type keep the general
   rules. A capitalised name or a
   built-in type is a `Type` anywhere in TypeScript, which covers most return
   types.
6. **Soft keywords are keywords only before an operand.** `as`, `from`,
   `get`, `of`, `set` and `static`, and TypeScript's sixteen, are keywords
   when a name, a quote, a backtick, a `{`, a `[`, a `*` or a `#` follows. So `type = 1` and
   `x.of` stay names. A modifier such as `readonly` or `private` starts a
   key, after a decorator too, so the member after it is a `Property`.
7. **JavaScript and TypeScript, by Zed.** `this` and `super` are plain.
   `true`, `false`, `null` and `undefined` are `Constant` (D4). A regex's
   flags are `Keyword`, `${` and its `}` are `Punctuation`, a ternary `?` is
   an `Operator`, and `?.` is `Punctuation`. In JavaScript, a capitalised name
   is a `Type` only after `class`, `extends` or `new`, or as a built-in object
   before `.`; a lone capital is a `Constant`. In TypeScript a lone capital is
   a `Type`, the usual type parameter. `constructor` before `(` is a `Type`. A
   decorator is an `Attribute` (D4), a `#private` name a `Property`, a tagged
   template's name a `Function`, and a hashbang a `Comment`. A JSX component
   name, dotted or not, is a `Tag`; a JSX entity is a `String`. `void` stays a
   `Keyword` even in a return type.
8. **JavaScript strings end at the line end.** A `\` continuation is not
   followed, as for Python's single-quoted strings (S3 deviation 8).
9. **CSS, by Zed.** In a selector, a tag name, `*` and `&` are `Tag`; a class,
   an id and a pseudo-class are plain after their `Punctuation`, as Zed's
   `selector.*` captures have no class. Combinators are `Operator`. A unit is
   a `Type`, a hex colour a `String`, and a custom property plain. An unquoted
   `url(…)` argument is one `Constant`. `from` and `to` in a keyframes block
   are `Keyword`, and `!important` is a `Keyword`.
10. **CSS tells a declaration from a nested rule on its line.** An item is a
    declaration when the first `{`, `;` or `}` on its line is not a `{`. With
    none of the three, it is a declaration when its name is followed by `:`
    and then a space or the line end; inside a rule, a `:` with no space
    after it counts too, unless a `,` ends the line as in a selector list. So
    a rule's last `margin:0` is a declaration. The same rule applies at the
    top level, so a `.jsx` style object or a lone declaration still colours.
11. **HTML, as built.** `<!DOCTYPE html>` is a `Tag` between `Punctuation`.
    A `</script` or `</style` closes the embedded language anywhere on the
    line, in any case, even inside a string, as browsers do. An attribute
    value stays a `String` even for `style` and `onclick`; an unquoted one
    runs to a space or `>`, so `href=/about/team` is one value. A `/` not
    followed by `>` ends no tag. The closing tag resets the line state, so a
    comment left open in a script ends there.
12. **Each new language has its own scene,** as in S3 deviation 1:
    `code-highlighting-javascript` (info word `jsx`), `-typescript`, `-tsx`,
    `-css` and `-html`. Each line fits the block's width, so no line scrolls.
13. **The lex bench gains four groups,** as in S3 deviation 10: a sample
    repeated to 20, 500 and 5,000 lines. TSX has no group: it runs
    TypeScript's code with one flag changed. On the run after the review,
    Rust's `lex` group measured 705 us at 5,000 lines against 646 us at S1a.
    JavaScript and TypeScript lex at about 4.5 and 5.3 us per KB, against
    Rust's 4.2 at the same run. `MAIN_THREAD_BUDGET` was measured with Rust
    text; TypeScript fills it about a quarter slower. The CSS lexer as first
    built measured 199 us at 5,000 lines on that run, against 211 us after
    the review; the first S4 run gave 177 us.
14. **`TokenClass` gains `Tag`.** D5 names eleven classes, but S1a built ten,
    as no lexer coloured a tag. `app` maps it to `syntax_tag`, which the theme
    already holds.
15. **`scan.rs` gains the scans two lexers share.** `generic_end` (now
    `Generics::end`, deviation 16) moves from `rust.rs` for TypeScript, `block_comment` from `json.rs` for JavaScript,
    CSS and, with its closer as a parameter, HTML's `<!--`, and `dotted_end`
    from `python.rs` for decorators. `entity_end` serves HTML and JSX.
    `end_while` is every name scan, and `Sink::token` pushes a token and
    returns its end.
16. **A generic list closes at any distance on its line, in linear time.**
    `Generics::end`, one per line for TypeScript and Rust alike, finds where
    a `<` closes before a `;`, `{` or `}`. Rescanning to the line end for
    every name before a `<` lexed one long line in quadratic time. So it looks
    ahead `LOOK_AHEAD` (64) bytes first, which decides almost every list
    written by hand. A list those bytes leave open is found in a pairing of
    the whole line: one pass with a stack of open `<`, made once and kept for
    the line.
    The pairing is `#[cold]` and out of line: inlined, it slowed ordinary
    TypeScript by 5% and JavaScript, which shares the lexer, by 3%. A list
    holding an object type is not a generic call; its capitalised names stay
    `Type`. A 64 KiB line of `a<` lexes in about twice the time of a 32 KiB
    one (Cost). Its cost per byte is still about 15 times prose's: every `<`
    walks the operator table. A first cut, 2026-11-16, stopped at 64 bytes,
    so a longer list was not lit.
17. **JSX tag edges.** A self-closing `/ >` with spaces is one `Punctuation`,
    and a `/` not followed by `>` ends no tag. A child tag past the nesting
    room stays text: the stack could not store it, so its attributes would
    lex as code.
18. **A TypeScript generic list in a type position spans lines.** A name
    classed `Type` in a type position leaves `Prev::TypePosition`, not
    `Prev::Type`. A type position is the name right after an annotation's
    `:`, a type alias's `=`, or a type keyword (`class`, `interface`,
    `extends`, `implements`, `type`, `new`…), and every name inside an open
    generic list. A `<` after such a name opens a list that its line leaves
    open when the line ends on `<` or `,` with no `;`, `{` or `}` past the
    `<`: Prettier's `Map<` and a hand-written `Record<string,`. The list
    stays open on the stack, and the lines below lex as types until its `>`.
    - `LineState` gains no field: the nesting stack already carries a
      `Generic` across lines, and its 16 entries bound the depth. It stays
      56 B.
    - The line end bounds a wrong guess. `new Date < deadline` without a
      semicolon keeps the same-line rule, since its line ends on a name; a
      first cut that opened every such `<` lexed the lines below as types up
      to the next `;`, `{` or `}`. A trailing comment after the `<` keeps the
      same-line rule too.
    - A carried list ends at a `;`, `{` or `}`, as on one line. So
      `Promise<{` at a line end lights its `<` as `Punctuation`, and the `>`
      after the object type's `}` stays an `Operator`.
    - Elsewhere the same-line rule holds. An expression's `f<\n T>(x)`, an
      arrow's return type, a name after `|` or `&`, and a name after `as`
      or `satisfies` are not type positions. A `<` after a value can compare,
      and an `as` type has no end the lexer sees. A member's plain `:` in a
      type body is an annotation (deviation 19), so `a: Map<` spans lines
      there.
    - `type Name<` at a line end leaves `Alias` under the list, so the
      alias's `=` on a later line still starts a type. A `{` in the list
      leaves it under a `Brace`; the line end pops it once it is on top.
    - Rust's lexer is unchanged: rustfmt splits `impl<` and `where` lists
      rarely, and `Deferred` records it.
    - Measured beside main on 2026-11-16: `lex_typescript` −1.3% to −3.0%,
      `lex_javascript` −1.1% to −1.3%, the long and nested line groups −2.7%
      to +2.4%. An A/A run of the same code moved the short groups ±0.5% and
      the line groups up to +7.5%, so no change is measurable.
19. **A TypeScript type body annotates its members.** A `{` after a type
    position, or right after an annotation's `:` or an alias's `=`, pushes
    `Nest::TypeBody` in place of `Brace`. Examples: `interface I {`,
    `class A extends B {`, `let p: {` and `type T = {`. A `}` closes it as it
    closes a `Brace`.
    - Inside it, a member's `:` starts a type, as `?:` does anywhere. So
      `items: Map<` spans lines as in deviation 18, and `name: label` lights
      `label` as a `Type`.
    - A list opened after a type position pushes `Nest::TypeGeneric`, and
      its `>` leaves `TypePosition`, so `interface I<T> {` opens a type body.
      Any other list's `>` leaves `ValueEnd`, as before. A field on `Generic`
      would have touched Rust's lexer, which compares `Generic` by value.
    - An object literal, a block and a function body stay a `Brace`. Their
      `{` follows `=`, `(`, `)`, `return`, a return type's name, or the `>`
      of a list that opened after none of these. So `<Options>{ key: value }`,
      `(): Promise<T> {` and `as Box<T> < limit` read as before; a review
      caught all three when every `>` left `TypePosition`.
    - A `namespace` or `enum` body is a type body too, its name being a type
      position. A label at its top level is then read as a member; `enum`
      members take no `:`.
    - A heritage list's later name, as in `implements A, B {`, is no type
      position, since `,` starts a key. Such a class body stays a `Brace`,
      as before.
    - Measured beside main on 2026-11-16: `lex_typescript` −4.7% to +0.7%,
      the line groups −3.7% to −0.9%, `lex_javascript` −2.2% to −6.3%
      through no code of its own; inside deviation 18's A/A noise.
20. **A TypeScript function's or method's return type is a type position.**
    So `): Promise<` at a line end spans lines, and `): label` lights `label`.
    - A `(` after a declared function's name, or after `function`, pushes
      `Nest::Params`. So does a `(` after a method's name: a name before `(`
      at a member's start in a type body. A `)` closes `Params` as it closes
      a `Paren`, and leaves `Prev::Returns`; a `:` after it pushes
      `Nest::ReturnType` and leaves `Annotation`.
    - `ReturnType` ends at the body's `{`, at a `;` or `,`, at a closer, and
      at a line end where it is on top, as `Alias` does. A `{` right after
      the `:` opens an object type, a `TypeBody` above it; any later `{` pops
      it and opens the body as a `Brace`.
    - `function f<T>(` keeps its parameter list: a list opened after a
      declared name pushes `Nest::FnGeneric`, whose `>` leaves `FnName`. A
      generic call's name now leaves `Type`, not `FnName`, so `f<T>(x) / 2`
      still divides; `FnName` means a declared name only.
    - An arrow's `(a): T =>` keeps the general rules: at its `(`, nothing
      tells it from a ternary's `c ? (a) : b`.
