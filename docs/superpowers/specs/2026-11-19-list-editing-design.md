# List editing: `↩`, `⇥`, `⇧⇥` and `⌫` in lists and quotes

Date: 2026-11-19. Status: **approved**, revised after a red-team review (History). L1 is built;
L2 is under review.
The owner approved the scope ("chantier A") on 2026-11-19, and decided every Open question the same
day.

## Why

Typing a list is poor, because the editor has no list-aware command. Checked against `main` at
`f2e7366f`:

- `↩` inserts a bare `\n` (`newline`, `crates/app/src/editor/actions.rs:138-141`). The user types
  every marker by hand.
- `tab` and `shift-tab` have no binding (`crates/app/src/keymap.rs:39-71`). Both reach the platform
  input and insert `\t`, the audit found. A `⇧⇥` that writes a byte is a data bug.
- `⌫` at an item's text start deletes one byte. `- abc` becomes `-abc`, and `- [ ] abc` becomes
  `- [ ]abc`: the markup turns to text.
- Typed by hand, a nested marker passes through `CommonMark` states that look broken (D8).

## Decisions taken

By the owner, on 2026-11-19.

1. **Chantier A is built.** `↩` continues the list: the next number, a task as an unchecked task.
   `↩` in the middle of an item splits it. `↩` on an empty item leaves the list, or outdents it one
   level when it is nested. `⇥` and `⇧⇥` indent and outdent the whole item. `⌫` at the text start
   removes the markup. `↩` inside `> ` continues the quote.
2. **Chantier B is out of scope.** Changes to the reveal rule are in Deferred.
3. **The parse stays `CommonMark`** (Open question 1). The commands still read a marker on a line
   where no item opens, through the one-line fallback `⌘L` uses (D8).
4. **A marker that changes width shifts its item's other lines** in the same batch, so the
   structure holds (Open question 9, D7).
5. **Renumbering follows CodeMirror**: only while the old numbers ran in sequence (Open question 2,
   D6).
6. **`↩` at the end of an item with children** writes a new sibling right after the marker line's
   text; the children become its children, as in Obsidian (Open question 3).
7. Open questions 4–7 are decided as recommended: `⌫` on an empty item deletes its line; `⇧⇥` works
   in place; `⌫` at a bullet's text start deletes the marker; `⇥` on a first item does nothing.

## Terms

This spec reuses the task boxes spec's terms (`2026-11-16-task-boxes-design.md`, Terms): **gutter
item**, **marker line**, **item part**, **text start**, **content start**, **opener**, **leaf text
start**, **atom**, **fallback**. It adds:

- **list command**: `↩`, `⇥`, `⇧⇥` or `⌫` as this spec defines them, as opposed to the **plain
  key**, the behaviour on `main`.
- **held line**: a line that a gutter item holds and on which no item opens. Its item part has no
  mark.
- **list**: the run of items one `Start(List)` event opens. pulldown-cmark opens a new list when the
  marker kind changes, and when a block outside the list ends it.
- **sibling**: an item of the same list. The **previous sibling** is the nearest one above.
- **holder**: the innermost gutter item that holds a line: the last item whose marker lies before
  the line, walked up through its parents until one ends past the line's start (D1).
- **caret item**: the item that opens on the caret's line, the innermost when several do, else the
  line's holder.
- **fallback item**: an empty marker that the fallback reads on a line where no item opens (D8).
- **empty item**: a gutter item, or a fallback item, whose marker line has only blanks past its text
  start, and that holds no other line. `- `, `1. ` and `- [ ] ` are empty items.
- **quote part**: the gutter's bytes from the line's start through its last `>` and the one blank
  after it (`Gutter::quote_end`, `crates/markdown/src/gutters.rs:66-68`).
- **empty marker line**: a marker line with only blanks past its text start. Its item is an empty
  item when it holds no other line.
- **container prefix**: what follows every `\n` a command inserts: the quote part and the item part
  of the containers that hold the new line (D2).
- **block**: an item's marker line and every line it holds, its children included: from the line
  start of its marker to its range's end.
- **marker kind**: a bullet's byte (`-`, `*`, `+`), or an ordered marker's delimiter (`.`, `)`).
- **renumber**: rewrite the digits of the ordered siblings after an edited item (D6).
- **width shift**: the blanks a batch adds to, or removes from, an item's other lines when its
  content column moves (D7).

## Context

- `toggle_task` is the precedent (`actions.rs:174-183`). It reads the cached parse, passes it with
  the text and the selection to `cycle_tasks` (`crates/markdown/src/cycle.rs:38-72`), and applies
  the returned hunks as one `Diff` through `EditorState::apply_diff`. `inkra-markdown` computes the
  edit from the parse, the app binds the key, and `inkra-editor` reads no markdown (task boxes
  spec, D6).
- `Buffer::apply_diff` (`crates/text/src/diff.rs:179-190`) is one undo step that merges with no
  typing on either side. It edits nothing when the buffer's version is not the diff's base.
  `EditorState::apply_diff` maps the selection through it (`crates/editor/src/state.rs:296-319`): a
  caret lands after an insertion at its offset, and at the start of a deleted range that holds it.
- `Structure::items` holds each item's marker only (`crates/markdown/src/structure.rs:14-24`). It
  has no end, no parent and no list.
- The shared keys bind once, in the `TextInput` context: `backspace` among them (`keymap.rs:41`,
  handler at `crates/app/src/text_input/actions.rs:29-35`). A host binds only its own keys, and
  `keymap/tests.rs` fails on a key bound in both (`.claude/rules/editor.md`).
- `enter` binds `Newline` in the editor context and `Confirm` in the field context
  (`keymap.rs:60`, `:87`). `Newline` is on the palette's hide list
  (`crates/app/src/command_palette/commands.rs:28`).

### Is the parse fresh at keystroke time?

Yes, in every path a user reaches.

- `edit_buffer` calls `after_edit`, and the editor's reshapes (`.claude/rules/editor.md`).
  `reshape` calls `refresh` first (`crates/app/src/editor/mod.rs:169-180`). `refresh` parses when
  the buffer's version moved, and records it in `parsed_at` (`mod.rs:135-161`).
- A write that bypasses `edit_buffer`, such as a reload, reaches `refresh` through
  `on_buffer_changed` (`mod.rs:164-167`).

So a list command reads `EditorView::markdown`, the parse the previous edit already paid for, and
runs no pass over the note. It stamps its `Diff` with `parsed_at`, not with the buffer's version
(D9). A parse that lags the text makes the command edit nothing: the handler checks `parsed_at`
before it calls `continue_list`.

A test presses `enter enter` and `enter b enter` with no frame between the keys
(`editor/tests/list_enter.rs`). It fails when `reshape` skips `refresh`, so a parse that lags one
frame would show as a dropped second key.

## What the references do

### Zed

Zed reads lists by line prefix, from the language config, not from a parse.

- The markdown config lists the prefixes: `unordered_list = ["- ", "* ", "+ "]`, `ordered_list`
  with the pattern `(\d+)\. ` and the format `{1}. `, and `task_list` with the prefixes `- [ ] `,
  `- [x] `, `- [X] ` and the continuation `- [ ] `
  (`vendor/zed-reference/crates/grammars/src/markdown/config.toml:25-27`). The config types are in
  `crates/language_core/src/language_config.rs:213-229`.
