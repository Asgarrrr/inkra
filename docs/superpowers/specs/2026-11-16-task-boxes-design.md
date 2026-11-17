# Task boxes

Status: **approved** 2026-11-16, from a brainstorm with the owner. Replaces S3
of `2026-09-30-markdown-blocks-design.md` ("S3 as planned", 2026-11-11). Its
parser facts and the pending-slot rule carry over and are cited, not copied.
An audit and an advisor review (Opus) of that plan against `main` on
2026-11-16 found the defects this spec fixes; they are cited by letter (A–I).

## Why

`- [ ] a` shows today as dimmed text with its `-` replaced by `•`. A note app
needs a box you see at a glance and tick without retyping the marker.

## Decisions taken

Taken by the owner on 2026-11-16.

1. **Level 0 only.** A box, a click, a key. No progress count, tree guides,
   fold, extra statuses (`[/]`, `[-]`) or vault-wide task view. They are in
   Deferred; nothing here is built for them.
2. **A square box, accent when checked** (mockup C). Unchecked: a grey
   outline. Checked: filled in the accent colour, with a check. The checked
   item's own text is struck through and muted; its nested items are not.
3. **A click toggles.** One click is one undo step, and moves the caret to the
   item's text start (markdown-blocks D6).
4. **`⌘L` cycles the task state** of every line the selection touches: a plain
   line becomes `- [ ] `, `[ ]` becomes `[x]`, `[x]` becomes `[ ]`. `⌘↩` stays
   the link key. This is Obsidian's binding since 1.0, which moved the toggle
   off `Cmd+Enter` for the same conflict. The owner first picked a shared
   `⌘↩`; the review showed `link_at` accepts a caret just past a link
   (`crates/markdown/src/links.rs:72-81`), so `⌘↩` at the end of
   `- [ ] Call [[Bob]]` would follow the link instead of toggling.
5. **The `.md` file stays the only source of truth.** Every state is the
   three bytes `[ ]`, `[x]` or `[X]`. Obsidian and GitHub read the same file.

## Terms

- **checkbox**: the three bytes `[ ]`, `[x]` or `[X]`.
- **box**: the square painted over a concealed checkbox.
- **task part**: from the end of the line's quote part (or the line start)
  through the checkbox. In `  - [ ] a` it is `  - [ ]`; in `> - [ ] a` it is
  `- [ ]`.
- **text start**: the first byte after the checkbox and its one space or tab;
  the line's end for an item with no text.
- **box run**: what the box replaces (D10): a bullet task's bullet through its
  checkbox, an ordered task's checkbox alone.
- **lead**: the task part before the box run (D10). Blank in `- [ ] a` and
  `  - [ ] a`; `1. ` in `1. [ ] a` and in `1. - [ ] a`; `- ` in `- - [ ] a`.

## D1. The data

`Gutter` (`crates/markdown/src/gutters.rs:11-18`) gains
`task: Option<Task>`:

```rust
pub struct Task {
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}
```

A line gets a `Gutter` when it is quoted or starts a task item that gets a
box: D10's rule, which replaces markdown-blocks item 13. The pending-slot rule decides the item's marker (markdown-blocks,
item 7; Zed `markdown.rs:3820-3836`). The parser facts table of
markdown-blocks holds for pulldown-cmark 0.13.6 (`Cargo.lock:4068-4069`).

`parse` reads the text's last line as if a `\n` ended it: a text without a
final `\n` is parsed with one appended, so `- [ ]` there is a task with no
text, as `- [ ]\n` is. No clamp is needed: no span or marker holds a `\n`,
and every element range ends before its line ending, so no output reaches
the added byte. A proptest pins it (`parse/tests/last_line.rs`). Amended
2026-11-17 after T4a.

Where the code goes (audit): `parse.rs` is 381 lines, so the pending slot and
the task gutter are built in `gutters.rs` (125 lines). Task gutters merge into
the quote gutters' post-pass (`parse.rs:122-123`) in line order.

`Gutter::same` (`crates/markdown/src/changed.rs:144-153`) also compares the
task: the mapped checkbox and `checked`. **(A)** Without it, a toggle leaves a
carried line with a stale box.

