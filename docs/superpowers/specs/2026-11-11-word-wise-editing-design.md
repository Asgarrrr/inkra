# Word-wise editing: `⌥←`, `⌥⌫`, double and triple click

Date: 2026-11-11. Status: **proposed**, awaiting review. No code yet.

Every macOS text field moves by word with `⌥←` and `⌥→`, deletes a word with
`⌥⌫`, selects a word on a double click and a paragraph on a triple click.
Inkra does none of these. This spec decides the word rule, where it lives,
the movements, the deletes, the bindings, the mouse, the case of concealed
markdown markers, and the slices that build it.

## Context

Checked against `main` at `8ef653b5`:

- `LogicalMovement` has six variants: `Left`, `Right`, `LineStart`,
  `LineEnd`, `DocStart`, `DocEnd` (`crates/editor/src/state.rs:8-16`).
- `EditorState::move_` maps each one to a target (`state.rs:228-242`).
  `Left` and `Right` step by grapheme through `Buffer::previous_grapheme`
  and `next_grapheme`. With a selection and no `extend`, the `collapsing`
  arm (`state.rs:229-232`) puts the caret on the selection's start or end.
- `backspace` and `delete` share a private `remove` (`state.rs:204-226`). It
  deletes one grapheme, or the selection, through `edit`, which calls
  `Buffer::transact_at` (`state.rs:65-75`). So a run of backspaces merges
  with typing into one undo step while the gaps stay under `GROUP_INTERVAL`,
  300 ms (`crates/text/src/buffer/history.rs:8`, `:130-139`).
- `edit_ungrouped` calls `Buffer::transact` instead: one undo step that
  merges with neither neighbour (`state.rs:77-82`). Cut and paste use it.
- The grapheme steps live on `Buffer`, in
  `crates/text/src/buffer/graphemes.rs:9-51`, on `unicode-segmentation`'s
  `GraphemeCursor`. `clip_grapheme` snaps an offset onto a grapheme boundary
  in a chosen direction (`graphemes.rs:62-76`).
- `unicode-segmentation` is a workspace dependency (`Cargo.toml:24`, `"1.13"`,
  locked at 1.13.3 in `Cargo.lock:5442-5443`) and a dependency of
  `inkra-text` (`crates/text/Cargo.toml:15`). `inkra-editor` depends on
  `inkra-text` only (`crates/editor/Cargo.toml:9-10`).
- `EditorState::line` finds the caret's line through `line_for_offset` and
  `line_range` (`state.rs:244-246`). Both scan from byte 0
  (`crates/text/src/buffer/lines.rs:15-17`, `:25-33`). Plan 025 replaces
  them with `Buffer::line_at`, which costs the line's own length.
- The shared bindings sit in the `TextInput` context
  (`crates/app/src/keymap.rs:34-48`, `text_input::CONTEXT` at
  `crates/app/src/text_input.rs:34`). The editor, the name field and every
  picker field get them. No binding starts with `alt-`; the one binding with
  `alt` is `cmd-alt-enter` (`keymap.rs:59`).
- A key bound in `TextInput` must not also be bound per host:
  `no_key_is_bound_both_shared_and_per_host` fails
  (`crates/app/src/keymap/tests.rs:19-29`).
- The shared actions and their handlers live in
  `crates/app/src/text_input/actions.rs:10-64`. `move_by` sends a
  `LogicalMovement` through `InputHost::move_cursor` (`actions.rs:66-76`).
- The command palette hides every move and delete action by name
  (`crates/app/src/command_palette/commands.rs:9-12`).
- The mouse: `EditorView::on_mouse_down` focuses, follows a `⌘`-click on a
  link, then sets `is_selecting` and calls `move_to`
  (`crates/app/src/editor/mouse.rs:31-62`). It never reads
  `event.click_count`. `on_mouse_move` extends to the offset under the
  pointer while `is_selecting` holds (`mouse.rs:64-79`). The field is a
  plain `bool` (`crates/app/src/editor/mod.rs:76`).
- The name field and the picker fields have no mouse handler: neither
  `field.rs` nor `text_input.rs` mentions `mouse`.
