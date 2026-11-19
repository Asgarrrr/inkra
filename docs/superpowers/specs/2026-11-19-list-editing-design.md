# List editing: `↩`, `⇥`, `⇧⇥` and `⌫` in lists and quotes

Date: 2026-11-19. Status: **proposed**, awaiting review. No code yet. The owner approved the scope
("chantier A") on 2026-11-19, after an audit of list typing. Open questions 1–7 wait on the owner.

## Why

Typing a list is poor, because the editor has no list-aware command. Checked against `main` at
`f2e7366f`:

- `↩` inserts a bare `\n` (`newline`, `crates/app/src/editor/actions.rs:138-141`). The user types
  every marker by hand.
- `tab` and `shift-tab` have no binding (`crates/app/src/keymap.rs:39-71`). Both reach the platform
  input and insert `\t`, the audit found. A `⇧⇥` that writes a byte is a data bug.
- `⌫` at an item's text start deletes one byte. `- abc` becomes `-abc`, and `- [ ] abc` becomes
  `- [ ]abc`: the markup turns to text.
- Typed by hand, a nested marker passes through `CommonMark` states that look broken: in `- a\n  -`
  the `-` is a setext underline, so `a` shows as a level-2 heading.

## Decisions taken

By the owner, on 2026-11-19.

1. **Chantier A is built.** `↩` continues the list: the next number, a task as an unchecked task.
   `↩` in the middle of an item splits it. `↩` on an empty item leaves the list, or outdents it one
   level when it is nested. `⇥` and `⇧⇥` indent and outdent the whole item. `⌫` at the text start
   removes the markup. `↩` inside `> ` continues the quote.
2. **Chantier B is out of scope.** Changes to the reveal rule are in Deferred.

## Terms

This spec reuses the task boxes spec's terms (`2026-11-16-task-boxes-design.md`, Terms): **gutter
item**, **marker line**, **item part**, **text start**, **content start**, **opener**, **hang
column**, **atom**. It adds:

- **list command**: `↩`, `⇥`, `⇧⇥` or `⌫` as this spec defines them, as opposed to the **plain
  key**, the behaviour on `main`.
- **held line**: a line that a gutter item holds and on which no item opens. Its item part has no
  mark.
- **caret item**: the innermost gutter item that opens on, or holds, the caret's line.
- **empty item**: a gutter item whose marker line has only blanks past its text start, and that
  holds no other line. `- `, `1. ` and `- [ ] ` are empty items.
- **quote part**: the gutter's bytes from the line's start through its last `>` and the one blank
  after it (`Gutter::quote_end`, `crates/markdown/src/gutters.rs:66-68`).
- **block**: an item's marker line and every line it holds, its children included: from the line
  start of its marker to its range's end.
- **sibling**: an item with the same parent item, or with no parent, as another. The **previous
  sibling** is the nearest one above.
- **marker kind**: a bullet's byte (`-`, `*`, `+`), or an ordered marker's delimiter (`.`, `)`).
  pulldown-cmark starts a new list when the kind changes.
- **renumber**: rewrite the digits of the ordered siblings after an edited item (D6).

## Context

- `toggle_task` is the precedent (`actions.rs:174-183`). It reads the cached parse with
  `parses::parsed`, passes it with the text and the selection to `cycle_tasks`
  (`crates/markdown/src/cycle.rs:38-72`), and applies the returned hunks as one `Diff` through
  `EditorState::apply_diff`. `inkra-markdown` computes the edit from the parse, the app binds the
  key, and `inkra-editor` reads no markdown (task boxes spec, D6).
- `Buffer::apply_diff` (`crates/text/src/diff.rs:179-190`) is one undo step that merges with no
  typing on either side. `EditorState::apply_diff` maps the selection through it
  (`crates/editor/src/state.rs:296-319`): a caret lands after an insertion at its offset, and at
  the start of a deleted range that holds it.