## D2. The conceal

**(B)** The task conceal's element and marker are both the task part. The
old plan set the element to the checkbox alone, which breaks "the marker is
inside the element" (`parse/tests.rs:623-627`): `⌃A` on `- [ ] a` put the caret
at 0, nothing revealed, and typing landed before a hidden bullet.

- A caret anywhere in the task part, leading spaces included, reveals it.
- A caret at the text start does not touch the checkbox, so after a click the
  box stays painted.
- Replacement `""`. The conceal's marker is the box run, not the whole task
  part: an ordered task keeps its number (D10). Amended 2026-11-17 after T4b.
- The quote part and the task part reveal separately (S1 review): in
  `> - [ ] a` a caret on `>` reveals the quote part only.

## D3. The gutter and its width

The gutter is a hanging indent: text after it, wrapped rows included, starts
at its right edge (markdown-blocks D3, as built in S1). A revealed part paints
inside it, so no reveal moves text.

**The prefix in pieces.** Markdown-blocks' "The prefix in pieces" holds, with
today's locations: `line_prefix` is in `layout/prefix.rs:104-159`;
`GutterCarry::layout` is in `layout/prefix/gutter.rs:133-169`, `Geometry` at
93-101, `measure` at 187-202. A revealed part shapes the whole gutter text once and splits it with
`ShapedLine::split_at` (`vendor/zed-reference/crates/gpui/src/text_system/line.rs:196`).
The `debug_assert!` at `prefix.rs:122-129` admits the task conceal as the last
own conceal.

**(C) A toggle never changes the width.** The body font is proportional, and
`GutterCarry` measures each distinct gutter text (`prefix/gutter.rs:142-155`), so
`- [ ] ` and `- [x] ` would differ by a few pixels and could rewrap the line.
The gutter is measured on its text with the checkbox's middle byte replaced
by `X`. Every state shares one measure and one width. Amended 2026-11-16
during T1a by the owner: with a space, a revealed `[x]` touched the text and
`[X]` overlapped it, breaking "a revealed part paints inside it". `X` is the
widest of the three bytes in the body font, at the cost of a few pixels of
gutter.

**`Geometry` gains the checkbox's `x` span** and the quote part's width. Both
come from the same measure. `GutterLayout` holds `Geometry` by value
(`prefix/gutter.rs:15-23`), so every gutter line gains a `Pixels` and an
`Option<Range<Pixels>>`. `GutterLayout` also gains `boxed: Option<bool>`: the
line's own `checked`, set only while its task part is concealed. The 256-byte
assertion on `LineLayout` (`line.rs:95`) stays the check, and a breach boxes
the new fields.

**(D) A lazy line carries the quote part only.** `GutterCarry::pass` carries
`Geometry::quote_part`, never the whole `Geometry` (`prefix/gutter.rs:124-129,
204-211`), and `layout` gives it to a lazy line (156-158). Under `> - [ ] a\n  b`,
line `b` takes the quote part's width and paints no box. A box is painted from
the line's own `task`, never from carried geometry.

## D4. Paint and hit test, per line

No box vector. **(E)** The old plan built box bounds in `shape_document`
beside `bars`; `layout/mod.rs` is at exactly 400 lines, the limit for an
unlisted file (`scripts/line-ceilings.sh:7`), and the vector is a second
O(lines) pass per keystroke for data each line already holds.

- **Paint.** The visible-line loop paints a box on each line whose gutter has
  a task and whose task part is concealed. The box is a square of
  `TASK_BOX_SIZE`, centred on the checkbox's `x` span and on row 0. It paints
  with the gutter, after the selection (`text_input/paint.rs:103-106`).
  Unchecked: `outline` in `md_marker`. Checked: `fill` in `accent`, then the
  check with `window.paint_svg` in `editor_background`. Both from
  `vendor/zed-reference/crates/gpui/src/window.rs` (`paint_quad` 4934,
  `paint_svg` 5235, `fill` 8128, `outline` 8140).