- `newline` adds a list delimiter only for an empty selection with `extend_list_on_newline` set
  (`crates/editor/src/input.rs:631-648`).
- `list_delimiter_for_newline` (`input.rs:2678-2796`) skips the line's leading whitespace and
  matches the longest prefix. With content after the marker and the caret past the prefix, it
  continues: the same bullet, `- [ ] ` for any task, the number plus one (2731-2733, 2772-2777).
  With the caret right after the prefix of an empty item, it clears the line at the top level
  (`ClearCurrentLine`), or outdents it by one tab stop and keeps the marker, `1.` for a number
  (`UnindentCurrentLine`, 2735-2744, 2780-2789). A caret before or inside the prefix inserts a plain
  newline.
- The newline edit itself is built at `input.rs:662-741`.
- Its tests pin this: `test_newline_unordered_list_continuation`
  (`crates/editor/src/editor_tests.rs:46476-46642`), task and ordered cases at 46341 and 46645. Case
  3 splits `- it|em` into `- it` and `- em`; cases 6 and 7 outdent, then clear, an empty nested item.
- `tab` indents the line by one indent unit when `indent_list_on_tab` is set and the line has a
  list prefix, wherever the caret is (`crates/editor/src/editor.rs:5436-5454`,
  `is_list_prefix_row` at `input.rs:2300-2364`). `indent_selection` shifts every selected row by
  `tab_size` (`editor.rs:5575-5605`). Markdown's `tab_size` is 2 (`config.toml:31`).
- `backtab` calls `outdent` (`editor.rs:5354-5364`). It writes nothing.
- Zed has no list-aware `⌫`, and it does not renumber.

### CodeMirror, `@codemirror/lang-markdown`

Read from `src/commands.ts` on GitHub, 2026-11-19.

- `insertNewlineContinueMarkup` works from the syntax tree. It acts on an empty selection only,
  and stops at a `FencedCode` node, so a code block never continues a list.
- It trims the blanks before the caret and inserts a line break and the markup. A number takes the
  next value, and `renumberList` rewrites the following siblings while each was the previous plus
  one. A task's new box is unchecked.
- On a line that is not the item's first, it writes aligned blanks and no marker.
- On an empty item it removes the markup at the top level. Nested, it replaces the marker with the
  parent's marker, the number incremented. With `nonTightLists` (default `true`), an empty second
  item of a tight list gets a blank line instead.
- On an empty `>` line after another, it clears both and leaves the quote.
- `deleteMarkupBackward` deletes the extra blanks after a marker first. Right after the marker of an
  item that is not the list's first, it replaces the marker with spaces, so the text keeps its
  column. A second `⌫` deletes one level of markup or indent, a `>` included.

### Obsidian

From knowledge; Obsidian is closed source.

- `↩` continues a bullet, a number plus one, and a task as `- [ ] `. Sure.
- `↩` on an empty item removes the marker, and outdents it first when nested. Likely; unsure of
  the nested case.
- `↩` at the end of an item with children writes a new item that takes the old item's children.
  The owner states it (decision 6).
- `⇥` and `⇧⇥` indent and outdent an item with its children. By default it indents with a tab
  character ("Indent using tabs"). Sure of the move, unsure of the default today.
- It renumbers an ordered list after an insertion. Unsure.

## D1. The data

`Structure::items` becomes a vector of `Item`. The collector fills it in the parse's one pass
(`structure.rs:33-51`), so the parse gains no pass.

```rust
pub struct Item {
    /// Its marker, the bullet or the first digit.
    pub marker: ByteOffset,
    /// Its range's end, without the last line ending pulldown-cmark gives it: its block holds the
    /// lines that start before it.
    pub end: ByteOffset,
    /// Its parent item's index in `Structure::items`; `None` for a top-level item.
    pub parent: Option<u32>,
    /// Its list: the index of its `Start(List)` among the note's lists, in document order.
    pub list: u32,
}
```

- A `Start` event carries its element's whole range, so `end` costs nothing. `end` drops the range's
  last line ending, so `parse(t)` equals `parse(t + "\n")` and no offset lies past the text.
- The collector keeps two stacks, of open items and of open lists, each entry with its range's end.
  At each `Start(Item)` or `Start(List)` it pops the entries that end at or before the new marker,
  not the new range's start, since an item's range can start a line early;
  the tops are the parent and the list. That needs no `End` event, and `Collector::start` already
  sees every `Start` (`structure.rs:33-51`).
- Siblings share `list`, not merely `parent`. `1. a\n\npara\n\n2. x` is two lists, and in
  `> - a\n\n- b` the second item is the first of its own list.
- The previous sibling of item `i` is the nearest item above with `i`'s `list`, found by walking
  `j = i - 1` and then up its parents. Reaching `i`'s parent means there is none. That costs the
  depth.
- The next siblings of item `i` are the items after `i`'s `end` with `i`'s `list`, until the list's
  last item. Only renumbering and the width shift walk them.
- The holder of a line starting at `s`: the last item whose marker lies before `s`, then up its
  `parent` chain until an item's `end` lies past `s`. In `- a\n  - b\n\n  c`, `c`'s holder is `a`,
  not `b`. The caret line's gutter is found by one binary search over `gutters`, as
  `Markdown::task_at` does (`gutters.rs:109-120`), and the item by one more over `items`.

The readers of `Structure::items` change with it:

- `cycle.rs:120-123` reads `.marker`.
- `cycle.rs:214-215` maps the markers through the splices and binary-searches the copy's items by
  `.marker` (`binary_search_by_key`).
- `parse.rs:152` passes `structure.items()` to `Items::gutters`, which indexes it by marker. It takes
  `&[Item]` and reads `.marker`, so no projected `Vec<ByteOffset>` is built.
- `items.rs:18` and `items.rs:41` name `structure.items` in doc comments; they keep their meaning.
- `innermost_item` reads the last item's `.marker` (`structure.rs:82-90`).

## D2. `↩`

`continue_list(text, markdown, caret) -> Option<ListEdit>`. `None` means no list rule applies, and
the app inserts the plain `\n` as today. Every list command returns a `ListEdit` (D9): its hunks,
and the caret's offset in the edited text.

### The container prefix

**The invariant.** Every `\n` a command inserts is followed by the **container prefix** of the
line it starts: the bytes that keep that line inside every container that holds it. Rules 2, 3, 6,
7 and 8, the lazy-line indent and D5's moved lines all take their inserted text from one function,
`Line::container_prefix(column)`. No rule writes a quote marker or an indent of its
own.

- **The quote part.** One `>` per leading quote that holds the new line, outermost first, each
  followed by one space. The `>`s are copied from the nearest line at or above that shows all of
  them: the caret's line, or its holder's marker line. A lazy line shows fewer, so it is never the
  source.
- **The item part.** Spaces from the quote part's end to the holder's content column (task boxes
  spec, D11: one blank slot per holding item). With no holder, none.
- **A blank line** takes the quote part alone, with its trailing space trimmed: `>`. A `>` line
  keeps the quote open, and a blank line needs no indent to stay in an item.

So the three failures of `e30bd84a` cannot occur: `> para\n> - |abc` gives `> para\n>\n> - abc`,
`> - a\n>   |b` gives `> - a\n>\n>   |b`, and `> - a|\nb` gives `> - a\n> - |\n>   b`.

### The rules

First match wins.