- gpui counts clicks for us: `MouseDownEvent::click_count`
  (`vendor/zed-reference/crates/gpui/src/interactive.rs:159`). A test sends a
  double click with `cx.simulate_event(MouseDownEvent { click_count: 2, .. })`
  (`crates/app/src/shell/tests/separators.rs:119-131`).
- Concealment: `inkra-markdown` decides each `Conceal`'s `element` and
  `marker` while it parses (live-preview spec, D3). A conceal is active,
  hidden, only when its element does not touch the selection, ends included
  (`crates/markdown/src/reveal.rs:15-19`, live-preview spec D4). So the caret
  can enter a concealed marker's range: the element then touches the
  selection, its markers reveal, and `after_move` reshapes the line
  (`crates/app/src/editor/mod.rs:429-437`, live-preview spec D7). The caret
  is always on displayed text.

## What Zed does

All paths are under `vendor/zed-reference/crates/`.

- **The word rule is by character class.** `CharClassifier::kind_with`
  sorts a `char` into `Word` (alphanumeric, `_`, or a language word
  character), `Whitespace`, or `Punctuation`
  (`language/src/buffer.rs:611-618`, `:6265-6292`).
- **`⌥←` is `previous_word_start`** (`editor/src/movement.rs:256-281`). It
  walks back to the first place where the class changes and the right
  character is not whitespace, or to a line start. It crosses lines: from a
  line start it reaches the previous line's last word.
- **`⌥→` is `next_word_end`** (`movement.rs:435-459`), the mirror image.
- **A move collapses from the head.** `move_to_previous_word_start` calls
  `move_cursors_with` (`editor/src/navigation.rs:385-399`), which moves the
  head and collapses the selection there
  (`editor/src/selections_collection.rs:1105-1116`). The `select_to_*`
  twins move the head only (`navigation.rs:417-431`).
- **A word delete stops at a newline.** `delete_to_previous_word_start`
  uses `previous_word_start_or_newline` unless `ignore_newlines` is set
  (`editor/src/input.rs:984-1015`, `movement.rs:284-295`,
  `editor/src/actions.rs:261-267`). `adjust_greedy_deletion` then shortens
  the range at a run of two or more spaces or at a bracket
  (`movement.rs:299`).
- **A word delete groups with typing.** It runs in `Editor::transact`
  (`editor/src/editor.rs:8783-8794`), and the text buffer merges
  transactions closer than 300 ms (`text/src/text.rs:226-230`, `:294`).
- **The mouse selects by unit.** `SelectMode` is `Character`, `Word`, `Line`
  or `All` (`editor/src/editor.rs:466-471`). `begin_selection` maps
  `click_count` 2 to the word under the pointer, 3 to the line with its
  newline, 4 and more to the whole buffer (`editor/src/selection.rs:1241-1276`).
  The word is `surrounding_word`, which prefers the higher class on either
  side of the offset, so a word wins over a space
  (`language/src/buffer.rs:4495-4528`).
- **A drag after a double click extends by word.** `update_selection` snaps
  the head to the edge of the word under the pointer, and keeps the first
  word inside the selection (`editor/src/selection.rs:1345-1370`). The line
  mode does the same by lines (`selection.rs:1371`).
- **Zed's default keymap is not in the vendored checkout.** The checkout has
  no `assets/` directory, so this spec cites Zed's action names, not its
  macOS key bindings.

gpui itself has no word movement. Its input example binds the grapheme keys
only (`gpui/examples/input.rs:758-769`).

## Decisions

### W1. The word rule: UAX #29 word segments, punctuation skipped

A **word** is a UAX #29 word segment that holds at least one alphanumeric
`char` (`char::is_alphanumeric`). Every other segment, whitespace or
punctuation, is a **gap**. `unicode-segmentation` gives the segments through
`split_word_bound_indices`, whose iterator runs both ways
(`unicode-segmentation-1.13.3/src/word.rs:167`).

- `⌥→` puts the caret at the end of the first word that ends after it, on
  the caret's line. From inside a word, that is the word's own end. Gaps are
  skipped, punctuation included.
- `⌥←` puts the caret at the start of the last word that starts before it,
  on the caret's line.