- **Hit test.** `DocumentLayout::task_at(local) -> Option<(Diff, ByteOffset)>`,
  the task's flip and its text start (D5): find the line
  under `local.y`, require **(F)** row 0 of that line with `y` inside the row
  (`row_for_y` clamps, `layout/mod.rs:252-265`, so a click below the last line
  would hit its box), require the task part concealed, then require `x` inside
  the box's square. A revealed checkbox is text; a click on it places the
  caret.

The check icon is Lucide `check`, added to `crates/app/assets/icons/` and to
the `Assets` enum (`crates/app/src/ui/assets.rs`) under its existing licence.

## D5. The click

`on_mouse_down` (`crates/app/src/editor/mouse.rs:34-65`) calls `task_at`
after focusing and before the `⌘`-click link branch (46-59).

- A plain click on a box toggles and returns before `is_selecting` (60), so
  it starts no drag selection.
- A click with `⌘` or `⇧` held skips the box: `⌘`-click keeps following
  links, `⇧`-click keeps extending the selection.
- The click that activates an inactive window skips the box too
  (`first_mouse`). It only activates the window, as Electron's default does.
  Added 2026-11-17 by the owner after the T2 review.
- **(G)** The click's edit is an `inkra_text::Diff` based at the layout's
  version (`shaped_at`, `layout/mod.rs:81-82`), and `Buffer::apply_diff`
  refuses a diff whose base is not the buffer's version. After a reload, the
  last layout describes the old text until the next render
  (`editor/reload.rs:24-27`); three matching bytes would not prove the task is
  the same one. A mismatch edits nothing, and the click places the caret as
  any click does. Zed only debug-asserts here
  (`markdown_preview_view.rs:1220`).
- The edit is `Task::flip`: the middle byte (`x` for a space, a space for `x`
  or `X`), applied through `EditorState::apply_diff`. It runs one
  `Buffer::transact`: one undo step, merged with neither neighbour. The click
  then collapses the caret at the text start the layout holds, the gutter's
  end, mapped through the flip. The byte count is unchanged, so the text start
  does not shift. Amended 2026-11-17 after the cross-slice review: the editor
  no longer restates the text-start rule of `gutters.rs`.
- A double click toggles twice: each press is one click (decision 3).

## D6. `⌘L`, the cycle

A new action `editor::ToggleTask`, bound to `cmd-l` in the editor context in
`crates/app/src/keymap.rs`. The command palette lists it as "Toggle task" by
its name; nothing else registers it.

**The lines.** Every line the selection touches. **(H)** A selection that
ends at column 0 of a later line drops that line, as Zed does
(`vendor/zed-reference/crates/editor/src/editor.rs`, around 5595 and 12843).

**Skipped lines.** Lines inside a code block, the front matter, a table, or an
HTML block. `Markdown` records no table range (`parse.rs:12-24`), so the splice
function runs its own pulldown-cmark pass with the shared options (`cmark.rs`)
and collects the ranges of `Tag::CodeBlock`, `Tag::MetadataBlock`,
`Tag::Table` and `Tag::HtmlBlock`. HTML blocks amended 2026-11-17 after the
T4a review: `⌘L` on `<div>` would make a task of it. One pass per keystroke of `⌘L` is not a per-frame cost. In a
selection of more than one line, blank lines are skipped too. **(I)** Without
this, `⌘A ⌘L` rewrites the YAML.

**The next state is the first cycled line's.** Every cycled line takes the
state that follows the first one's, so a mixed selection lands uniform:

| Line | Becomes |
|---|---|
| `- [ ] a`, `1. [ ] a` | `- [x] a`, `1. [x] a` |
| `- [x] a`, `- [X] a` | `- [ ] a` |
| `- a`, `* a`, `1. a` | `- [ ] a`, `* [ ] a`, `1. [ ] a` |
| `a`, `> a`, `  a` | `- [ ] a`, `> - [ ] a`, `  - [ ] a` |
| an empty line, a single-line selection | `- [ ] ` |

The marker is read from the line's text: leading `[ \t>]*`, then a bullet
(`-`, `*`, `+`) or a number with `.` or `)`, then a checkbox. A checked or
unchecked line changes only the middle byte. A plain line inserts `- [ ] `
after its leading `[ \t>]*`; a bullet line inserts `[ ] ` after its bullet and
space. Obsidian swaps a bullet for a checkbox the same way (forum 45383), and
has turned plain text into a task since 0.13.24 (forum 14634).