- `Structure::items` holds each item's marker only (`crates/markdown/src/structure.rs:14-24`). It
  has no end and no parent. `Gutter::item` gives a line's item part, its task and whether it is
  marked (`gutters.rs:12-37`).
- The shared keys bind once, in the `TextInput` context: `backspace` among them
  (`keymap.rs:41`, handler at `crates/app/src/text_input/actions.rs:29-35`). A host binds only its
  own keys, and `keymap/tests.rs` fails on a key bound in both (`.claude/rules/editor.md`).
- `enter` binds `Newline` in the editor context and `Confirm` in the field context
  (`keymap.rs:60`, `:87`). `Newline` is on the palette's hide list
  (`crates/app/src/command_palette/commands.rs:28`).

### Is the parse fresh at keystroke time?

Yes, in every path a user reaches.

- `edit_buffer` calls `after_edit`, and the editor's reshapes (`.claude/rules/editor.md`).
  `reshape` calls `refresh` first (`crates/app/src/editor/mod.rs:169-180`). `refresh` parses when
  the buffer's version moved (`mod.rs:135-161`).
- A write that bypasses `edit_buffer`, such as a reload, reaches `refresh` through
  `on_buffer_changed` (`mod.rs:164-167`).
- `parses::parsed` returns the cached parse for the buffer's current version, and parses once on a
  miss (`crates/app/src/editor/parses.rs:23-48`).

So a list command reads the parse that the previous edit already paid for. It runs no extra
pulldown-cmark pass (Performance).

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
- The newline edit itself is built at `input.rs:662-741`. `ClearCurrentLine` replaces the row with
  nothing; `UnindentCurrentLine` rewrites the indent less one `outdent_len`.
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
- `⇥` and `⇧⇥` indent and outdent an item with its children. By default it indents with a tab
  character ("Indent using tabs"). Sure of the move, unsure of the default today.
- It renumbers an ordered list after an insertion. Unsure.
- `⌫` right after an empty `- `: unsure whether it removes the marker or the line.

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
}
```

- `Tag::Item`'s range is known at its `Start` event, so `end` costs nothing.
- The collector keeps a stack of open items' indices. At each `Start(Item)` it pops the items whose
  `end` lies at or before the new item's start; the top is the parent. That needs no `End` event.
- `cycle.rs:120-123` reads `.marker` instead of the bare offset. `innermost_item` reads the last
  item's `.marker` (`structure.rs:82-90`).
- The previous sibling of item `i`: walk `j = i - 1`, then up its parents, until an item with `i`'s
  parent. Reaching `i`'s parent means there is none. That costs the depth, never the list.
- The next siblings of item `i`: the items after `i` whose marker lies at or past `i`'s `end` with
  `i`'s parent, until the parent's `end`. Only renumbering walks them (D6).
- The caret item comes from the caret line's gutter, found by one binary search over `gutters` as
  `Markdown::task_at` does (`gutters.rs:109-120`). Its marker is the last of `items` at or before
  the line's text start, and its index follows from one more binary search.

Why `end` and `parent` and not a tree: the vector stays flat and sorted, as `cycle.rs` reads it, and
an item's relatives cost its depth.

## D2. `↩`

`continue_list(text, markdown, caret) -> Option<Vec<Hunk>>`. `None` means no list rule applies, and
the app inserts the plain `\n` as today. The rules, first match wins:

1. **A selection, or marked text, takes the plain key.** Zed and CodeMirror do the same. The
   selection is replaced by `\n`, as on `main`.
2. **A caret inside a code block.** The new line copies the caret line's quote part and fills the
   item columns with blanks up to the hang column of the caret item, so the code stays in the
   item. No marker is written. A code block outside every item and quote takes the plain key. A
   marker line whose text opens a fence counts as inside: `- ```rust|` writes `\n  `.