- No word that way on the line: the caret goes to the line's edge.
- At the line's edge already, and not at the document's edge: the caret
  crosses the line break and applies the rule to the adjacent line only. If
  that line has no word, the caret stops at its far edge. So each blank line
  costs one press.
- The target passes through `Buffer::clip_grapheme` in the move's direction
  (`graphemes.rs:62-76`). UAX #29 word rules do not know grapheme
  `Prepend` characters, so a word bound can probably fall inside a grapheme
  cluster in rare scripts. The snap makes "never inside a grapheme" hold by
  construction.

The candidates:

- **UAX #29 segments (chosen).** The dependency is already there, and the
  rules suit prose: `don't`, `it’s`, `3.14`, `snake_case` and `U.S.A` are one
  word each (WB6-WB7, WB11-WB12, WB13a). A decomposed accent stays in its
  word (WB4).
- **Zed's three classes (rejected).** `'` is punctuation there, so `don't` is
  three stops for `⌥→` in Zed's rule and two words for a double click. A note
  app is mostly prose, and an apostrophe inside a word is common in English
  and French. Zed also treats a CJK run as one word, since every ideograph is
  alphanumeric (`buffer.rs:6266`).
- **Cocoa's behaviour (rejected as a rule, kept as the target feel).**
  NSTextView uses dictionary segmentation for CJK and Thai. Matching it
  needs ICU-class data, a heavy new dependency for one script family. The
  Latin-script feel is the same as W1 as far as this spec can tell; slice 1
  checks the table below against TextEdit by hand.

CJK under W1: UAX #29 puts each Han ideograph and each hiragana in a segment
of its own and keeps a katakana run together (WB13). So `⌥→` walks
`日本語のテキスト` as `日`, `本`, `語`, `の`, `テキスト`. That is finer than
Cocoa's words and coarser than nothing. Dictionary segmentation goes to
`docs/roadmap.md`, triggered by a user who writes CJK notes.

The table. `|` is the caret; each row starts from the same caret for both
keys. The segments were printed from `unicode-segmentation` 1.13.3 while
writing this spec.

| # | Text and caret | after `⌥→` | after `⌥←` |
|---|---|---|---|
| 1 | `he\|llo world` | `hello\| world` | `\|hello world` |
| 2 | `hello\| world` | `hello world\|` | `\|hello world` |
| 3 | `foo,\| bar` | `foo, bar\|` | `\|foo, bar` |
| 4 | `a\|  —  b` | `a  —  b\|` | `\|a  —  b` |
| 5 | `do\|n't stop` | `don't\| stop` | `\|don't stop` |
| 6 | `3.1\|4 apples` | `3.14\| apples` | `\|3.14 apples` |
| 7 | `e-ma\|il me` | `e-mail\| me` | `e-\|mail me` |
| 8 | `one\|⏎two three` | `one⏎two\| three` | `\|one⏎two three` |
| 9 | `one two⏎\|three` | `one two⏎three\|` | `one \|two⏎three` |
| 10 | `one\|⏎⏎two` | `one⏎\|⏎two` | `\|one⏎⏎two` |
| 11 | `日\|本語のテキスト` | `日本\|語のテキスト` | `\|日本語のテキスト` |
| 12 | `see\| **bold** now` (concealed) | `see **bold\|** now` | `\|see **bold** now` |

`⏎` is a line break. Row 7: a hyphen is a gap, so `e-mail` is two words, as
in Zed and, probably, in Cocoa. Row 10: the caret stops on the blank line.
Row 12 is W7's subject.

### W2. Where it lives: on `Buffer`, beside the grapheme steps

A new module `crates/text/src/buffer/words.rs` adds:

```rust
impl Buffer {
    pub fn previous_word_start(&self, offset: ByteOffset) -> ByteOffset;
    pub fn next_word_end(&self, offset: ByteOffset) -> ByteOffset;
    pub fn word_at(&self, offset: ByteOffset) -> Range<ByteOffset>; // W6, slice 3
}
```

The names follow Zed's (`movement.rs:256`, `:435`) and the module sits next
to `graphemes.rs`, one concern per module.