**The edit.** `inkra-markdown` computes the splices as data, a pure function
of the text, the parse and the line range: no buffer, testable by proptest.
`⌘L` builds an `inkra_text::Diff` from them at the buffer's version and
applies it through `EditorState::apply_diff`, the click's path:
`Buffer::apply_diff` runs one `Buffer::transact` back to front, then
`map_through` maps the selection with Zed's biases. An offset after a splice
shifts by its length delta, and a caret at an insertion's position moves after
the inserted text: a caret at the start of `a` lands before `a` in `- [ ] a`,
not before the `-`. A caret on a replaced byte lands after it. A second `⌘L`
cycles the same lines, and typing after `⌘L` lands where the caret was.
Amended 2026-11-17 after the cross-slice review: no batch edit of its own.

**The batch, checked by one parse.** Amended 2026-11-17 for T4b. The lines
are the selection's first line and every later line that starts before the
selection's end, which is (H). A blank line holds only spaces and tabs. The
state comes from the parse, not from the line's text: the target is checked
when the first cycled line's gutter has an unchecked task, else unchecked.
Each cycled line has its candidate splices, in order:

- a task in the other state: its flip;
- a task already in the target state: none;
- any other line: the insertion after its bullet (D6's table; an empty item
  `-` gains ` [ ] `), then `- [ ] ` after its leading `[ \t>]*`. The
  checkbox holds the target's byte, so under a ticked first line `c` becomes
  `- [x] c`.

A round applies every line's current candidate to one copy of the text and
parses it once. A line whose shifted start then has a task in the target state
keeps its splice; any other line moves to its next candidate, or drops out
after its last.

**No markup turns to text.** Amended 2026-11-17 after the T5 review. A round
also fails a splice when any line of the copy opens fewer list items or block
quotes than the same line of the text: `- [ ] ` before `> a` would make the
quote text. The splice failed is the nearest cycled line's at or above the line
that lost one: no splice holds a newline, so lines keep their indices, and a
splice reaches only its own line and those below it, as `---\n>\n---` turning
into front matter shows. The counts come from one pulldown-cmark pass of the
text, the skipped lines' pass, and one more pass of each round's copy. So
`- > a` has no candidate that keeps its quote, and `⌘L` edits nothing. The first round in which every splice landed returns its batch.
After three rounds the batch is empty.

The argument: a batch is returned only from a round whose own parse shows each
of its lines a task in the target state, and no line with fewer items or quotes. Every splice lies inside its line, so
the result holds by construction, whatever a splice does to its neighbour's
laziness; the empty batch holds trivially. A failing round moves at least one
line on through at most two candidates, so the rounds end. The cap of three
covers the second candidates (round 2), then the batch without the lines no
candidate makes a task (round 3). A longer chain edits nothing rather than
guess.

So D1 and D6 stay one predicate, and the batch is judged whole: `a\n    b`
selected cycles both lines, `b` nesting under the new item, though `b` alone
cannot be a task. Likewise `a\n2. b` becomes `- [ ] a\n2. [ ] b`: after the
new item `2.` starts a list, though alone it is lazy and takes `- [ ] 2. b`. A
`⌘L` costs the cached parse, one pulldown-cmark pass for the skipped lines,
and at most three parses of the spliced copy, each with one more pass for its
items and quotes. This replaces T4a's per-line
reparse, which cost up to two parses per line and judged each line alone.

## D7. The struck text

`Styles` gains `DONE` (`crates/markdown/src/styles.rs`, next free bit
`1 << 12`). It is set on a checked task item's own text: every paragraph
directly inside the item, continuation and lazy lines included.

- **Set where the marker is decided.** The pending slot (D1) resolves after
  `Start(Item)`, or after one `Start(Paragraph)` in a loose item. When it
  resolves a checked task that gets a gutter, it adds `DONE` to the item's
  `Open` and, in a loose item, to the already open paragraph's `Open`. Later
  paragraphs of the item inherit it through `Open::enter` (`parse.rs:255-258`).