3. **A caret on a held line.** The new line copies the line's bytes from its start to its text
   start: its quote part and its hidden blanks. So `- a\n  b|` gives `- a\n  b\n  |`, a new line
   in the same paragraph. With the caret at the text start, the `\n` goes before those bytes, so no
   line is left with blanks only. A lazy line copies its own prefix, which may be empty: the user
   wrote it lazy.
4. **A caret on an empty item's line, at or past its text start** (D3).
5. **A caret on a marker line, at or past its text start.** The edit replaces the blanks just
   before the caret, down to the text start at most, with `\n` and the **continuation**:
   - the marker line's quote part, copied;
   - the bytes from the item part's start to the caret item's marker, copied when they are blanks
     only, else spaces up to the marker's column (`- - a` continues as `  - `);
   - the marker: the same bullet byte, or the number plus one with the same delimiter, its digits
     zero-padded to the old count (`09.` gives `10.`, `01.` gives `02.`);
   - the blanks between the marker and the content start, copied, or one space when the marker
     line has five columns or more (`CommonMark` 5.2, as `checkbox_at` counts them,
     `cycle.rs:148-162`);
   - for a task, `[ ] ` and nothing else: a checked task continues unchecked.

   The text after the caret moves to the new item: that is the split. `- it|em` gives
   `- it\n- |em`. Every line the old item held after the caret, and its children, move with it
   (Open question 3).
6. **A caret on a quote line with no item part.** The new line copies the quote part:
   `> a|` gives `> a\n> |`. An empty quote line, blanks only past its quote part, loses its
   innermost `>` and the blank after it instead: `> > |` gives `> |`, and `> |` gives an empty
   line, so the quote ends.
7. **A caret before the text start** of a marker line, in a revealed item part, takes the plain key.
   `|- a` moves the item down a line, as in Zed (case 8).
8. **A line holding an opener past its text start** takes the plain key. In `- > - a` the inner
   item is no gutter item; continuing the outer item there would be a guess.

Quote and list combine through rule 5's first bullet: `> - a|` gives `> - a\n> - |`, and
`> 1. [x] a|` gives `> 1. [x] a\n> 2. [ ] |`.

An ordered continuation renumbers the siblings below it (D6).

**The caret** lands after the continuation, at the new item's text start. `apply_diff` puts it there
with no extra field: the continuation is an insertion at the caret's offset.

## D3. `↩` on an empty item

- **Top level**: the edit deletes the item part's bytes and any blanks after them, from the item
  part's start to the line end. The quote part stays. So `- a\n- |` gives `- a\n|`, and the list
  ends. `> - |` gives `> |`, and the quote goes on: one level per `↩`.
- **Nested**: the item outdents one level, as `⇧⇥` does (D4). `- a\n  - b\n  - |` gives
  `- a\n  - b\n- |`. A second `↩` then leaves the list.
- The caret lands at the line's new text start: the quote part's end at the top level, the
  outdented item's text start when nested.
- An empty item that holds a child is no empty item: its marker line falls under D2's rule 5.

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
on a marker line or a held line. An item whose ancestor is also given moves with that ancestor
only. Consecutive siblings among the given items form a **run**.

**`⇥` on a run.** The run's first item needs a previous sibling, which becomes the run's parent.

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
  its own delimiter.
- A run whose first item has no previous sibling does not move. Nothing else is written: a first
  item cannot become a child (Open question 7).

**`⇧⇥` on a run.** The run's items need a parent; they become its siblings, right after it in the
source.

- The shift is the first item's marker column less the parent's marker column. The edit removes
  that many columns of blanks from the start of every line of the run's blocks, after the quote
  part. A line with fewer leading blanks, such as a lazy line, loses all it has. When the cut falls
  inside a tab, that line's leading blanks are first rewritten as spaces.
- The siblings after the run stay where they are in the source. Their indent now puts them under
  the last outdented item, so they become its children (Open question 5). That is what a text
  shift does in Zed and CodeMirror, and it moves no line.
