# List editing: `↩`, `⇥`, `⇧⇥` and `⌫` in lists and quotes

Date: 2026-11-19. Status: **proposed**, revised after a red-team review (History). No code yet.
The owner approved the scope ("chantier A") on 2026-11-19, and decided Open questions 1–7 and 9 the
same day. Open question 8 waits on the owner.

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
(D9). A parse that lags the text then fails `apply_diff`, and the command edits nothing.

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
    /// Its range's end, as pulldown-cmark gives it: its block holds the lines that start before it.
    pub end: ByteOffset,
    /// Its parent item's index in `Structure::items`; `None` for a top-level item.
    pub parent: Option<u32>,
    /// Its list: the index of its `Start(List)` among the note's lists, in document order.
    pub list: u32,
}
```

- A `Start` event carries its element's whole range, so `end` costs nothing.
- The collector keeps two stacks, of open items and of open lists, each entry with its range's end.
  At each `Start(Item)` or `Start(List)` it pops the entries that end at or before the new start;
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

`continue_list(text, markdown, caret) -> Option<Vec<Hunk>>`. `None` means no list rule applies, and
the app inserts the plain `\n` as today. The rules, first match wins:

1. **A selection, or marked text, takes the plain key.** Zed and CodeMirror do the same. The
   selection is replaced by `\n`, as on `main`.
2. **A caret on a held line**, a code line included. The new line copies the line's bytes from its
   start to its text start: its quote part and its item part. A code line's item part ends at its
   leaf text start (task boxes spec, D11), so the code keeps its place in the item and no marker is
   written:
   - `- a\n  b|` gives `- a\n  b\n  |`, a new line of the same paragraph;
   - a task's fenced code, `- [ ] a\n  ```\n  x|`, gives a new code line at column 2, where the
     code starts, not at the box's hang column 6;
   - an indented code block in an item, `- a\n\n      x|`, gives a new line with the six blanks,
     still code.

   With the caret at the text start, the `\n` goes before those bytes, so no line is left with
   blanks only. A lazy line copies its own prefix, which may be empty: the user wrote it lazy.