- **Not inherited by a block.** `DONE` is dropped when a list, a quote or a
  code block opens inside the item. Nested items are not struck. Any other
  block of the item (a heading, a table, an HTML block) inherits it and is
  struck. Added 2026-11-17 after the T3 review: these are the item's own text
  as much as a paragraph is.
- **Markers are not struck.** The contract: `DONE` is on every non-`MARKER`
  span of the item's own text, and a `MARKER` span may carry it.
  `markdown_style` (`crates/app/src/theme/markdown.rs`) strikes `DONE` only
  when `MARKER` is absent, and colours it `md_marker`. Gaps are styled
  `open.indent | MARKER`; the resolution adds `DONE` to `indent` too, so a
  continuation line's indent carries it. A loose item's own marker does not:
  its gap is styled before the slot resolves on the paragraph. Amended
  2026-11-17 after the T3 review.
- Continuation and lazy lines restyle through the span diff
  (`changed.rs:113-115`), so a toggle restyles every row of the item.
- **A wrapped row strikes its trailing space.** The space that ends a wrapped
  row is the item's text, so the line runs past the last word. Accepted by the
  owner 2026-11-17 after T3.

One predicate decides both the box and `DONE`: the pending slot resolving a
task that gets a box (D10). A task that gets none (`- > - [ ] a`) gets neither
box nor strike, so `⌘L` can never make a struck line without a box.

## D8. Theme

No new colour token: `md_marker`, `accent` and `editor_background` exist
(`crates/app/src/theme/colors.rs`). The token count test
(`crates/app/src/theme/file/tests.rs:9`) stays at 41. `theme.rs` gains
`TASK_BOX_SIZE` (16 px), `TASK_BOX_RADIUS` (4 px), `TASK_BOX_BORDER` (1.5 px)
and `TASK_CHECK_SIZE` (12 px) beside `QUOTE_BAR_WIDTH` (`theme.rs:292`). They
come from mockup C: a 15 px box with a 4 px radius and a 1.5 px border at
15 px text. At the 18 px body the box takes 16 px, the check 12 px. A
distinct box colour waits for a theme that asks for one.

## D9. Incremental draw

A toggle is a one-byte splice in the task's own line
(`changed.rs:17`). A reveal flip dirties its line through `Carry::flipped`
(`dirty.rs:69-79`). With `Gutter::same` comparing the task (D1), no carried
line keeps a stale box or a stale strike. The layout oracle of the
incremental-draw spec (D10) gains toggles and `⌘L` batches in its alphabet;
its alphabet already holds tasks.

## D10. The markers before the box

Added 2026-11-17 after T4b, by the owner: "la spec est mauvaise". Until then a
concealed task part painted nothing but its box, so `1. [ ] a` lost its number
and showed a gap where it stood. Markdown-blocks item 13 also gave no box to
`- - [ ] a` or `1. - [ ] a`, and `⌘L` on them added a second checkbox. The
number is content, the order of the steps. Only the task's own bullet gives way
to the box, as Obsidian draws it.

**The box rule.** A task item gets a box when its line holds all its leading
quotes and, between the quote part's end and the item's marker, no `>`.
CommonMark opens a list item mid-line only after container markers, so that gap
holds only spaces, tabs and the markers of items that open on the line. `- [ ] a`,
`  - [ ] a`, `> - [ ] a`, `1. [ ] a`, `- - [ ] a` and `1. - [ ] a` get a box.
`- > - [ ] a` and `- a\n  > - [ ] b` do not: a quote inside a list item has no
gutter (markdown-blocks, As built item 1).

**The data.** `Gutter::quote_end` moves into `Task`, the one place it differs
from `range.end`:

```rust
pub struct Task {
    /// Where the task part starts: past the quote part's last `>` and its blank.
    pub start: ByteOffset,
    /// Where the box run starts: the bullet, or an ordered task's checkbox.
    pub run: ByteOffset,
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}
```

The quote part ends at `task.start` on a task line and at `range.end` on any
other gutter.