1. **A selection, or marked text, takes the plain key.** Zed and CodeMirror do the same. The
   selection is replaced by `\n`, as on `main`.
2. **A caret on a held line, or on any line inside a code block that a quote or an item holds.**
   The edit replaces the blanks just before the caret, back to the text start, with `\n` and the
   container prefix of the caret line's holder. No marker is written.
   - `- a\n  b|` gives `- a\n  b\n  |`, a new line of the same paragraph.
   - A code line also copies its own blanks between the holder's content column and its leaf text
     start (task boxes spec, D11), so the code goes on. A task's fenced code,
     `- [ ] a\n  ```\n  x|`, gives a new code line at column 2, the task's content column, not at
     its hang column 6. An indented code block in an item, `- a\n\n      x|`, keeps its six blanks.
   - Quoted code, `> ```\n> x\n> |`, gives `> ```\n> x\n>\n> |`: a line in a code block never leaves
     its quote (rule 8 does not apply).
   - With the caret at the text start, only the container prefix precedes it. The line the caret
     leaves becomes a blank line, so it keeps the trimmed quote part alone: `> - a\n>   |b` gives
     `> - a\n>\n>   |b`.
   - A lazy line's holder is the item it continues, so `> - a\nb|` gives `> - a\nb\n>   |`.
   - A lazy line shows no `>`, so the line it leaves at its text start takes the container prefix's
     quote part, trimmed: `> - a\n|b` gives `> - a\n>\n>   |b`, and `> a\n|b` gives `> a\n>\n> |b`.
   - When the new line is blank, a lazy line right below the caret's would follow a blank line and
     leave the item, so it takes the container prefix in place of its leading blanks, as in rule 6:
     `- a\n  b|\nz` gives `- a\n  b\n  |\n  z`.
3. **A caret on a marker line whose text opens a fence.** The new line takes the container prefix
   of the caret item itself. `- ```rust|` gives `- ```rust\n  |`. `CodeBlock` does not tell a fence
   from indented code, so an indented code block on a marker line takes the rule too:
   `-     code|` gives `-     code\n  |` (Known limits).
4. **A code block outside every item and quote** takes the plain key.
5. **A caret on an empty item's line, at or past its text start** (D3). An empty item holds no
   other line; a fallback item counts (D8).
6. **A caret on a marker line past its text start, or at the text start of a marker line with no
   text.** The second case is an empty marker line that holds lines, which decision 6 creates
   (`- a\n- |\n  - c`). The edit replaces the blanks just before the caret, back to the text start,
   with `\n` and the **continuation**. The blanks just after the caret go too, so the text that
   moves starts at the new text start: `- a|    b` gives `- a\n- |b`, not indented code, and
   `- b| c\n  - a` gives `- b\n- |c\n  - a`, with no width shift. The continuation is:
   - the container prefix of the caret item's parent, its holder;
   - spaces from there to the caret item's marker column, so `- - a` continues as `  - `;
   - the marker: the same bullet byte, or the number plus one with the same delimiter, its digits
     zero-padded to the old count (`01.` gives `02.`);
   - the blanks between the marker and the content start, copied, or one space when the marker
     line has five columns or more (`CommonMark` 5.2, as `checkbox_at` counts them,
     `cycle.rs:148-162`);
   - for a task, `[ ] ` and nothing else: a checked task continues unchecked.

   The text after the caret moves to the new item: that is the split. `- it|em` gives
   `- it\n- |em`. Every line the old item held after the caret moves with it, and stays held:
   - its children become the new item's children (decision 6): `- a|\n  - c` gives
     `- a\n- |\n  - c`;
   - a held line stays held, now by the new item;
   - a lazy line is part of the item's text, so it moves too. When the new marker line is empty, a
     lazy line would leave the list, so the batch writes the new item's container prefix at its
     start: `- a|\nb` gives `- a\n- |\n  b`, and `> - a|\nb` gives `> - a\n> - |\n>   b`;
   - blank lines between the caret's line and the moved lines stay above the new marker line, since
     an item may begin with at most one blank line (`CommonMark` 5.2). With the caret at the end of
     its line's text and only blank lines before the next held line, the continuation goes after
     those blank lines: `- a|\n\n  b` gives `- a\n\n- |\n  b`, and `b` stays in the list. The blanks
     before the caret still go, as a second hunk: `- a |\n\n  b` gives `- a\n\n- |\n  b`;
   - when the new marker's content column differs from the old (`9.` to `10.`), the moved lines take
     the width shift (D7). `9. a|\n   - c` gives `9. a\n10. |\n    - c`.

   The caret lands at the new item's text start.
7. **A caret at a marker line's text start, with text after it.** The continuation would leave an
   empty item above the text, and an empty item cannot interrupt a paragraph (D8). The edit writes
   at the marker line's start, so the old line keeps its own prefix after the inserted `\n`:
   - when the caret item is not its list's first item, an empty sibling above: the continuation and
     a `\n`. The new item takes the caret item's number, and the caret item and those below
     renumber (D6). `- a\n- |b` gives `- a\n- \n- |b`.
   - when it is its list's first item, a blank line above: the trimmed container prefix of the
     marker line's quote part and a `\n`. `para\n- |abc` gives `para\n\n- |abc`, `- p\n  - |abc`
     gives `- p\n\n  - |abc`, and `> para\n> - |abc` gives `> para\n>\n> - |abc`. An empty item
     there would read as `p` with a setext underline, or as paragraph text.

   The caret stays on the text.
8. **A caret on a quote line with no item part, outside every code block.** The new line takes the
   container prefix: `> a|` gives `> a\n> |`. An empty quote line, blanks only past its quote part,
   loses its innermost `>` and the blank after it instead: `> > |` gives `> |`, and `> |` gives an
   empty line, so the quote ends. The caret lands at the line's new end.
   - On a lazy quote line, which shows fewer `>` than its `Gutter::depth`, the new line takes
     `Gutter::depth` quotes, their `>`s copied from the nearest line above that shows them all, as
     the container prefix takes them: `> a\nb|` gives `> a\nb\n> |`. A lazy line is never empty,
     so the exit does not apply to it.
   - When the new line is blank, a lazy line right below takes the container prefix, as in rule 2:
     `> a|\nz` gives `> a\n> |\n> z`.
9. **A caret before the text start** of a marker line, in a revealed item part, takes the plain key.
   `|- a` moves the item down a line, as in Zed (case 8).
10. **A line holding an opener past its text start** takes the plain key. In `- > - a` the inner
    item is no gutter item; continuing the outer item there would be a guess.

Quote and list combine through the container prefix: `> - a|` gives `> - a\n> - |`, and
`> 1. [x] a|` gives `> 1. [x] a\n> 2. [ ] |`.

An ordered continuation renumbers the siblings below it (D6).

### Why an empty marker line that holds lines continues (P0, P1)

Decision 6 turns `- a|\n  - c` into `- a\n- |\n  - c`: an empty marker line that holds `c`. D3's
exit there would delete the marker and orphan `c`, which `CommonMark` then reads as a top-level
item; P0 forbids it. So such a line is no empty item, and rule 6 continues it: `↩` gives
`- a\n- \n- |\n  - c`, and the line it leaves is a true empty item, which a further `↩` removes.
`⌫` on it deletes the line and gives the held lines to the item above (D5, rule 1), so `↩` then `⌫`
restores `- a\n  - c` (P1).

## D3. `↩` on an empty item

An empty item holds no other line (Terms); one that holds lines falls under D2's rule 6.

- **Top level**: the edit deletes the item part's bytes and any blanks after them, from the item
  part's start to the line end. The quote part stays. So `- a\n- |` gives `- a\n|`, and the list
  ends. `> - |` gives `> |`, and the quote goes on: one level per `↩`.
- **Nested**: the item outdents one level, as `⇧⇥` does (D4). `- a\n  - b\n  - |` gives
  `- a\n  - b\n- |`. A second `↩` then leaves the list.
- A fallback item is nested when its line has a holder (D8).
- The caret lands at the line's new text start: the quote part's end at the top level, the
  outdented item's text start when nested.

Zed and CodeMirror agree on both cases. CodeMirror's `nonTightLists` blank line is not followed: a
note app's lists are tight by habit (Deviations).

## D4. `⇥` and `⇧⇥`

Two editor actions, `editor::IndentItem` on `tab` and `editor::OutdentItem` on `shift-tab`, in the
editor context. Both go on the palette's hide list: from the palette, they would act on whatever
line the caret sits on.

`indent_items(text, markdown, selection) -> Option<ListEdit>` and `outdent_items(...)`. `None`
means the selection touches no item; a `ListEdit` with no hunks means it touches one and nothing can
move. `⇥` and `⇧⇥` insert no `\n`, so the container prefix's invariant (D2) holds trivially.

**The items.** The lines the selection touches, by `⌘L`'s rule: the first, then each later line
that starts before the selection's end (task boxes spec, D6, (H)). Each line gives its caret item,
on a marker line or a held line. A line inside a code block gives none, so a caret or a selection
inside an item's code never moves the item. An item whose ancestor is also given moves with that
ancestor only. Consecutive siblings among the given items form a **run**.

**`⇥` on a run.** The run's first item needs a previous sibling, in its own list, which becomes the
run's parent.

- The shift is that sibling's content column less the first item's marker column, in columns from
  the line's start. So the run's markers land on the new parent's content column, which `CommonMark`
  reads as a child (5.2): `- a\n- b` gives `- a\n  - b`, and `10. a\n11. b` gives
  `10. a\n    1. b`.
- The edit inserts that many spaces on every line of every block in the run, after the line's
  quote part. A blank line takes none, so no line gets trailing blanks. A lazy line takes them too
  and stays a continuation. A held code line takes them too, so the code stays in the item.
- **The marker kind.** When the new parent already has children, each moved marker takes the kind
  of its last child, so the run joins that list: a bullet takes its byte, a number takes the last
  number plus one, then counts up. Else a bullet keeps its byte and a number restarts at `1` with
  its own delimiter. A marker whose width changes takes the width shift (D7).
- A run whose first item has no previous sibling does not move. Nothing else is written: a first
  item cannot become a child (decision 7). In `> - a\n\n- b`, `b` is the first of its list, so `⇥`
  writes nothing; a shift there made indented code.
- A run whose previous sibling is an empty item above a blank line does not move either, as decision
  7 reads it: the parse ends that item at the blank line, so it can hold no child. `- a\n- \n\n- c|`
  and `- a\n- [ ] \n\n- c|` with `⇥` write nothing (owner decision, 2026-11-25).
- `⇥` on an empty fallback item writes nothing: its marker already sits where the parse will read
  a child once text arrives.

**`⇧⇥` on a run.** The run's items need a parent; they become its siblings, right after it in the
source.

- The shift is the first item's marker column less the parent's marker column. The edit removes
  that many columns of blanks from the start of every line of the run's blocks, after the quote
  part. A line with fewer leading blanks, such as a lazy line, loses all it has. When the cut falls
  inside a tab, that line's leading blanks are first rewritten as spaces.
- **The marker kind.** Each outdented marker takes the parent's kind: the parent's bullet byte, or
  the parent's number plus one, then counting up.
- **The adopted siblings.** The siblings after the run stay in place in the source, and become the
  last outdented item's children (decision 7). The same batch makes them parse as its children:
  - their blocks take the width shift to the new parent's content column (D7);
  - an ordered run of them renumbers from `1`, since a list that starts past `1` cannot interrupt
    the new parent's paragraph. `- a\n  1. b\n  2. c` with `⇧⇥` on `b` gives `- a\n- b\n  1. c`.
- A top-level item does not move, and `⇧⇥` writes nothing. Leaving the list is `⌫`'s job (D5).
- An empty fallback item outdents to its holder's marker column with the holder's kind (D8).

**Off a list.** `⇧⇥` never inserts a byte, anywhere. `⇥` falls back as follows:

- a caret on a line with no caret item, or inside any code block, an item's included: insert `\t`,
  the plain key, since code wants its tab;
- a selection that touches no item: nothing, so a `⇥` never replaces selected text.

**Depth.** No limit is set. Each level costs one slot in the gutter (task boxes spec, D11).

**The caret and the selection** keep their place in the text. `apply_diff` maps them: an insertion at
a line's start moves a caret on that line to the right.

An ordered item that changes level renumbers the siblings it leaves and the ones it joins (D6).

## D5. `⌫` at the text start

`remove_markup(text, markdown, caret) -> Option<ListEdit>`, for an empty selection with the caret
at a marker line's text start, a fallback item's included. Any other `⌫` takes the plain key, the
D2 atom included: a held line's `⌫` still joins it to the line above (task boxes spec, D2). One step
per press, first match wins:

1. **An empty marker line**, whether its item holds other lines or not (`- |`, `1. |`, `- [ ] |`,
   and the `- |` of `- a\n- |\n  - c`): the edit deletes the line's item part, its trailing blanks,
   and the `\n` before the line, so the caret lands at the previous line's end. The item's held
   lines, children included, stay in place and now belong to the item above: its previous sibling,
   else its parent. They take the width shift to that item's content column (D7), so they stay
   held. An ordered child list that starts past `1` renumbers from `1`, as D4 does for adopted
   siblings, since it cannot interrupt the new holder's paragraph: `- a\n- |\n  3. c` gives
   `- a|\n  1. c`. So `↩` then `⌫` restores the text and the caret, whether `↩` continued a plain item or one
   with children (P1): `- a\n- |\n  - c` gives `- a|\n  - c`. On a note's first line, only the item
   part goes, and its held lines take no shift. A quoted empty item keeps its quote part:
   `> - a\n> - |` gives `> - a|`.
2. **A task**: the checkbox and the one blank after it go. `- [ ] abc` gives `- abc`, and
   `1. [x] a` gives `1. a`.
3. **Any other item**, top level or nested: the marker and the blanks up to the text start go. The
   bytes before the marker stay, the quote part and the ancestors' indent. So `- a\n- |b` gives
   `- a\n|b`, a lazy line of `a`'s paragraph, which D11 already paints at `a`'s text. `- a\n  - |b`
   gives `- a\n  |b`, a held line of `a`. A top-level first item gives a paragraph. A further `⌫`
   reaches the atom or the plain key and joins the line above (decision 7). The item's own held
   lines and children stay where they are; they now follow the line as continuation lines.

A removed ordered marker renumbers the siblings below it (D6).

**The seam.** `backspace` binds once, in `TextInput`, and the field must keep the plain key. So the
editor does not bind it a second time. `InputHost` gains one method beside `snap`:

```rust
/// The edit `⌫` makes instead of deleting a grapheme, as the text was last parsed.
fn backspace_markup(&self, cx: &App) -> Option<(Version, ListEdit)> {
    None
}
```

The shared handler applies the hunks as a `Diff` at the returned version through `apply_diff`, then
places the `ListEdit`'s caret as D9 does, with `no_snap`. With `None` it calls
`EditorState::backspace` as today. `EditorView`'s returns `parsed_at` and `remove_markup`'s
`ListEdit` (D9). `NameField` takes the default. The trait already has two implementers, so this
adds no trait with one.

**Delete and `⌘⌫`** are out of scope (Deferred). `⌘⌫` has no editor binding (`keymap.rs:72` is the
sidebar's).

## D6. Renumbering

After an edit adds, removes or moves an ordered item, the siblings of that item's list below the
edit get new digits, CodeMirror's rule (decision 5):

- The walk starts at the first sibling after the edit, with the number the edited list now expects
  there. It rewrites a sibling's digits, zero-padded to that sibling's own digit count.
- It stops at the first sibling whose number was not the previous one plus one before the edit, or
  whose marker kind differs. So a list the user numbers by hand, as `1. 1. 1.` or `1. 5. 9.`, is
  left alone.
- D4's two lists both renumber: the one an item leaves and the one it joins. An adopted ordered run
  restarts at `1` (D4).
- A sibling whose digit count changes takes the width shift (D7).
- The hunks join the command's own batch, so the whole command stays one undo step.

The walk costs the edited list's later siblings and their descendants, never the note.

## D7. The width shift

A marker's width sets its item's content column. When a command changes it, the item's other lines
would leave the item or turn lazy: `9. a\n   b` renumbered to `10.` puts `b` at column 3, before
the content column 4. So the same batch shifts them (decision 4).

- It applies to every marker a command rewrites: a renumbered sibling, a marker that takes a new
  kind under `⇥` or `⇧⇥`, a new continuation's moved lines (D2, rule 6), and adopted siblings (D4).
- The shift is the new content column less the old one. Every line of the item's block but its
  marker line takes it, after the quote part, as D4 inserts or cuts blanks. Blank lines take none.
- Its hunks join the command's batch, so the command stays one undo step.
- The hunks of one command never overlap: a line takes the sum of the shifts of the items that hold
  it, written as one hunk.

## D8. A marker the parse reads as no item

An empty item cannot interrupt a paragraph (`CommonMark` 5.2). Checked with pulldown-cmark 0.13.6:

- `- a\n  - ` reads the `-` as a setext underline, so `a` shows as a level-2 heading. Only `-` does
  this.
- `- a\n  * `, `- a\n  + ` and `1. a\n   1. ` read the marker as paragraph text, `a *`, with no item.
- The same holds at the top level after a paragraph: `para\n- ` and `para\n* `.

The parse stays `CommonMark` (decision 3): the screen shows the heading or the text until the first
letter, which makes the line an item. The commands still read the marker:

- On a line where no item opens and that holds no code, the commands run the fallback
  (`innermost_item`, `structure.rs:82-90`) on the line's text past its quote part and leading
  blanks, padded as `cycle.rs:168-176` pads it. A marker it finds with nothing after it is a
  **fallback item**.
- A fallback item's holder is the line's holder (D1), and its marker column is its own.
- `↩` treats it as an empty item (D3): nested under a holder, it outdents; with none, it leaves the
  list. `- p\n  - |` gives `- p\n- |`, and the heading is gone.
- `⇧⇥` outdents it to its holder's marker column, with the holder's kind (D4). With no holder it
  writes nothing.
- `⌫` deletes its line (D5, rule 1).
- `⇥` writes nothing (D4).
- A fallback marker with text after it, as a lazy `para\n2. b`, is not a fallback item; the line
  takes the held-line or plain rules.
- A line in a `Structure::skipped` range runs no fallback: a `- ` in front matter or in an HTML block
  is no markdown marker, so `---\ntags:\n  - |\n---` and `<div>\n- |\n</div>` take the plain key.

The fallback is one pulldown-cmark pass over one line, and runs only on the caret's line when no
item opens there.

## D9. The app

Every list command returns a `ListEdit`:

```rust
pub struct ListEdit {
    /// In order and disjoint, as `Diff::new` asserts.
    pub hunks: Vec<Hunk>,
    /// Where the caret lands, in the edited text.
    pub caret: ByteOffset,
}
```

The caret is explicit because `apply_diff`'s mapping cannot place it everywhere: D2's rule 6 can
write the new marker line after blank lines, below the caret. `⇥` and `⇧⇥` with a selection keep
`apply_diff`'s mapping of the selection, and their `caret` is the mapped head.

- `newline`: with an empty selection, no marked text and `parsed_at` equal to the buffer's version,
  it calls `continue_list` on `EditorView::markdown`. The hunks become a `Diff` at `parsed_at`. Inside one
  `edit_buffer`, `apply_diff` applies it and, when it lands, `EditorState::move_to` collapses the
  selection at `caret` (`state.rs:220-229`). It passes `inkra_editor::no_snap`: the snap of the
  parse before the edit would read the new offsets against old atoms, so `- a\n  b |\n  c` with `↩`
  would land at 13. `refresh`'s `snap_selection` snaps the caret against the new parse, as it does
  for marked text (task boxes spec, D2). `None` inserts `\n` through `EditorState::insert`, so a
  plain `↩` still merges with typing.
- `indent_item` and `outdent_item`: the same shape, with D4's fallbacks.
- **A stale parse.** When `parsed_at` is not the buffer's version, the handler returns before it
  calls `continue_list`, and takes no lease: the old parse's offsets could fall outside the text. The
  key edits nothing on that press. It never falls back to the plain key, whose edit could be the
  wrong one. `refresh` then parses as usual, so the next press works. No user path reaches it: every
  edit refreshes before the next key.
- The four handlers live in a new `crates/app/src/editor/actions/lists.rs`, declared from
  `actions.rs`, which declares the two actions and registers them, as it does `ToggleTask`. Not
  `editor/lists.rs`: `editor/mod.rs` sits at its line ceiling (`scripts/line-ceilings.tsv`), which
  one more `mod` line would break.
- `keymap.rs` gains `tab` → `IndentItem` and `shift-tab` → `OutdentItem` in the editor context.
  `ctrl-tab` and `ctrl-shift-tab` stay `AppShell`'s (`keymap.rs:98-99`).
- Each list command is one undo step, since `apply_diff` transacts on its own.

**The caret after `⌘Z`.** `Buffer::undo` returns the range its last inverse edit restored
(`crates/text/src/buffer/history.rs:180-187`), and `EditorState::undo` puts the caret at its end
(`state.rs:278-284`). `Buffer::apply_diff` replaces the hunks last first (`diff.rs:183-187`), and
undo replays the recorded edits backwards, so it ends on the hunk furthest down the note. The caret
lands at the end of that hunk's restored text. The owner accepts it (Open question 8). Per case:

| Command | Caret after `⌘Z` |
|---|---|
| `↩` rule 2, 3, 6 or 8 on the caret's line alone (rule 6 with blanks after the caret writes the cut and the new line as two adjacent hunks) | Where it was: the last hunk ends at the caret. |
| `↩` rule 6 with moved lines, a lazy-line prefix, a width shift or a renumbering; rule 2 or 8 with a lazy-line prefix | At the end of the lowest hunk, on a line below. |
| `↩` rule 6 written after blank lines | At the end of the lowest hunk: the new marker line's start or a moved line. |
| `↩` rule 7, either form | At the marker line's start, before the item part: the hunk is an insertion there. |
| D3 top level | Where it was: the restored item part ends at the line's end. |
| D3 nested | After the restored blanks, or after the restored marker when its kind changed: before the text start, so the item part reveals. |
| `⌫` rule 1 with no held lines, 2, 3 | Where it was. |
| `⌫` rule 1 with held lines, or a renumbering | At the end of the lowest hunk. |
| `⇥`, `⇧⇥` | At the end of the lowest hunk: after the restored blanks of the block's last line. |

`inkra-markdown` gains `crates/markdown/src/list_edit.rs`, with one submodule per command
(`newline.rs`, `indent.rs`, `backspace.rs`), `renumber.rs` and `shift.rs` (D7), their tests in their
own files. It exports `continue_list`, `indent_items`, `outdent_items` and `remove_markup`. No
marker grammar is written there: every marker, item and quote comes from the parse or the fallback,
as in `cycle.rs`. Reading the digits and the delimiter at a marker found that way is `marker_end`'s
job (`items.rs:285-290`).

## Performance

- Each list command reads the parse already cached for the buffer's version. It runs no pass over
  the note: no command verifies its edit by parsing a copy, unlike `⌘L`. The fallback (D8) is one
  pass over the caret's line, only where no item opens.
- Finding the caret item costs two binary searches. A sibling, parent or holder costs the depth.
- `↩` writes O(line) bytes, plus the lines that move. With no width shift, it reads the lines below
  the caret item's marker line up to the first non-blank one, not the item's whole block. `⇥` and `⇧⇥` cost the moved blocks' lines.
  Renumbering and the width shift cost the edited list's later siblings and their lines.
- The edit then reparses once through `refresh`, as any keystroke does.
- `Item` grows `Structure::items` from 8 to 32 bytes per item, filled in the existing pass. No
  projected vector is built (D1). Measure
  `cargo bench -p inkra-markdown --bench parse -- --quick 'markdown_parse/500000$'` before and
  after L1; it should stay within noise.

## Tests

`inkra-markdown`, `#[test]` tables per rule, input and expected text with `|` for the caret:

- D1: the holder of `c` in `- a\n  - b\n\n  c` is `a`; `1. a\n\npara\n\n2. x` has two lists;
  `> - a\n\n- b` has two lists.
- D2: every rule, each with a quoted variant; `- [x] a|`, `1) a|`, `09. a|`, `- - a|`, `-   a|`,
  `- it|em`, `- a |b` (the blank trimmed), `- a\n  b|`, `|- a`, `- > - a|`, `- a|\n  - c`,
  `- a|\nb`, `- a\n- |b`, `para\n- |abc`, `- p\n  - |abc`, a task's fenced code, an indented code
  block in an item, a fence on a marker line, a top-level fence, a lazy line; the three quoted
  cases `> para\n> - |abc`, `> - a\n>   |b` and `> - a|\nb`; quoted code `> ```\n> x\n> |`; the
  empty marker line that holds lines, `- a\n- |\n  - c`; `- a|\n\n  b`; `9. a|\n   - c`.
- The container prefix: one table over a quote, nested quotes, an item in a quote, a quote held by
  nothing, a lazy caret line, a blank new line, a tab in the indent.
- D3: top level, nested once and twice, inside a quote, an ordered parent.
- D4: one item, an item with children, a run, a run with a non-item line, a first item,
  `> - a\n\n- b`, a top-level `⇧⇥`, a tab in the leading blanks, a held code line, a selection
  inside an item's code, each marker-kind case, `- a\n  1. b\n  2. c` with `⇧⇥` on `b`.
- D5: each rule; a task with no text; the first line of a note; an empty marker line that holds a
  child, with a previous sibling, with a parent only, and with a wider previous sibling (`10.`).
- D6: an insertion, a removal, a move, a hand-numbered list, a kind change.
- D7: `9.` to `10.` with a held line and a child list, by renumbering, by `⇧⇥` and by `↩`; `10.` to
  `-` under `⇧⇥`.
- D8: `- p\n  - |`, `- p\n  * |`, `1. a\n   1. |` and `para\n- |` under `↩`, `⇧⇥`, `⌫` and `⇥`.
- No list command parses the note: `FALLBACKS`, the thread-local pass counter `Fallback::read`
  keeps for `⌘L` and the list commands alike (`structure.rs`), allows one fallback pass per command
  at most.

Proptests over generated notes: lists of bullets, numbers up to three digits, tasks, quotes, tabs in
the indent, held lines, lazy lines and code blocks in items, nesting up to four:

- **P0, the intended structure.** After every list command, a parse of the result shows the
  structure the command meant. Each command states it as a change to the old parse's items, each
  item given by its depth and its list: `↩` by rule 6 adds one item, a sibling of the caret item in
  its list, and gives it the old item's children; `⇥` makes the run children of the previous
  sibling, in its last child's list when it has one; `⇧⇥` makes the run siblings of the parent and
  the following siblings children of the run's last item; `⌫` removes one item, and an empty
  marker line's children become the item above's; `↩` on an empty item removes it or moves it up
  one level, and rule 2 or 8 adds no item. Every other item keeps its depth, its
  list and its order, and every quote still opens. The test parses; the command does not.
- **P1.** At a marker line's end with text, children and blank lines below included,
  `continue_list` then `remove_markup` restores the text up to the laziness of the lines `↩` moved.
  `↩` writes the container prefix before a moved lazy line, by design (D2, rule 6), and `⌫` keeps
  it: `- a|\nb` gives `- a\n  b` back. The generator keeps lazy lines; for them P1 compares the
  text with each moved lazy line's prefix stripped, and P0 still checks the parse's structure. P1
  restores the caret too, except when rule 6 wrote the marker line after blank lines: the caret then
  ends on the last blank line.
- **P5, the container prefix.** Every `\n` a command inserts is followed by the container prefix of
  its line, and the parse of the result puts that line in the same quotes and items as the command
  meant.
- **P2.** `indent_items` then `outdent_items` on an item with no sibling below restores the text, on
  bullet and on sequential ordered lists.
- **P3.** `outdent_items` writes no non-blank byte outside the markers it rewrites, and `⇧⇥` with
  no item writes nothing.
- **P4.** After `continue_list` by rule 6, the new line holds a gutter item, and a task when the
  caret item was one (`Markdown::task_at`).

`app`, headless `#[gpui::test]` against `MemFs`, one `simulate_keystrokes` test per binding:

- `enter` continues `- a`, and on the empty item leaves the list.
- `tab` nests `- b` under `- a`; `tab` on a paragraph inserts `\t`.
- `shift-tab` outdents; `shift-tab` on a paragraph changes nothing.
- `backspace` turns `- [ ] a` into `- a` in the editor, and deletes one byte in the name field.
- `cmd-z` after each restores the text in one step, and puts the caret where D9's table says: one
  test per row, rule 7, D3 nested and rule 2 at the text start included.
- A command on a stale parse, made by a write that skips `refresh`, edits nothing.
- Keys pressed in a row each read the previous edit's parse: `- a` then `enter enter` leaves the
  list, `1. a` then `enter b enter` gives `3. `.
- The caret of a `ListEdit` snaps against the new parse: `- a\n  b |\n  c` with `enter` lands at
  the new line's text start, not at 13. `backspace` on `- a\n- |\n  3. c` gives `- a|\n  1. c`.
- `keymap/tests.rs` passes: `tab` and `shift-tab` are bound in the editor context alone.

After L1 and L2, render the `markdown-lists` scene with a continued and a nested item, and look at it.

## Slices

Each slice is vertical, shippable alone, and ends green on `./scripts/check.sh`, then runs the
markdown mutants on the diff (CLAUDE.md, Commands).