**Cost bound.** Each call segments the caret's line, and at a line edge the
adjacent line too: at most two lines, never the document. The line edges
come from the last `\n` before the offset and the first `\n` after it. If
plan 025 has landed, that is `Buffer::line_at`; if not, slice 1 takes
plan 025's `line_at` with it rather than a second private copy. Never
`EditorState::line`: today it scans from byte 0 (`state.rs:244-246`).

Rejected: **the logic in `EditorState`**. `inkra-editor` has no
`unicode-segmentation` dependency (`crates/editor/Cargo.toml:9-10`), so it
would gain one, and the double click in `app` needs `word_at` without an
`EditorState` move. The grapheme steps already set the pattern: the text
crate knows the text's units, the editor crate decides what a key does.

### W3. The movements: two variants, one collapse rule

`LogicalMovement` gains `WordLeft` and `WordRight`. `move_` maps them to
`buffer.previous_word_start(head)` and `buffer.next_word_end(head)`, with
`extend` as for every other variant.

**The collapse rule.** With a selection and no `extend`, a word move starts
from the selection's edge on its side: `WordLeft` from `selection.start()`,
`WordRight` from `selection.end()`. Then it moves one word. Two new arms in
`move_`, beside the `collapsing` arms of `Left` and `Right`
(`state.rs:231-232`).

Rejected:

- **From the head, as Zed** (`selections_collection.rs:1105-1116`). After a
  `⇧⌥→` selection, `⌥←` would move from the selection's end and land inside
  the selected text. The result depends on how the selection was made, which
  a writer does not track.
- **Collapse only, as `Left` and `Right` do.** The user pressed a word key;
  spending the press on a collapse the arrow key already offers is a wasted
  key. Cocoa probably moves from the edge as W3 does; slice 1 checks it in
  TextEdit and records any difference under "As built".

With `extend`, the head moves and the anchor stays, as `SelectLeft` does
today. A word selection can cross the anchor, as a grapheme selection can.

### W4. Deleting a word: `backspace_word` and `delete_word`, one undo step each

`EditorState` gains `backspace_word` and `delete_word`. Both reuse
`remove`'s shape (`state.rs:212-226`), with the range from W1:

- With a selection, they delete the selection, as `⌫` does.
- At a line start, `backspace_word` deletes the line break only. At a line
  end, `delete_word` deletes the line break only. This is Zed's
  `previous_word_start_or_newline` (`movement.rs:284-295`). A word delete
  never joins two lines and eats a word in one press.
- Otherwise `backspace_word` deletes from `previous_word_start(caret)` to
  the caret, and `delete_word` from the caret to `next_word_end(caret)`.

| Text and caret | key | result |
|---|---|---|
| `foo bar\|` | `⌥⌫` | `foo \|` |
| `foo bar \|` | `⌥⌫` | `foo \|` |
| `foo, \|` | `⌥⌫` | `\|` |
| `one⏎\|two` | `⌥⌫` | `one\|two` |
| `\|foo bar` | `⌥⌦` | `\| bar` |
| `foo\|⏎bar` | `⌥⌦` | `foo\|bar` |

**The undo rule.** A word delete is one undo step that merges with neither
neighbour: it goes through `edit_ungrouped`, so `Buffer::transact`
(`state.rs:77-82`). CLAUDE.md files a one-step gesture there, beside paste
and cut. After "type a sentence, `⌥⌫`, type a word", `⌘Z` removes the new
word, then restores the deleted one, then removes the sentence.

Rejected: **`transact_at`, merging with typing, as `⌫` and Zed do**
(`editor.rs:8783-8794`). A fast `⌥⌫` after typing would merge into the
typing's step, and one `⌘Z` would undo both. A word is a large unit to lose
by surprise. Three `⌥⌫` in a row are three steps; that is the price.

Rejected: **Zed's `adjust_greedy_deletion`** (`movement.rs:299`). It stops a
delete at a bracket or a run of spaces, a code concern. W1 already stops at
the line break.

### W5. The bindings: six keys in `TextInput`