**The conceals.** The task conceal's element stays the task part, so a caret
anywhere in it reveals all of it raw (D2). Its marker becomes the box run.
The markers of the outer items in the lead keep their own conceals: a bullet's
`•`. Those conceals now start inside the gutter, so they are its own: the
`debug_assert!` in `line_prefix` counts the quotes, the lead's bullets, then the
task conceal, in marker order. The G-property in `parse/tests/gutters.rs`
admits the lead's one-byte bullets between the quotes and the task conceal.

**The paint.** A concealed task part paints its lead, then the box.

- The lead is shaped from the gutter's text through all its conceals, so `- `
  shows `• ` and `1. ` shows as typed, with the body's runs (`show` in
  `layout/runs.rs`). Every one lies inside the task part's element, so while the
  task part is concealed they are all active: the set an `is_active` filter
  would keep. It paints at the task part's `x`, the quote part's width.
  A blank lead shapes nothing.
- The box stays centred on the checkbox's `x` span. In `1. [ ] a` it sits
  right after `1. `; in `- [ ] a` it still sits where `[` stands.
- The width is unchanged: the raw text with the `X` (C). No reveal moves text,
  and a `•` narrower than `-` paints inside the gutter.
- A click on the lead lands as on a concealed part, at the text start. Only
  the box toggles (D4).
- The lead is measured once per distinct gutter text, with the geometry
  (`GutterCarry::measured`); it depends only on that text at the body size.

**One task box in the layout.** `Geometry::checkbox` and `GutterLayout::boxed`
describe one thing, measured in two places. They merge into one
`Option<TaskBox>` on `GutterLayout`: the checkbox's `x` span, `checked`, and
the lead's shape. The lead's `x` is the geometry's quote part width, and the
click flips the task of the layout's own parse, so `TaskBox` holds no offset. It is set only on a line
holding its own task, never from carried geometry (D). The task part's
concealed state stays the reveal's, so the box and the lead paint together.

**`⌘L`.**

- A plain line's insertion goes after its innermost marker: past the leading
  `[ \t>]*`, every bullet or number followed by one to four blanks opens the
  next, in turn (CommonMark 5.2), and the checkbox goes past the innermost's
  first blank. `- 1. a` becomes `- 1. [ ] a`, a task item inside the outer one,
  not `- [ ] 1. a`, whose `1. a` would be text; `-  - a` becomes `-  - [ ] a`.
  The second candidate, `- [ ] ` after the leading `[ \t>]*`, is unchanged.
- No splice turns markup into text (D6, amended after the T5 review): `- > a`
  edits nothing.
- A line holding a task that gets no box is skipped, like a code line: its
  checkbox cannot be cycled, and a box on its outer item would make two
  checkboxes. The skip pass (`Scan::new` in `cycle.rs`) collects the lines of
  its `TaskListMarker` events; a line whose gutter has a task is kept.

## Tests

`inkra-markdown`, by example:

- `- [ ] a`, `- [x] a`, `- [X] a`, `* [ ] a`, `1. [ ] a`, `> - [ ] a`,
  `  - [ ] a`, `-    [ ] a`, loose `- [ ] a\n\n- [x] b`: a gutter with a task.
- `- [ ]a`: no task. `- [ ]\n` and `- [ ]` at the end of the text: a task with
  no text. `-\n`: keeps its `•`. `- - [ ] a`: a gutter task with the outer `•` in its
  lead (D10); `- > - [ ] a`: no gutter task.
  `- a\n  - [ ] b`: the inner line has a gutter task.
- The conceal covers the task part; a caret at 0 in `- [ ] a` touches it; a
  caret at the text start does not.
- The struck text (`DONE` without `MARKER`) of `- [x] a` is `a` only; it
  covers a continuation line's text but not its indent, every paragraph of a
  loose item, and no nested item, quote or code block inside the item.
- The cycle: every row of D6's table; a selection ending at column 0; a code
  block, front matter and a table row skipped; a mixed selection lands on the
  first line's next state; a splice that changes the next line's laziness.

`inkra-markdown`, proptest:

- The conceal properties and the G-properties, changed for a task gutter:
  `depth` may be 0 with a non-empty range when `task` is set
  (`parse/tests/gutters.rs`, around 228-247).