1. **L1, `↩` continues and leaves** (~2 d). D1's `Item` and its readers, the container prefix, D2
   whole, D3's top-level case, D7 (`shift.rs`) for the lines `↩` moves, D8 for `↩`, `ListEdit`, the
   `newline` change, the stale-parse rule, their tests, P0, P4 and P5 for `↩`, P1's continue half
   with the plain fallback. Until L2 a nested empty item leaves the list as at the top level. Until
   L4 nothing renumbers, so a split of `1. a` gives two `2.`s. P0 checks depth, list and order,
   which a repeated number does not change, so L1 runs it on every generated list; a sibling's
   digits stay as written, so no width changes below the edit. L4 adds the number check. The parse
   bench before and after.
2. **L2, `⇥` and `⇧⇥`** (~2 d). D4 with the adopted siblings renumbered from `1`, D3's nested case,
   D7 for marker kinds and adopted siblings, D8 for `⇧⇥` and `⇥`, the two actions and bindings, the
   hide list, their tests, P0 for both, P2 on bullet lists, P3. The scene.
3. **L3, `⌫`** (~0.5 d). D5 and `backspace_markup`, D7 for an empty marker line's held lines, D8 for
   `⌫`, their tests, P0 and P5 for `⌫`, P1 whole.
4. **L4, renumbering** (~1 d). D6 in every command, D7 for renumbered widths, their tests, P0's
   generator with hand-numbered lists, P2 on ordered lists.