| Key | Action | Handler |
|---|---|---|
| `alt-left` | `WordLeft` | `move_by(.., WordLeft, false, ..)` |
| `alt-right` | `WordRight` | `move_by(.., WordRight, false, ..)` |
| `alt-shift-left` | `SelectWordLeft` | `move_by(.., WordLeft, true, ..)` |
| `alt-shift-right` | `SelectWordRight` | `move_by(.., WordRight, true, ..)` |
| `alt-backspace` | `BackspaceWord` | `edit_buffer(.., backspace_word)` |
| `alt-delete` | `DeleteWord` | `edit_buffer(.., delete_word)` |

All six bind in the `input` context next to `left` and `right`
(`keymap.rs:34-48`), so the editor, the name field and every picker field
get them. The actions join `actions!(text_input, [...])` and `on_actions`
(`actions.rs:10-64`), and `is_hidden` in the command palette
(`commands.rs:9-12`).

The key names hold against gpui:

- `parse_internal` reads `alt` as the option modifier
  (`vendor/zed-reference/crates/gpui/src/platform/keystroke.rs:146-148`).
- `backspace`, `delete`, `left` and `right` are named keys
  (`keystroke.rs:422`, the list at `:459-462`).
- On macOS the arrow keys arrive as `left` and `right` with `alt` set from
  the option flag, and without the `function` flag that arrow keys carry
  natively (`vendor/zed-reference/crates/gpui_macos/src/events.rs:358`,
  `:361-363`, `:384-385`). `⌫` is `backspace` and `⌦` is `delete`
  (`events.rs:379`, `:390`).

Rejected: **`ctrl-alt-left` and friends for subwords**, as Zed has them.
Prose has no camelCase.

### W6. The mouse: double click selects a word, triple click a line

`on_mouse_down` reads `event.click_count` after the `⌘`-click branch
(`mouse.rs:43-56`), which stays first:

- **1**: unchanged, `move_to(offset, shift)`.
- **2**: selects `buffer.word_at(offset)`.
- **3 and more**: selects the logical line with its line break: from the
  line start to the next line's start, or to the end on the last line.

`word_at(offset)` returns, on the offset's line:

1. the word that holds the offset, ends excluded, if any;
2. else a word that ends or starts at the offset, the left one first. The
   hit test returns the nearest boundary (`mouse.rs:9-11`), so a click on a
   word's last half lands on its end; this rule still selects that word;
3. else the gap segment on the offset's right, such as a run of spaces, as
   Zed's `surrounding_word` does (`buffer.rs:4495-4528`);
4. else, on an empty line, the empty range at the offset.

`⇧` with a count of 2 or more acts as a count of 1: it extends by grapheme.
That keeps today's `⇧`-click and leaves Zed's `extend_selection` by unit
(`selection.rs:1154-1218`) for later.

**The drag after a double or triple click extends by unit**, as Zed
(`selection.rs:1345-1370`) and Cocoa do. The press stores the unit and the
first range in the drag state. On each move, the selection becomes the union
of the first range and the unit under the pointer: the head goes to the far
edge of that unit, and the anchor to the first range's other edge.

**How this sits on plan 021.** Plan 021 moves the drag's mouse-move
listener from the editor's `div` to the window, so a drag keeps following
the pointer outside the editor. It keeps `is_selecting: bool` and
`on_mouse_move`. This spec replaces the `bool` with
`selecting: Option<Drag>`:

```rust
enum Drag {
    Grapheme,
    Word(Range<ByteOffset>),
    Line(Range<ByteOffset>),
}
```

`on_mouse_move` keeps its early return on `None`, so plan 021's window
handler needs no change. `on_mouse_up` sets `None`. The snap itself is pure
and lives in `inkra-editor`: `EditorState::drag_to(buffer, first, unit,
offset)`, testable without a window.

Rejected:

- **No word drag; a drag after a double click extends by grapheme.** It is
  less code, but every macOS text view snaps, and a writer who double-clicks
  a word and drags expects whole words.
- **Select-all on a fourth click, as Zed does** (`selection.rs:1272-1276`).
  `⌘A` exists, and a fast fourth click that drops a line selection for the
  whole note surprises more than it helps.