- The cycle: applying its splices to the text and reparsing gives the stated
  state on every cycled line and leaves every other line byte-identical; no
  line opens fewer items or quotes; the mapped selection stays inside the text
  and on a char boundary.

`inkra-editor`: `apply_diff` is one undo step, merges with no neighbour, maps
the selection, and edits nothing from a stale base.

`app`, headless `#[gpui::test]` against `MemFs`:

- Revealing a task part moves no text; toggling does not change the gutter
  width.
- In `> - [ ] a` a caret on `>` reveals the quote part only; a lazy line under
  it takes the quote part's width and paints no box.
- A click on a box toggles `[ ]`↔`[x]` and moves the caret to the text start;
  `[X]` toggles to `[ ]`; `⌘Z` restores the text; a click on a revealed
  checkbox places the caret and edits nothing; a press on a box then a drag
  selects nothing; `⌘`- and `⇧`-click do not toggle; a click below the last
  line does not toggle its box; a click on a layout older than the buffer
  edits nothing; a toggle with the view scrolled away from the old caret does
  not scroll.
- `simulate_keystrokes("cmd-l")`: on a plain line, on a task, on a selection
  of three lines; `cmd-enter` on a task line holding a link still follows it.
- The layout oracle with toggles.

Scene: the `markdown-blocks` scene holds no task
(`shot/scenes/markdown_blocks.rs:7-18`). It gains `- [ ] a`, `- [x] done`,
a nested task, `1. [ ] a`, `> - [ ] a` with a lazy line, and a checked item
that wraps. `shows` is updated (`shot/scenes.rs:439-448`). Render it and look:
`cargo run -p inkra-app --features shot --example shot markdown-blocks`.

Mutants after each slice: `markdown` and `editor` on the diff, and `app` with
the command from `CLAUDE.md`.

## Slices

Each slice is vertical and ends green on `./scripts/check.sh` with its scene
rendered and looked at.

1. **T1a, the task gutter** (~0.5 d). D1, D2, D3 and `Gutter::same`: the
   marker hides into a fixed-width hanging gutter, reveals in place, and the
   quote part reveals apart. No box yet; the gutter shows blank. The
   `markdown` tests and the `app` reveal, width and lazy-line tests; the
   scene.
2. **T1b, the box paints** (~0.5 d). The paint half of D4, D8, the check
   icon, `TASK_BOX_SIZE`; the scene shows boxes, checked and not, and none on
   a lazy line.
3. **T2, the click** (~0.5 d). `task_at`, D5, its `app` tests, the oracle with
   toggles.
4. **T3, the strike** (~0.5 d). D7 and its tests; the scene's checked lines
   now read struck.
5. **T4a, `⌘L` on one line** (~0.5 d). D6 for an empty selection: the splice
   function with its table and skip rules and their proptest, a one-splice
   edit, the action and binding, the caret mapping, `cmd-l` and `cmd-enter`
   keystroke tests.
6. **T4b, `⌘L` on a selection** (~0.5 d). The batch checked by one parse
   (D6, amended for T4b) through `apply_diff`, the column-0 rule, the uniform
   next state, the mapped selection, the oracle with batches.
7. **T5, the markers before the box** (~0.5 d). D10: the box rule, `Task`'s
   `start` and `run`, the box-run conceal, the lead's paint, one `TaskBox`,
   `⌘L` after the innermost marker and the unboxed-task skip. Tests:
   `1. [ ] a` keeps `1.` and its box follows it; `- - [ ] a` paints `•` and a
   box; `1. - [ ] a` paints `1.` and a box; `- > - [ ] a` gets no box and
   `⌘L` leaves it; a caret in the lead reveals the whole task part raw;
   toggling keeps the width; `⌘L` on `- 1. a` gives `- 1. [ ] a`. The scene
   gains `1. [ ] a`, `- - [ ] a`, `1. - [x] a` and a revealed ordered task.

T1a ships before anything else depends on it, because it changes what `parse`
emits for every list item. T3 and T4a are independent of each other.

## Deviations