3. **A caret on a marker line inside a code block**: a marker line whose text opens a fence. The
   new line copies the quote part and fills spaces up to the content start's column. `- ```rust|`
   gives `- ```rust\n  |`.
4. **A code block outside every item and quote** takes the plain key.
5. **A caret on an empty item's line, at or past its text start** (D3). A fallback item counts
   (D8).
6. **A caret on a marker line, past its text start.** The edit replaces the blanks just before the
   caret, down to the text start at most, with `\n` and the **continuation**:
   - the marker line's quote part, copied;
   - the bytes from the item part's start to the caret item's marker, copied when they are blanks
     only, else spaces up to the marker's column (`- - a` continues as `  - `);
   - the marker: the same bullet byte, or the number plus one with the same delimiter, its digits
     zero-padded to the old count (`01.` gives `02.`);
   - the blanks between the marker and the content start, copied, or one space when the marker
     line has five columns or more (`CommonMark` 5.2, as `checkbox_at` counts them,
     `cycle.rs:148-162`);
   - for a task, `[ ] ` and nothing else: a checked task continues unchecked.

   The text after the caret moves to the new item: that is the split. `- it|em` gives
   `- it\n- |em`. Every line the old item held after the caret moves with it:
   - its children become the new item's children (decision 6): `- a|\n  - c` gives
     `- a\n- |\n  - c`;
   - a held line stays held, now by the new item;
   - a lazy line after the caret is part of the item's text, so it moves too. When the new item's
     marker line is empty, a lazy line would drop out of the list, so the batch indents each such
     line to the new item's content column: `- a|\nb` gives `- a\n- |\n  b`.

   When the new marker is wider than the old (`9.` to `10.`), the lines that move take the width
   shift (D7).
7. **A caret at a marker line's text start, with text after it.** The continuation would leave an
   empty item above the text, and an empty item cannot interrupt a paragraph (D8). So:
   - when the caret item is not its list's first item, the edit writes an empty sibling above: the
     continuation's bytes and a `\n`, at the marker line's start. The new item takes the caret
     item's number, and the caret item and those below renumber (D6). The caret stays on the text.
     `- a\n- |b` gives `- a\n- \n- |b`.
   - when it is its list's first item, the edit writes a `\n` at the marker line's start instead: a
     blank line above, which keeps the item an item. `para\n- |abc` gives `para\n\n- |abc`, and
     `- p\n  - |abc` gives `- p\n\n  - |abc`. An empty item there would read as `p` with a setext
     underline, or as paragraph text.
8. **A caret on a quote line with no item part.** The new line copies the quote part:
   `> a|` gives `> a\n> |`. An empty quote line, blanks only past its quote part, loses its
   innermost `>` and the blank after it instead: `> > |` gives `> |`, and `> |` gives an empty
   line, so the quote ends.
9. **A caret before the text start** of a marker line, in a revealed item part, takes the plain key.
   `|- a` moves the item down a line, as in Zed (case 8).
10. **A line holding an opener past its text start** takes the plain key. In `- > - a` the inner
    item is no gutter item; continuing the outer item there would be a guess.

Quote and list combine through rule 6's first bullet: `> - a|` gives `> - a\n> - |`, and
`> 1. [x] a|` gives `> 1. [x] a\n> 2. [ ] |`.

An ordered continuation renumbers the siblings below it (D6).

**The caret** lands at the new item's text start (rule 6), or stays on its text (rule 7).
`apply_diff` puts it there with no extra field.

## D3. `↩` on an empty item

- **Top level**: the edit deletes the item part's bytes and any blanks after them, from the item
  part's start to the line end. The quote part stays. So `- a\n- |` gives `- a\n|`, and the list
  ends. `> - |` gives `> |`, and the quote goes on: one level per `↩`.
- **Nested**: the item outdents one level, as `⇧⇥` does (D4). `- a\n  - b\n  - |` gives
  `- a\n  - b\n- |`. A second `↩` then leaves the list.
- A fallback item is nested when its line has a holder (D8).
- The caret lands at the line's new text start: the quote part's end at the top level, the
  outdented item's text start when nested.
- An empty item that holds a child is no empty item: its marker line falls under D2's rule 6.

Zed and CodeMirror agree on both cases. CodeMirror's `nonTightLists` blank line is not followed: a
note app's lists are tight by habit (Deviations).

## D4. `⇥` and `⇧⇥`

Two editor actions, `editor::IndentItem` on `tab` and `editor::OutdentItem` on `shift-tab`, in the
editor context. Both go on the palette's hide list: from the palette, they would act on whatever
line the caret sits on.

`indent_items(text, markdown, selection) -> Option<Vec<Hunk>>` and `outdent_items(...)`. `None`
means the selection touches no item; `Some` of an empty vector means it touches one and nothing can
move.

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

`remove_markup(text, markdown, caret) -> Option<Vec<Hunk>>`, for an empty selection with the caret
at a marker line's text start, a fallback item's included. Any other `⌫` takes the plain key, the
D2 atom included: a held line's `⌫` still joins it to the line above (task boxes spec, D2). One step
per press, first match wins:

1. **An empty item** (`- |`, `1. |`, `- [ ] |`): the edit deletes the line's item part, its
   trailing blanks, and the `\n` before the line, so the caret lands at the previous line's end. So
   `↩` then `⌫` on an empty continued item restores the text and the caret (decision 7). On a
   note's first line, only the item part goes. A quoted empty item keeps its quote part:
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
fn backspace_markup(&self, cx: &App) -> Option<Diff> {
    None
}
```

The shared handler applies the `Diff` through `apply_diff` when one comes back, else calls
`EditorState::backspace` as today. `EditorView`'s returns `remove_markup`'s hunks as a `Diff` at
`parsed_at` (D9). `NameField` takes the default. The trait already has two implementers, so this
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

The fallback is one pulldown-cmark pass over one line, and runs only on the caret's line when no
item opens there.

## D9. The app

- `newline` (`actions.rs:138-141`): with an empty selection and no marked text, it calls
  `continue_list` on `EditorView::markdown`. `Some(hunks)` becomes a `Diff` at `parsed_at` and goes
  through `apply_diff`, as `toggle_task` applies its batch. `None` inserts `\n` through
  `EditorState::insert`, so a plain `↩` still merges with typing.
- `indent_item` and `outdent_item`: the same shape, with D4's fallbacks.
- **A stale parse.** When `parsed_at` is not the buffer's version, `apply_diff` returns false and
  the key edits nothing on that press. It never falls back to the plain key, whose edit could be the
  wrong one. `refresh` then parses as usual, so the next press works. No user path reaches it: every
  edit refreshes before the next key.
- The four handlers live in a new `crates/app/src/editor/lists.rs`; `actions.rs` declares the two
  actions and registers them, as it does `ToggleTask`.
- `keymap.rs` gains `tab` → `IndentItem` and `shift-tab` → `OutdentItem` in the editor context.
  `ctrl-tab` and `ctrl-shift-tab` stay `AppShell`'s (`keymap.rs:98-99`).
- Each list command is one undo step, since `apply_diff` transacts on its own.

**The caret after `⌘Z`.** `Buffer::undo` returns the range its last inverse edit restored
(`crates/text/src/buffer/history.rs:180-187`), and `EditorState::undo` puts the caret at its end
(`state.rs:278-284`). `Buffer::apply_diff` replaces the hunks last first (`diff.rs:183-187`), and
undo replays the recorded edits backwards, so it ends on the hunk furthest down the note. Per
command:

- `↩` with one hunk, `⌫`, and `↩` on an empty item: the caret returns to where it was before the
  command.
- `↩` with renumbering or moved lines, and `⇥` or `⇧⇥` on more than one line: the caret lands at the
  end of the hunk furthest down the note, as after `⌘L` today (Open question 8).

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
- `↩` writes O(line) bytes, plus the lines that move. `⇥` and `⇧⇥` cost the moved blocks' lines.
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
  block in an item, a fence on a marker line, a top-level fence, a lazy line.
- D3: top level, nested once and twice, inside a quote, an ordered parent.
- D4: one item, an item with children, a run, a run with a non-item line, a first item,
  `> - a\n\n- b`, a top-level `⇧⇥`, a tab in the leading blanks, a held code line, a selection
  inside an item's code, each marker-kind case, `- a\n  1. b\n  2. c` with `⇧⇥` on `b`.
- D5: each rule; a task with no text; the first line of a note.
- D6: an insertion, a removal, a move, a hand-numbered list, a kind change.
- D7: `9.` to `10.` with a held line and a child list, by renumbering, by `⇧⇥` and by `↩`; `10.` to
  `-` under `⇧⇥`.
- D8: `- p\n  - |`, `- p\n  * |`, `1. a\n   1. |` and `para\n- |` under `↩`, `⇧⇥`, `⌫` and `⇥`.
- No list command parses the note: a thread-local pass counter, as `cycle.rs` keeps
  (`cycle.rs:242-251`), allows one fallback pass per command at most.

Proptests over generated notes: lists of bullets, numbers up to three digits, tasks, quotes, tabs in
the indent, held lines, lazy lines and code blocks in items, nesting up to four:

- **P0, the intended structure.** After every list command, a parse of the result shows the
  structure the command meant. Each command states it as a change to the old parse's items, each
  item given by its depth and its list: `↩` by rule 6 adds one item, a sibling of the caret item in
  its list, and gives it the old item's children; `⇥` makes the run children of the previous
  sibling, in its last child's list when it has one; `⇧⇥` makes the run siblings of the parent and
  the following siblings children of the run's last item; `⌫` removes one item and nothing else;
  `↩` on an empty item removes it or moves it up one level. Every other item keeps its depth, its
  list and its order, and every quote still opens. The test parses; the command does not.
- **P1.** At a marker line's end with text, `continue_list` then `remove_markup` restores the text
  and the caret.
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
- `cmd-z` after each restores the text in one step, and puts the caret where D9 says.
- A command on a stale parse, made by a write that skips `refresh`, edits nothing.
- `keymap/tests.rs` passes: `tab` and `shift-tab` are bound in the editor context alone.

After L1 and L2, render the `markdown-lists` scene with a continued and a nested item, and look at it.

## Slices

Each slice is vertical, shippable alone, and ends green on `./scripts/check.sh`, then runs the
markdown mutants on the diff (CLAUDE.md, Commands).

1. **L1, `↩` continues and leaves** (~1.5 d). D1's `Item` and its readers, D2 whole, D3's top-level
   case, D8 for `↩`, the `newline` change, the stale-parse rule, their tests, P0 for `↩`, P1's
   continue half with the plain fallback, P4. Until L2 a nested empty item leaves the list as at the
   top level. Until L4 nothing renumbers, so a split of `1. a` gives two `2.`s, and a widened marker
   takes no width shift. The parse bench before and after.
2. **L2, `⇥` and `⇧⇥`** (~2 d). D4 with the adopted siblings renumbered from `1`, D3's nested case,
   D7 for marker kinds and adopted siblings, D8 for `⇧⇥` and `⇥`, the two actions and bindings, the
   hide list, their tests, P0 for both, P2 on bullet lists, P3. The scene.
3. **L3, `⌫`** (~0.5 d). D5 and `backspace_markup`, D8 for `⌫`, their tests, P0 for `⌫`, P1
   whole.
4. **L4, renumbering** (~1 d). D6 in every command, D7 for renumbered widths and moved lines, their
   tests, P2 on ordered lists.

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

## Deferred

| Item | Trigger |
|---|---|
| Reveal-rule changes for list markers (chantier B) | Chantier B is scheduled. |
| An empty marker under a paragraph shown as an item, not a heading or text (D8) | Chantier B. |
| `⌘Z` restores the selection from before a command (Open question 8) | The owner picks option b. |
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
   line; after a renumbering `↩`, on the last renumbered number.
   - a. Accept it for chantier A. Recommended: it matches `⌘L`, and the fix belongs to the history.
   - b. Store the selection with each transaction and restore it on `⌘Z`, as Zed does. It changes
     `inkra-text`'s history for every edit, so it would be its own spec.
9. **A marker that changes width.** Decided by the owner on 2026-11-19: the same batch shifts the
   item's other lines by the width difference (D7).