- **The visual row for a triple click.** A soft-wrapped paragraph is one
  logical line; a writer means the paragraph.
- **Double click in the name field and pickers.** They have no mouse
  handler today. Adding one is its own change.

### W7. Concealed markers: the rule runs on the buffer text

The word rule sees the buffer, markers included. A marker is punctuation, so
it is a gap and W1 skips it. The caret stops where a word of the buffer
starts or ends. When that stop is inside or at the edge of a concealed
element, the element touches the caret, reveals, and the line reshapes
(live-preview spec D4, D7). No conceal-specific code is needed, and the
caret stays on displayed text by D4's invariant.

`see **bold** now`, all concealed, caret after `see` (W1, row 12):

- `⌥→` lands at `see **bold|** now`. The strong reveals; the caret is
  before the closing `**`, right after the word it passed.
- `⌥→` again lands at `see **bold** now|`, and the strong conceals again.
- `⌥←` from the end lands at `|now`, then at `**|bold**`, then at `|see`.

Rejected: **a conceal-aware step that treats active markers as absent**. It
needs the conceals in the move. They live in `inkra-markdown`, which
`inkra-editor` must not depend on (`CLAUDE.md`, Architecture). It would
mean a second word pass in `app` over display text, mapped back through
`DisplayLine`, for a gain W7 mostly gets for free.

**Open question: a wikilink with an alias.** `[[page|alias]]` shows `alias`
only. From `|[[page|alias]]`, `⌥→` stops after `page`, inside the hidden
target. The link reveals, so the caret is on displayed text, but the stop
is on a word the user did not see before the press. A second press reaches
`alias`. A markdown link `[t](url)` has the same case in its URL's words.
**Recommendation:** ship W7 as is, and look at it in the running app. If the
stop inside a hidden target hurts, add the conceal-aware step in `app` for
links only, with its own spec entry.

A double click on concealed text selects the buffer word under the pointer.
The selection touches the element, so its markers reveal and the text after
them shifts right under the pointer. The live-preview spec accepts the same
shift for a drag (its "Deferred" table).

### W8. Deviations from Zed

| Zed | Inkra | Reason |
|---|---|---|
| Three character classes (`buffer.rs:6265-6292`) | UAX #29 word segments (W1) | `don't` is one word; prose, not code |
| A CJK run is one word | One segment per ideograph (W1) | UAX #29's rule; dictionary words deferred |
| Moves cross any number of blank lines (`movement.rs:256-281`) | At most one line crossed per press (W1) | The cost stays within two lines (W2) |
| A move collapses from the head (`selections_collection.rs:1105-1116`) | From the edge on the move's side (W3) | The result does not depend on how the selection was made |
| A word delete groups with typing (`editor.rs:8783-8794`) | One undo step of its own (W4) | One `⌘Z` must not take typing and a word together |
| `adjust_greedy_deletion` (`movement.rs:299`) | None (W4) | A code concern |
| Subword moves | None (W5) | Prose has no camelCase |
| Four clicks select all (`selection.rs:1272-1276`) | Three and more select the line (W6) | `⌘A` exists |
| `⇧` with a double click extends by word (`selection.rs:1154-1218`) | Extends by grapheme (W6) | Deferred until asked |

## Tests

`crates/text`, in `crates/text/src/buffer/tests.rs` beside the grapheme
tests:

- One test per row of W1's table, on `previous_word_start` and
  `next_word_end` directly, plus the document edges: both return their
  input at 0 and at `len`.
- `word_at`: inside a word, at a word's end, on a run of spaces, on an empty
  line, on `don't`.
- A cost test, as other line-local lookups are tested: on a buffer of
  100 000 lines, a word step on the last line touches that line only. If no
  counter exists to observe it, slice 1 records the bound in a bench
  instead (`crates/text/benches/buffer.rs`).

A proptest in `crates/text/tests/buffer_proptest.rs`, on its
`text_strategy`:

- For every char-boundary offset, `next_word_end` lands on a grapheme
  boundary and is greater than the offset unless the offset is `len`.
  `previous_word_start` lands on a grapheme boundary and is less than the
  offset unless the offset is 0. So repeated presses always make progress
  until the edge.