## History

- 2026-11-19: drafted from the list-typing audit, after the owner approved chantier A (`56b97c65`).
- 2026-11-19: the owner decided Open questions 1–7 and 9. A red-team review, checked with
  pulldown-cmark 0.13.6, found:
  - B1, only `-` makes a setext heading; `*`, `+` and `1.` read as text. Rewrote the facts (D8),
    gave each command its fallback rule, and made `↩` at a text start write a sibling above or a
    blank line (D2, rule 7).
  - B2, siblings had meant the same parent, so `1. a\n\npara\n\n2. x` renumbered across two lists
    and `> - a\n\n- b` with `⇥` made indented code. `Item` now records its list (D1).
  - M1, a held line's item was the last marker above it; it is now the holder (D1).
  - M2, a wider marker broke its item's lines: the width shift (D7, decision 4).
  - M3, `⇧⇥` in place left an adopted ordered run as text: it renumbers from `1` in L2 (D4).
  - M4, the code rule used the hang column, wrong for a task's code: a code line copies its item
    part (D2, rule 2).
  - M5, L1 lacked the empty-item exit, so it was not shippable: D3's top-level case moved into it.
  - Minor findings: the readers of `Structure::items` (D1), the `parsed_at` stamp and the stale
    parse (D9), the caret after `⌘Z` (D9), a selection in an item's code (D4), lazy lines in a
    split (D2, rule 6), and the central property P0.
- 2026-11-19: the owner accepted Open question 8's option a. A second review of `e30bd84a` found:
  - Two majors, one cause: rules 2, 7 and the lazy-line indent wrote their own prefixes, so
    `> para\n> - |abc`, `> - a\n>   |b` and `> - a|\nb` left the quote or the item. Fixed by one
    invariant, the container prefix, which every inserted `\n` takes (D2), and property P5.
  - An empty marker line that holds lines, which decision 6 creates, fell through every rule and
    broke P0 and P1. Derived from them: `↩` continues it (D2, rule 6), and `⌫` deletes it and gives
    its held lines to the item above with the width shift (D5, rule 1). `- a|\n\n  b` now keeps `b`
    in the list.
  - L1 promised P0 without the width shift its moves need: D7 for moved lines moved into L1.
  - D9's caret after `⌘Z` was wrong for rule 7, D3 nested and rule 2 at the text start: a table
    per case, each tested. `ListEdit` makes the caret after a command explicit.
  - Rule 8 could end quoted code: a line in a code block never leaves its quote (D2, rules 2 and 8).
- 2026-11-19: a final review of `5d3f0d62` found that a `ListEdit`'s caret snapped with the old
  parse (now `no_snap`, then `refresh`, D9), that P1 failed on lazy lines (exception stated, P1),
  that `backspace_markup` returned a `Diff` (now a `ListEdit`, D5), that `⌫` left an ordered child
  list starting past `1` as text (renumbered, D5 rule 1), and that rule 8 had no source for a lazy
  line's `>`s (`Gutter::depth`, D2 rule 8).
- 2026-11-21: the review of L1 found that rule 2 split a quote at a lazy line's text start (its quote
  part now comes from the container prefix), that rules 2 and 8 left a lazy line below outside its
  item or quote (they write its prefix, as rule 6 does), that the fallback read markers in front
  matter and HTML (D8 skips skipped ranges), that rule 6 kept the blanks after the caret and, past
  blank lines, before it (both go), and that the moved-line walk read the whole block with no shift.
  The handler moved to `editor/actions/lists.rs` for the line ceiling, `Item::end` drops its last
  line ending, the stale-parse check runs before `continue_list`, and four cases became Known
  limits.
- 2026-11-25: L2. Its properties against pulldown-cmark found cases the decisions leave open, now
  numbered amendments; code comments cite them by number. `Marker` no longer carries its kind: the
  commands read it from the text (`list_edit/kind.rs`).
  1. **An item on its parent's marker line** (owner-approved, decision 2 below), as `b` in `- - b`, has no blanks of its own to cut:
     `⇧⇥` writes nothing. `↩` on such an empty item deletes its marker alone, so the parent stays,
     empty: `- - |` gives `- |`, and `> 1. - |` gives `> 1. |`.
  2. **`⇥` joins the last child's list only when that list ends the new parent's block**: only blank
     lines follow it there. After a paragraph, as in `- a\n  * x\n\n  p\n- b`, the run starts a
     list of its own: a bullet keeps its byte, a number restarts at `1`.
  3. **A child list of the parent right below the run**, with no blank line above it, is adopted
     too, and restarts at `1`: under the outdented item's paragraph, a list past `1` would read as
     text. `- a\n  - b|\n  9. c` gives `- a\n- b\n  1. c`. With a blank line above, it moves with
     the rest of the block and keeps its numbers.
  4. **Reversed by the owner** (decision 1 below). It made the adopted siblings join the outdented
     item's last child list, in its kind; they now keep their own.
  5. **Padding.** A restart and an adopted renumbering write `1`, `2`, and so on unpadded; `⇥`'s
     join pads to the last child's digit count, so `01. x` is followed by `02.`. D6 decides the rest
     in L4.
  6. **The width shift reads the written marker.** An item's new content column counts from its new
     marker's end over the blanks already after it, so a tab there counts its columns anew (D7).
  7. **The caret after `⌘Z` on `⇧⇥` whose lowest hunk is a marker rewrite** snaps past the item
     part, to the text start: `1. a\n   - b|` with `⇧⇥`, then `⌘Z`, puts it at 9, not 8 (D9).