- **The marker kind.** Each outdented marker takes the parent's kind: the parent's bullet byte, or
  the parent's number plus one, then counting up.
- A top-level item does not move, and `⇧⇥` writes nothing. Leaving the list is `⌫`'s job (D5).

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
at a marker line's text start. Any other `⌫` takes the plain key, the D2 atom included: a held
line's `⌫` still joins it to the line above (task boxes spec, D2). One step per press, first match
wins:

1. **An empty item** (`- |`, `1. |`, `- [ ] |`): the edit deletes the line's item part, its
   trailing blanks, and the `\n` before the line, so the caret lands at the previous line's end. So
   `↩` then `⌫` on an empty continued item restores the text and the caret (Open question 4). On a
   note's first line, only the item part goes. A quoted empty item keeps its quote part:
   `> - a\n> - |` gives `> - a|`.
2. **A task**: the checkbox and the one blank after it go. `- [ ] abc` gives `- abc`, and
   `1. [x] a` gives `1. a`.
3. **Any other item**, top level or nested: the marker and the blanks up to the text start go. The
   bytes before the marker stay, the quote part and the ancestors' indent. So `- a\n- |b` gives
   `- a\n|b`, a lazy line of `a`'s paragraph, which D11 already paints at `a`'s text. `- a\n  - |b`
   gives `- a\n  |b`, a held line of `a`. A top-level first item gives a paragraph. A further `⌫`
   reaches the atom or the plain key and joins the line above (Open question 6).

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
`EditorState::backspace` as today. `EditorView`'s returns `remove_markup`'s hunks as a `Diff` at the
buffer's version. `NameField` takes the default. The trait already has two implementers, so this
adds no trait with one.