- The result never leaves the offset's line and its adjacent one.

`crates/editor`, in `crates/editor/src/state/tests.rs`, in its style:

- The collapse rule from both selection directions (W3).
- `extend` keeps the anchor and can cross it.
- Each row of W4's table, and its undo: `⌥⌫` after typing undoes alone.
- `drag_to` with a word unit, both directions from the first word, and with
  a line unit.

`crates/app`:

- One `simulate_keystrokes` test per binding of W5, in
  `crates/app/src/text_input/tests.rs`: the four moves on the editor host,
  the two deletes on the name field host, so each host is reached.
- The keymap test already guards against a double binding
  (`keymap/tests.rs:19-29`).
- In a new `crates/app/src/editor/tests/clicks.rs`: a double click selects a
  word, a triple click selects a line with its break, a double click then a
  drag extends by word. Events go through `cx.simulate_event` with a
  `click_count`, as `shell/tests/separators.rs:119-131` does.
- A double click on a concealed `**bold**` selects `bold` and reveals the
  markers, beside `editor/tests/conceal.rs`.

## Risks

- **Cocoa parity is unverified.** W1's Latin-script rows, W3's collapse and
  W4's deletes follow what this spec believes TextEdit does. Nobody checked
  it in TextEdit while writing the spec. Slice 1 and slice 2 check their
  rows by hand and record differences under "As built".
- **A very long line.** A note of one 1 MB line pays a full UAX #29 pass per
  word key. That is one scan, within the rule, but probably slow. Segmenting
  only from the caret to the line's edge would bound it by the distance
  moved; profile before adding it.
- **A word bound inside a grapheme.** W1's snap through `clip_grapheme`
  covers it, and the proptest guards it.
- **The drag's first range goes stale.** An external reload during a held
  drag moves the text under the stored range. `drag_to` clips both ends, so
  nothing panics; the selection may be off until the button lifts.
- **Plan 021 may not land first.** Slice 3 works without it: the drag state
  replaces the `bool` either way. The drag outside the editor is then still
  missing, as today.
- **`state.rs` is 326 lines.** Slices 1 to 3 add perhaps 60 lines. If it
  nears 400, the drag snap moves to `crates/editor/src/state/drag.rs`, an
  `impl EditorState` block of its own.

## Plan

Each slice is one PR of about 150 lines, builds, passes
`./scripts/check.sh`, and diffs `./scripts/test-names.sh`. Each touches
`text` or `editor` logic, so each runs `cargo mutants --in-diff` on its
diff, per CLAUDE.md.

1. **Move by word.** Data: `crates/text/src/buffer/words.rs` with
   `previous_word_start` and `next_word_end` (W1, W2), and plan 025's
   `line_at` if it has not landed. Behaviour: `WordLeft`, `WordRight` in
   `LogicalMovement` with the collapse rule (W3); the actions `WordLeft`,
   `WordRight`; `alt-left`, `alt-right`; the palette hide list. Tests:
   W1's table, the proptest, the collapse tests, two `simulate_keystrokes`
   tests. Check W1's rows in TextEdit.
2. **Select and delete by word.** Data: none new. Behaviour:
   `backspace_word`, `delete_word` (W4); `SelectWordLeft`,
   `SelectWordRight`, `BackspaceWord`, `DeleteWord`; their four bindings.
   Tests: `extend`, W4's table and its undo, four `simulate_keystrokes`
   tests.
3. **Double and triple click.** Data: `Buffer::word_at`; `Drag` in place of
   `is_selecting` (W6). Behaviour: `on_mouse_down` by `click_count`,
   `EditorState::drag_to`, the snapping `on_mouse_move`. Tests: `word_at`,
   `drag_to`, `editor/tests/clicks.rs`, the concealed double click. Render
   the editor scene after it and look at a double-clicked word. Builds on
   plan 021 when it has landed.
4. **Close.** Mark this spec implemented, with "As built". Add CJK
   dictionary words, the conceal-aware step (W7's open question) if not
   built, and `⇧`-double-click to `docs/roadmap.md`, each with its trigger.