- **From Zed.** Zed renders a task as a `Checkbox` element in its separate
  preview pane (`vendor/zed-reference/crates/markdown/src/markdown.rs:2999-3032`)
  and writes all three bytes on toggle, debug-asserting lower-case `[x]`
  (`markdown_preview_view.rs:1201-1229`). Inkra paints a box over the
  concealed checkbox in the live editor, writes the middle byte so `[X]`
  toggles too, checks the version before writing (G), and moves the caret. Zed's
  editor has no toggle action; `⌘L` follows Obsidian.
- **From markdown-blocks S3.** The conceal covers the task part (B); width is
  state-independent (C); boxes are found per line, not from a vector (E); no
  `marks.rs` module and no new colour tokens; strike and key added.
- **From this spec, in T1a.** `Geometry` gains the quote part's width in T1a
  and the checkbox's `x` span in T1b, where paint first reads it: an unread
  field fails clippy's `dead_code`. `prefix.rs` is near 400 lines after T1a;
  T1b plans its split.
- **From this spec, in T2.** `task_at` returns the task's flip as a `Diff`
  at the layout's version, and its text start, the gutter's end; `apply_diff`
  makes the check (G). `layout/mod.rs` is at its 400 lines and gains no
  accessor. Amended 2026-11-17 after the cross-slice review: `task_at` took
  the buffer's version and the editor re-checked the bytes, so (G) ran twice.
  A carried line holds no document offset of its task: since T5,
  `task_at` reads the task from the layout's own parse. The hit test (F) is
  containment in the painted square, which fits inside row 0
  (`TASK_BOX_SIZE` 16 px, the smallest row that holds a task 28 px), so it
  has no separate row check.
- **From this spec, in T4b.** Every failing line moves to its next candidate
  in the same round, so a line can run out before the line above it settles:
  with `2. b` and `    c` selected after `x`, `c` is lazy under the failing
  `2. [ ] b` and drops out, though `- [ ] 2. b` would let it nest. Moving only
  the first failing line would land it but cost a parse per line on `⌘A`; the
  cap of three parses wins. Added 2026-11-17 after the T4b review.
- **From this spec, in T5.** Added 2026-11-17 after the T5 review.
  - `TaskBox` holds no checkbox offset. `Task` gained `start` and `run`, so a
    line-relative copy could not rebuild the whole `Task` to flip it. `task_at`
    finds the task in the layout's own parse, `DocumentLayout::markdown`, by the
    line's start; (G) is unchanged.
  - The lead shapes through all its conceals, with no `is_active` filter. The
    cache is keyed by gutter text, so the first line with that text may have its
    task part revealed, and a filter would cache a raw lead. All are active
    whenever the task part is concealed, so the set is the same.
  - The skip pass reads the text with a final `\n`, as `parse` does (D1).
    Without it, `- > - [ ]` on an unterminated last line has no
    `TaskListMarker`, and `⌘L` would add a second checkbox.
  - `show` moved from `shape.rs` to `runs.rs`, beside `line_runs`. `shape.rs`
    was at its line ceiling, and the `hidden` argument grows its closure.

## Deferred

| Item | Trigger |
|---|---|
| Progress count on a parent task, tree guides, folding children | The owner asks (level 1 of the brainstorm). |
| Extra statuses `[/]`, `[-]`, `[>]` | The owner asks (level 2); needs our own marker scan, pulldown-cmark reads only space, `x`, `X`. |
| Dates, priorities, a vault-wide task view | The owner asks (level 3). |
| `↩` at the end of a task starts `- [ ] ` | List continuation is built. |
| A pointer cursor over a box | A link gets one first. |
| A distinct box colour token | A theme asks for one. |

## Open questions

1. **After `⌘Z` the caret sits inside the checkbox**, so the task part shows
   as text until the caret leaves. Recommended: accept; the reveal rule says
   so.
2. **`- [ ]` with no newline at the end of the note is no task.** Decided:
   reversed by the owner 2026-11-17 after T4a: the last line parses as if a
   newline followed it (D1). `⌘L` on that line added a second checkbox.
3. **A click on the box of a task with no text (`- [ ]\n`) reveals its
   marker.** Its text start is the conceal's end, and a caret at a conceal's
   end touches it (`crates/markdown/src/reveal.rs:15`). Recommended: accept;
   typing after it hides the marker again.