- 2026-11-25: the owner decided L2's open points.
  1. **The adopted siblings keep their own kind**, as D4 reads literally: they shift, and an ordered
     run restarts at `1`. `- a\n  - b|\n    1. x\n  - c\n  - d` with `⇧⇥` gives
     `- a\n- b\n  1. x\n  - c\n  - d`: `c` and `d` stay bullets, a list of their own right after
     `x`'s. Amendment 4 is reversed; amendment 3 holds, its list restarting at `1` in its own kind.
  2. **Amendment 1 stands as written**: `⇧⇥` on an item on its parent's marker line writes
     nothing, and `↩` on such an empty item deletes its marker alone.
  3. **`⇥` under an empty item above a blank line writes nothing**, as on a first item (decision 7,
     D4). It replaces that case's Known limit.

## Deviations

- **From Zed, the source of truth.** Zed matches a line prefix with fixed strings and a regex
  (`input.rs:2678-2796`). Inkra reads the parse. So a code block never continues a list, a lazy
  number is no item, and `* [ ]` and `1. [ ]` continue as tasks. Zed continues every task as
  `- [ ] ` (`config.toml:27`); Inkra keeps the task's own marker.
- **From Zed, the indent.** Zed indents a list line by `tab_size`, 2 spaces
  (`editor.rs:5575-5605`). Inkra shifts a block to its previous sibling's content column, so
  `10. a` nests its next item by 4, and a child always lands where `CommonMark` reads it. Zed also
  indents a first item; Inkra does not, since the result would not nest.
- **From Zed, the empty nested item.** Zed outdents by one tab stop and keeps the marker, `1.` for a
  number (`input.rs:2739-2741`). Inkra outdents to the parent's level and takes the parent's marker
  kind, as CodeMirror does.
- **From Zed, `↩` at a text start.** Zed continues there and leaves an empty item above
  (`input.rs:2731`). Inkra writes a sibling above only where it parses, else a blank line (D2,
  rule 7).
- **From Zed, `⌫`, renumbering and the width shift.** Zed has none of them. Inkra follows
  CodeMirror's renumbering rule, and adds the width shift.
- **From CodeMirror, `⌫` after a marker.** CodeMirror replaces the marker with spaces. Inkra deletes
  it: the text keeps its bytes before the marker, opens no indented code, and D11 already paints a
  lazy line at its item's text.
- **From CodeMirror, `nonTightLists`.** Not followed: an empty second item leaves the list as any
  empty item does.
- **From Obsidian, the indent byte.** Obsidian indents with a tab by default (unsure). Inkra writes
  spaces, whose columns are exact in a quote and after a wide number.

## Known limits

Accepted as rare by the L1 review (2026-11-21). The `↩` property skips the first two; its
generator never writes the last two.

- A blank line written right under an empty marker line, by rule 7's blank-line form or rule 2 at a
  text start, ends that item before the lines it held: `> - \n>   1. |a` turns `1. a` into a
  sibling list.
- Quoted indented code that no item holds loses its code indent under rule 2: the parse records a
  leaf text start only while a list is open (task boxes spec, D11), so `>     x|` gives
  `>     x\n> |`, a paragraph line after the code.
- Rule 3 fires on an indented code block that opens on a marker line (D2, rule 3).
- A sibling of `999999999.` is numbered `1000000000.`, which `CommonMark` (9 digits at most) reads
  as text. Found by the final L1 verification; no note reaches it by hand.

Accepted by L2 (2026-11-25). The `⇧⇥` and `↩` properties skip the first; the `⇥`, `⇧⇥` and `↩`
properties skip the second.

- An empty item above a blank line holds nothing past it: the parse ends it there. So `⇧⇥` that
  would make it adopt siblings, and D3's nested `↩` on it, leave the lines past the blank line
  outside it. `⇥` under it writes nothing (D4).
- A tab right after a moved descendant's marker can count fewer columns once its block shifts:
  only the markers a command rewrites read their blanks anew (amendment 6).

## Deferred

| Item | Trigger |
|---|---|
| Reveal-rule changes for list markers (chantier B) | Chantier B is scheduled. |
| An empty marker under a paragraph shown as an item, not a heading or text (D8) | Chantier B. |
| The history stores and restores the selection with every edit, as Zed does, so `⌘Z` after a command puts the caret back (Open question 8) | Its own spec, decided by the owner on 2026-11-19 as later work. |
| `Delete` at a marker line's end joins the next item's text without its markup | The owner asks; it mirrors D5. |
| `⌫` at a quote line's text start removes its `>` | The owner asks. |
| `↩` keeps a code line's own indent past its leaf text start | Code editing gets auto-indent. |
| `⇥` and `⇧⇥` in the name field | A file name with a tab is reported. |
| Moving an item up or down with its children | The owner asks. |

## Risks

- **The transient heading.** Between `⇥` on an empty item and the first letter, the parent's text
  shows as a level-2 heading (`-`) or gains the marker as text (`*`, `+`, `1.`). Decision 3 accepts
  it.
- **Tabs in a quote.** Columns count from the line's start, as `Blanks::scan` counts them
  (`gutters.rs:229-263`). A tab right after a `>` splits its columns between the quote and the
  item, so D4 and D7 rewrite such a line's leading blanks as spaces before they cut.
- **A long batch.** `⇧⇥` on an item high in a long list adopts every sibling below it, so one key
  can rewrite many lines. Each is one hunk of one undo step.

## Open questions

1. **An empty first child reads as a heading or text.** Decided by the owner on 2026-11-19: the
   parse stays `CommonMark`, and the commands read the marker through the fallback (D8). The display
   may change in chantier B.
2. **Renumber the siblings below?** Decided on 2026-11-19: CodeMirror's rule, while the old numbers
   ran in sequence (D6).
3. **`↩` at the end of an item that has children.** Decided on 2026-11-19: a new sibling right after
   the marker line's text, which takes the children, as in Obsidian (D2, rule 6).
4. **`⌫` on an empty item.** Decided on 2026-11-19, as recommended: it deletes the line and the
   `\n` before it (D5).
5. **`⇧⇥` on an item with siblings below.** Decided on 2026-11-19, as recommended: in place; the
   siblings below become its children, renumbered from `1` when ordered (D4).
6. **`⌫` at a bullet's or a number's text start.** Decided on 2026-11-19, as recommended: delete the
   marker and its blanks (D5).
7. **`⇥` on an item with no previous sibling.** Decided on 2026-11-19, as recommended: nothing (D4).
8. **The caret after `⌘Z` on a command of several hunks** (D9). It lands at the end of the hunk
   furthest down the note, as after `⌘L` today: after `⇥` on an item with held lines, on its last
   line; after a renumbering `↩`, on the last renumbered number. Decided by the owner on 2026-11-19:
   accept it (option a). A later, separate spec makes the history store and restore the selection
   with every edit, as Zed does (Deferred). The rejected option b did it here, which changes
   `inkra-text`'s history for every edit.
9. **A marker that changes width.** Decided by the owner on 2026-11-19: the same batch shifts the
   item's other lines by the width difference (D7).