**Delete and `⌘⌫`** are out of scope (Deferred). `⌘⌫` has no editor binding (`keymap.rs:72` is the
sidebar's).

## D6. Renumbering

After an edit adds, removes or moves an ordered item, the siblings of that item's list below the
edit get new digits, CodeMirror's rule (Open question 2):

- The walk starts at the first sibling after the edit, with the number the edited list now expects
  there. It rewrites a sibling's digits only, zero-padded to that sibling's own digit count.
- It stops at the first sibling whose number was not the previous one plus one before the edit, or
  whose marker kind differs. So a list the user numbers by hand, as `1. 1. 1.` or `1. 5. 9.`, is
  left alone.
- D4's two lists both renumber: the one an item leaves and the one it joins.
- The hunks are added to the command's own batch, so the whole command stays one undo step.

The walk costs the edited list's later siblings and their descendants, never the note.

## D7. The app

- `newline` (`actions.rs:138-141`): with an empty selection and no marked text, it calls
  `continue_list` on the cached parse. `Some(hunks)` goes through `apply_diff`, as `toggle_task`
  does; `None` inserts `\n` through `EditorState::insert`, so a plain `↩` still merges with typing.
- `indent_item` and `outdent_item`: the same shape, with `D4`'s fallbacks.
- The four handlers live in a new `crates/app/src/editor/lists.rs`; `actions.rs` declares the two
  actions and registers them, as it does `ToggleTask`.
- `keymap.rs` gains `tab` → `IndentItem` and `shift-tab` → `OutdentItem` in the editor context.
  `ctrl-tab` and `ctrl-shift-tab` stay `AppShell`'s (`keymap.rs:98-99`).
- Each list command is one undo step, since `apply_diff` transacts on its own.

`inkra-markdown` gains `crates/markdown/src/list_edit.rs`, with one submodule per command
(`newline.rs`, `indent.rs`, `backspace.rs`) and `renumber.rs`, their tests in their own files. It
exports `continue_list`, `indent_items`, `outdent_items` and `remove_markup`. No marker grammar is
written there: every marker, item and quote comes from the parse, as in `cycle.rs`. Reading the
digits and the delimiter at a marker the parse found is `marker_end`'s job (`items.rs:285-290`).

## Performance

- Each list command reads the parse already cached for the buffer's version. It runs no
  pulldown-cmark pass: no command verifies its edit by parsing a copy, unlike `⌘L`.
- Finding the caret item costs two binary searches. A sibling or parent costs the depth.
- `↩` writes O(line) bytes. `⇥` and `⇧⇥` cost the moved blocks' lines. Renumbering costs the
  edited list's later siblings.
- The edit then reparses once through `refresh`, as any keystroke does.
- `Item` grows `Structure::items` from 8 to 24 bytes per item, filled in the existing pass. Measure
  `cargo bench -p inkra-markdown --bench parse -- --quick 'markdown_parse/500000$'` before and
  after L1; it should stay within noise.

## Tests

`inkra-markdown`, `#[test]` tables per rule, input and expected text with `|` for the caret:

- D2: every rule, each with a quoted variant; `- [x] a|`, `1) a|`, `09. a|`, `- - a|`, `-   a|`,
  `- it|em`, `- a |b` (the blank trimmed), `- a\n  b|`, `|- a`, `- > - a|`, a fence held by an item,
  a top-level fence, a lazy line.
- D3: top level, nested once and twice, inside a quote, an ordered parent.
- D4: one item, an item with children, a run, a run with a non-item line, a first item, a top-level
  `⇧⇥`, a tab in the leading blanks, a held code line, each marker-kind case.
- D5: each rule; a task with no text; the first line of a note.
- D6: an insertion, a removal, a move, a hand-numbered list, a kind change, `9.` to `10.`.
- No list command parses: a thread-local pass counter, as `cycle.rs` keeps (`cycle.rs:242-251`).

Proptests over generated lists (bullets, numbers, tasks, quotes, nesting up to four):

- **P1.** At a marker line's end with text, `continue_list` then `remove_markup` restores the text
  and the caret.
- **P2.** On a bullet list, `indent_items` then `outdent_items` on the same item restores the text.
  On a sequential ordered list it does too, once D6 is built.
- **P3.** `outdent_items` writes no non-blank byte outside the digits it renumbers, and `⇧⇥` with
  no item writes nothing.
- **P4.** After any list command, every opener of the text outside the command's own hunks still
  opens at its mapped byte, as `cycle.rs` checks it (`cycle.rs:213-222`). The test parses; the
  command does not.
- **P5.** After `continue_list` by rule 5, the new line holds a gutter item, and a task when the
  caret item was one (`Markdown::task_at`).

`app`, headless `#[gpui::test]` against `MemFs`, one `simulate_keystrokes` test per binding:

- `enter` continues `- a`, and on the empty item leaves the list.
- `tab` nests `- b` under `- a`; `tab` on a paragraph inserts `\t`.
- `shift-tab` outdents; `shift-tab` on a paragraph changes nothing.
- `backspace` turns `- [ ] a` into `- a` in the editor, and deletes one byte in the name field.
- `cmd-z` after each restores the text in one step.
- `keymap/tests.rs` passes: `tab` and `shift-tab` are bound in the editor context alone.

After L1 and L2, render the `markdown-lists` scene with a continued and a nested item, and look at it.

## Slices

Each slice is vertical and ends green on `./scripts/check.sh`, then runs the markdown mutants on the
diff (CLAUDE.md, Commands).

1. **L1, `↩` continues** (~1 d). D1's `Item`, D2 whole, the `newline` change, D2's tests, P1's
   continue half with the plain fallback, P4, P5. No renumbering yet: a split of `1. a` gives two
   `2.`s until L4. The parse bench before and after.
2. **L2, `⇥` and `⇧⇥`** (~1.5 d). D4, the two actions and bindings, the hide list, D4's tests, P2
   on bullet lists, P3. The scene.
3. **L3, the empty item and `⌫`** (~1 d). D3 on top of L2's outdent, D5 and `backspace_markup`,
   their tests, P1 whole.
4. **L4, renumbering** (~0.5 d), after Open question 2. D6 in all three commands, its tests, P2 on
   ordered lists.

## History

- 2026-11-19: drafted from the list-typing audit, after the owner approved chantier A.

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
- **From Zed, `⌫` and renumbering.** Zed has neither. Inkra follows CodeMirror's renumbering rule.
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
| An empty first child shown as an item, not a setext heading (Open question 1) | Chantier B, or the owner picks option b. |
| `Delete` at a marker line's end joins the next item's text without its markup | The owner asks; it mirrors D5. |
| `⌫` at a quote line's text start removes its `>` | The owner asks. |
| `↩` keeps a code line's own indent | Code editing gets auto-indent. |
| `⇥` and `⇧⇥` in the name field | A file name with a tab is reported. |
| Moving an item up or down with its children | The owner asks. |

## Risks

- **A setext heading on an empty first child.** `⇥` on an empty item under `- a` writes
  `- a\n  - `, which `CommonMark` reads as `a` with a setext underline (Open question 1). Typing one
  letter makes it an item.
- **Renumbering across a digit count.** `9.` to `10.` widens the content column by one, so a held
  line indented for `9. ` may turn lazy, and a child list indented by 3 may leave the item. Renumber
  rewrites digits only; the tests pin the case.
- **Tabs in a quote.** Columns count from the line's start, as `Blanks::scan` counts them
  (`gutters.rs:229-263`). A tab right after a `>` splits its columns between the quote and the
  item, so D4 rewrites such a line's leading blanks as spaces before it cuts.
- **Splitting moves children.** With D2's rule 5, `↩` at the end of `- a` with children gives the
  children to the new empty item (Open question 3).

## Open questions

1. **An empty first child reads as a setext heading.** `- a\n  - ` is `a` as a level-2 heading in
   `CommonMark`, until a letter is typed. It follows `⇥` on an empty item, and option B of question
   3.
   - a. Accept it until chantier B. Recommended: it lasts one keystroke, and the file stays
     `CommonMark`.
   - b. Inkra's parse reads a lone `-` line right under an item's paragraph as an empty item. The
     screen then differs from GitHub's rendering of the same bytes.
   - c. `⇥` on an empty item writes nothing until text arrives. That needs state in the view.
2. **Renumber the siblings below?**
   - a. Never, as Zed.
   - b. CodeMirror's rule, while the old numbers ran in sequence (D6). Recommended: a hand-numbered
     list stays as typed.
   - c. Always the whole list.
3. **`↩` at the end of an item that has children.**
   - A. A new sibling at the caret; the children move under it. Recommended: no line moves, and the
     new item is a second item, so no setext state.
   - B. A new first child. Workflowy and Notion do this; it shows the setext state of question 1.
   - C. A new sibling after the whole block. The caret jumps below the children.
4. **`⌫` on an empty item.**
   - a. It deletes the line and the `\n` before it. Recommended: `↩` then `⌫` restores the text,
     and P1 holds.
   - b. It deletes the markup and leaves an empty line, CodeMirror's first step.
5. **`⇧⇥` on an item with siblings below.**
   - a. In place: the siblings below become its children. Recommended: no line moves, as in Zed and
     CodeMirror.
   - b. The item and its block move after the parent's block. Outliners do this; it moves text.
6. **`⌫` at a bullet's or a number's text start.**
   - a. Delete the marker and its blanks; the line turns lazy or held. Recommended (D5).
   - b. Replace them with spaces, as CodeMirror does. The text keeps its column in the source, but
     a wide number's spaces can open indented code after a blank line.
7. **`⇥` on an item with no previous sibling.**
   - a. Nothing. Recommended: the result could not nest.
   - b. Indent anyway, as Zed does. The item would stay at its level and only move in the source.
