# Task boxes

Status: **approved** 2026-11-16, from a brainstorm with the owner; T6a and T6b approved by the
owner on 2026-11-18, then reshaped the same day by the owner's answers to the T6 review and to
Open questions 5 and 6. Replaces S3 of
`2026-09-30-markdown-blocks-design.md` ("S3 as planned", 2026-11-11), whose parser facts and
pending-slot rule are cited, not copied. An audit and an advisor review (Opus) of that plan found
the defects this spec fixes; they are cited by letter (A–I). D1–D10 state the design as it stands
after T6; History records how it got there.

## Why

`- [ ] a` showed as dimmed text with its `-` replaced by `•`. A note app needs a box you see at a
glance and tick without retyping the marker.

## Decisions taken

By the owner, on 2026-11-16 unless dated.

1. **Level 0 only.** A box, a click, a key. No progress count, tree guides, fold, extra statuses
   (`[/]`, `[-]`) or vault-wide task view. They are in Deferred; nothing here is built for them.
2. **A square box, accent when checked** (mockup C). Unchecked: a grey outline. Checked: filled in
   the accent colour, with a check. The checked item's own text is struck through and muted; its
   nested items are not.
3. **A click toggles.** One click is one undo step, and moves the caret to the item's text start
   (markdown-blocks D6).
4. **`⌘L` cycles the task state** of every line the selection touches: a plain line becomes
   `- [ ] `, `[ ]` becomes `[x]`, `[x]` becomes `[ ]`. `⌘↩` stays the link key. This is
   Obsidian's binding since 1.0, which moved the toggle off `Cmd+Enter` for the same conflict.
5. **The `.md` file stays the only source of truth.** Every state is the three checkbox bytes: `[`,
   a middle byte, `]`. The middle byte is `x` or `X` when checked, and a space, a tab, a vertical
   tab or a form feed when unchecked, as pulldown-cmark reads it (D6). Obsidian and GitHub read
   the same file.
6. **A task line takes its natural width** (2026-11-18). Concealed, its gutter holds the lead as it
   reads, then the box in the bullet's place, in a slot as wide as `• `. Revealed, its gutter widens
   to the raw text of its quote part and task part, and paints that text raw. So a reveal moves the
   line's text, as a revealed `**` does; "no reveal moves text" never held for bold either.

## Terms

- **checkbox**: the three bytes `[`, a middle byte, `]`. pulldown-cmark reads `x` or `X` as
  checked, and a space, a tab, a vertical tab or a form feed as unchecked
  (`pulldown-cmark-0.13.6/src/scanners.rs:386-394`).
- **task part**: from the end of the line's quote part, or the line's start, through the checkbox.
  In `  - [ ] a` it is `  - [ ]`; in `> - [ ] a` it is `- [ ]`.
- **box run**: what the box stands for: a bullet task's bullet through its checkbox, an ordered
  task's checkbox alone.
- **lead**: the task part before the box run. Empty in `- [ ] a`, blank in `  - [ ] a`, `1. ` in
  `1. [ ] a` and in `1. - [ ] a`, `- ` in `- - [ ] a`.
- **box**: the square painted for a concealed task part.
- **box slot**: the box and the gap after it: the advance of `• ` shaped with the body's runs, or
  `TASK_BOX_SIZE` where that is wider. It depends on the font only, so it is one width in every
  checkbox state.
- **text start**: the first byte after the checkbox and its one space or tab; the line's end for an
  item with no text.
- **opener**: a list item or a block quote that opens in the text, at its byte: an item's marker, a
  quote's start.
- **content start**: the byte where an item's text starts, or would start, on its marker line (D6).
- **fallback**: the pulldown-cmark pass over one line's text, after its prefix, that gives a
  content start on a line where no item opens (D6).

## D1. The data

`Gutter` (`crates/markdown/src/gutters.rs:12-23`) has `task: Option<Task>`
(`gutters.rs:25-34`):

```rust
pub struct Task {
    /// Where the task part starts: past the quote part's last `>` and its blank.
    pub start: ByteOffset,
    /// Where the box run starts: the bullet, or an ordered task's checkbox.
    pub run: ByteOffset,
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}
```

A line gets a `Gutter` when it is quoted or starts a task item that gets a box (D10). A task
gutter's range runs through the checkbox and its one space or tab. The quote part ends at
`Gutter::quote_end` (`gutters.rs:36-42`): `task.start` on a task line, `range.end` on any other.

- **The marker.** The pending-slot rule decides an item's marker (markdown-blocks, S3 correction 7;
  Zed `vendor/zed-reference/crates/markdown/src/markdown.rs:3820-3836`). `Items`
  (`gutters.rs:59-128`) resolves it at the item's first event that is not one `Start(Paragraph)`.
  The parser facts table of markdown-blocks holds for pulldown-cmark 0.13.6
  (`Cargo.lock:4068-4069`).
- **The merge.** Task gutters take their line's place among the quote gutters, in line order, after
  the parse's one pass (`Items::gutters`, called at `crates/markdown/src/parse.rs:140`).
- **One line.** pulldown-cmark reads a checkbox only on its item's marker line. It scans the task
  marker right after the list marker, and only when the rest of that line is not blank
  (`pulldown-cmark-0.13.6/src/firstpass.rs:139-161`). `task_gutter` states this as a
  `debug_assert!`, and `-\n  [ ] a` is no task.
- **The last line.** `parse` reads the text's last line as if a `\n` ended it, so `- [ ]` there is a
  task with no text, as `- [ ]\n` is. No output reaches the added byte: no span or marker holds a
  `\n`, and every range ends before its line ending. A proptest pins it over every offset
  `Markdown` holds (`parse/tests/last_line.rs`).
- **One lookup.** `Markdown::task_at(line_start) -> Option<&Task>` finds the task whose gutter
  starts at a line's start, by binary search over `gutters`. `cycle.rs`, `DocumentLayout::task_at`
  and the tests call it.
- **(A)** `Gutter::same` (`crates/markdown/src/changed.rs:139-162`) compares the task: its mapped
  `start..run` and checkbox, and `checked`. Without it, a toggle leaves a carried line with a stale
  box.

**The structure `⌘L` reads.** `Markdown` has `structure: Structure`, filled in the parse's one
pulldown-cmark pass (`parse.rs:82-139`):

```rust
pub struct Structure {
    /// Each list item that opens, in document order.
    pub items: Vec<ListItem>,
    /// Each block quote's start, where pulldown-cmark starts its range, in document order.
    pub quotes: Vec<ByteOffset>,
    /// Each code block, front matter, table, HTML block and ATX heading, from its first line's
    /// start to its last line's end, and each line holding a task with no box. Sorted; none holds
    /// another.
    pub skipped: Vec<Range<ByteOffset>>,
}

pub struct ListItem {
    /// The bullet, or the digits and their `.` or `)`.
    pub marker: Range<ByteOffset>,
    /// The content start (D6); the marker's end when the line ends there.
    pub content: ByteOffset,
}
```

- `crates/markdown/src/structure.rs` holds both types and the collector the parse loop calls once
  per event. `parse.rs` is at 384 of its 400 lines, so it gains only the field and the call.
- `Items` decides the box, so it records each line holding a task with no box. The parse merges
  those lines into `skipped`, in order.
- A range in `skipped` ends before its last `\n`, as a `CodeBlock`'s does, so no offset reaches
  past the text.
- The layout reads none of `structure`, so `changed` compares none of it.

**Its cost per keystroke.** It adds no scan. The parse loop already visits every event; the
structure adds one push per item, quote and skipped block. An item's content start reads the
blanks after its marker, at most five bytes. Its column counts from the line's start: every byte
is one column, and a tab stops at the next multiple of 4. The collector carries the column from the
previous opener on the line, an item or a quote, counting every byte up to the marker. So a line's
prefix is read once. The 500 KB note
of the parse bench (`crates/markdown/benches/parse.rs`) holds about 4,800 items, 1,200 quotes and
1,200 skipped blocks. That is about 7,200 pushes and 145 KB, beside a parse of 5.58 ms on 8b9db360.

**The column carry matches pulldown-cmark.** `LineStart` counts a tab's width from the byte after
the last tab it scanned, which lies on a stop, so its stops count from the line's start too
(`pulldown-cmark-0.13.6/src/scanners.rs:160-187`, the tab at 173-181).

- A quote between two items counts as bytes: in `- > - a`, the carry from the outer `-` passes the
  blank, the `>` and its blank, and the inner `-` sits at column 4.
- A tab after a `>` needs no special case. The `>` takes one of its columns, and `LineStart` keeps
  the rest as indent before the next marker (`spaces_remaining`, line 181). In `- >\t- a` the tab
  spans column 3 alone; in `>\t- a` it spans columns 1 to 3, and two remain after the `>`. In both,
  the inner `-` sits at column 4 and its content start is the `a`.

## D2. The conceal

**(B)** The task conceal's element is the task part; its marker is the box run
(`gutter_conceals`, `gutters.rs:255-278`). A marker must lie inside its element
(`parse/tests.rs:627-631`). With the checkbox alone as the element, `⌃A` on `- [ ] a` put the caret
at 0, revealed nothing, and typing landed before a hidden bullet.

- A caret anywhere in the task part, leading blanks included, reveals it.
- A caret at the text start does not touch the checkbox, so after a click the box stays painted.
- The replacement is `""`. The lead keeps its own conceals: a bullet in it shows `•` (`bullet`,
  `gutters.rs:181-192`), and a number shows as typed.
- The quote part and the task part reveal apart (markdown-blocks S1 review): in `> - [ ] a` a caret
  on `>` reveals the quote part only.
- A revealed task part reads raw, its lead included (D3).

The `debug_assert!` in `line_prefix` (`crates/app/src/editor/layout/prefix.rs:131-149`) counts a
gutter's own conceals, in marker order: its quotes, its lead's bullets, then the task conceal. The
G-property in `parse/tests/gutters.rs` admits the lead's one-byte bullets between them.

## D3. The gutter and its width

The gutter is a hanging indent: the line's text, wrapped rows included, starts at its right edge
(markdown-blocks D3). `line_prefix` (`prefix.rs:111-180`) builds it from the line's `Gutter` and
its own conceals. `GutterCarry::layout` (`crates/app/src/editor/layout/prefix/gutter.rs:155-213`)
measures it, once per distinct gutter text within a `shape_document`.

**Concealed.** While the task part is concealed, the gutter is the quote part, then the lead, then
the box slot.

- The quote part is measured on its own text, as a quote line's gutter is, and draws its bars.
- The lead is shaped through its own conceals with the body's runs (`show`,
  `crates/app/src/editor/layout/runs.rs:144-171`), so `- ` reads `• ` and `1. ` reads as typed.
  An empty lead shapes nothing. A blank lead is shaped for its width, so a nested task keeps its
  indent; it paints no glyph.
- The box sits at the box slot's left, in the bullet's place, right after the lead. No gap stands
  where the box run was.
- The width is the quote part's, plus the lead's, plus the box slot. The text starts there on row
  0, and wrapped rows hang at the same edge.

**The box slot.** It is the advance of `• ` shaped with the body's runs, as a bullet's `• ` reads
in a line's text (D2). So a task's text starts where a sibling bullet's text does, and its
children line up as a bullet's do.

- The box (`TASK_BOX_SIZE`) sits at the slot's left. The gap after it is what the slot leaves:
  about 6 px at the 18 px body.
- Where `TASK_BOX_SIZE` is wider than the advance of `• `, at a small body size, the slot is
  `TASK_BOX_SIZE`, with no gap added. The text then starts right after the box, right of a
  sibling bullet's text column. The rule: the slot is the larger of the two, plus nothing.
- `GutterCarry` measures the slot once per `shape_document`, as it measures a gutter text once.

The expected geometry, with the task part concealed:

- The box's left edge equals a sibling `•`'s left: in `- [ ] a\n- b`, both sit at `x` 0.
- The text column equals a sibling bullet's: `a` and `b` start at one `x`.
- A child of a task sits under the parent's text: in `- [ ] a\n  - c`, the child's `•` sits where
  it sits under a bullet parent, under `a` in the headless font. The child's indent is its two
  blanks, so a font whose two spaces differ from `• ` offsets a bullet's child and a task's child
  alike.

**(C) A toggle never changes a concealed width.** The quote part and the lead hold no checkbox
byte, and the box slot depends on the font only. A checked task's lead carries `DONE`, which
changes its colour, never its advance (D7). So every checkbox state shares one width, and a
toggle never rewraps a concealed line.

**Revealed.** A caret in the task part widens the gutter to the raw text of its quote part and
task part, which paints there raw, as a revealed `**` paints raw.

- The prefix range is `gutter.range` in both states, so it does not depend on the state.
  `line_prefix` still splits its conceals at `gutter.range.end` (`prefix.rs:124-130`), and the text
  stays right. It is the line past the gutter in both states, with the same conceals; the lead's
  bullets and the box run stay the gutter's own, so they hide nothing in it.
- `Shown` holds the raw shape of the gutter's text, split at `quote_end` when one part alone
  shows, by the split code that exists (`Shown::new`, `prefix/gutter.rs:66-91`, through
  `ShapedLine::split_at`). The task part alone paints at the quote part's width; both parts paint at
  `x` 0.
- The width is the raw width of the gutter's text: the quote part, the task part, and the one space
  or tab after the checkbox that the gutter's range holds (D1). Row 0's text moves right by the raw
  width less the concealed width.
- Wrapped rows hang at the same edge, the raw part's end. The line wraps at the column less its
  indent (`line_wrap`, `crates/app/src/editor/layout/shape.rs:347-357`), so a long task rewraps when
  its task part reveals, as a paragraph does when its `**` does.
- The caret and a click in the task part map through `Shown`, as they do today
  (`GutterLayout::offset_for_x`, `prefix/gutter.rs:41-45`). A click on a concealed quote part lands
  at the text start.
- The quote part keeps its behaviour. Revealed, its raw text paints in the gutter at `x` 0;
  concealed, only its bars do. Its raw width is its concealed width, so a reveal of the quote part
  alone moves no text.
- A toggle while revealed edits the gutter's text: the raw `[x]` and `[ ]` may differ by a pixel,
  so the text may move by that pixel, as after any typed byte.

**Where the raw width comes from.** `layout` already shapes the gutter's whole text raw while a part
is revealed, to paint it (`prefix/gutter.rs:167-168`). With the task part revealed, the line's
`Geometry` takes that shape's width. So the raw width costs no shape of its own, and it is the
line's, never cached.

**The records.**

- `Measured` (`prefix/gutter.rs:116-124`) is what one gutter text measures to, concealed: its
  `Geometry`, and `task: Option<TaskShape>`, the task's measured shape. `measure` never reads
  `Reveal`. So two lines with the same gutter text, one revealed and one concealed, share one
  concealed geometry, and the revealed one widens only its own.
- `TaskShape` holds `x`, the box's left: the quote part's width plus the lead's. It also holds the
  lead's shape. The concealed width is `x` plus the box slot.
- `GutterLayout` (`prefix/gutter.rs:15-23`) holds `task_box: Option<Box<TaskBox>>`: the line's
  `TaskShape` and its own `checked`, set only while its task part is concealed.
- `Geometry` (`prefix/gutter.rs:106-114`) holds the width, the bars' `x` and the quote part's
  width. A concealed task line's width ends at the box slot's right; a revealed one's at the raw
  text's end.
- `Shown` (`prefix/gutter.rs:57-104`) keeps its split, as above.
- The 256-byte assertion on `LineLayout` (`crates/app/src/editor/layout/line.rs:95-96`) stays the
  check. `TaskBox` and `Shown` stay boxed, and T6 adds no inline field.
- The doc comment on `LineLayout::prefix` (`line.rs:89`) says "no reveal re-wraps the line". The
  prefix stays out of the shaped text in both states, but a task part's reveal changes the indent,
  so the line rewraps. T6b updates the comment.

**(D) A lazy line carries the quote part only.** `GutterCarry::pass` (`prefix/gutter.rs:145-153`)
carries `Geometry::quote_part`, and `layout` gives it to a lazy line. Under `> - [ ] a\n  b`, line
`b` takes the quote part's width and paints no box. A box comes from the line's own `task`, never
from carried geometry. A revealed task line keeps its quote part's width in `Geometry`, so a reveal
changes nothing a lazy line reads.

## D4. Paint and hit test, per line

No box vector. **(E)** A vector built in `shape_document` beside `bars` would be a second O(lines)
pass per keystroke, for data each line already holds. `layout/mod.rs` also stands at the 400-line
limit of an unlisted file (`scripts/line-ceilings.sh:7`).

- **The square.** `LineLayout::task_box` (`line.rs:103-110`) gives the square a concealed task
  paints and a click hits: `TASK_BOX_SIZE` wide from `TaskShape`'s `x`, centred on row 0.
- **Paint.** The visible-line loop calls `paint_task` on each line
  (`crates/app/src/editor/layout/paint.rs:182-202`). It paints the lead at the quote part's width,
  then the box (`paint_task_box`, 205-257). Unchecked: `outline` in `md_marker`. Checked: `fill`
  in `accent`, then the check by `window.paint_svg` in `editor_background`. It paints with the
  gutter, after the selection (`crates/app/src/text_input/paint.rs:103-106`). gpui:
  `vendor/zed-reference/crates/gpui/src/window.rs`, `paint_quad` 4934, `paint_svg` 5235, `fill`
  8128, `outline` 8140.
- **Hit test.** `DocumentLayout::task_at(local) -> Option<(Diff, ByteOffset)>` (`paint.rs:263-280`)
  returns the task's flip and its text start (D5). It finds the line under `local.y` and requires
  `local` inside its square. **(F)** `row_for_y` clamps
  (`crates/app/src/editor/layout/mod.rs:251-265`), so the square, not the line lookup, rejects a
  click below the last line. The square fits inside row 0: 16 px in a row of 28 px at least.
- The task is the one in the layout's own parse, `Markdown::task_at` at the line's start (D1). A
  carried line holds no document offset.
- A revealed task part has no square: a click on it places the caret on its byte in the gutter
  (D3).

The check icon is Lucide `check` in `crates/app/assets/icons/`, `IconName::Check`
(`crates/app/src/ui/assets.rs:37`), under the icons' existing licence.

## D5. The click

`on_mouse_down` (`crates/app/src/editor/mouse.rs:41-81`) calls `task_under` (36-39), which calls
`task_at`, after focusing and before the `⌘`-click link branch (66-79).

- A plain click on a box toggles and returns before `is_selecting` (80), so it starts no drag
  selection.
- A click with `⌘` or `⇧` held skips the box: `⌘`-click keeps following links, `⇧`-click keeps
  extending the selection.
- The click that activates an inactive window skips the box too (`first_mouse`). It only activates
  the window, as Electron's default does.
- **(G)** The flip is an `inkra_text::Diff` based at the layout's version (`shaped_at`,
  `layout/mod.rs:81-82`), and `Buffer::apply_diff` refuses a diff whose base is not the buffer's
  version. After a reload, the last layout describes the old text until the next render
  (`crates/app/src/editor/reload.rs:24-26`). Three matching bytes would not prove the task is the
  same one. A mismatch edits nothing, and the click places the caret as any click does. Zed only
  debug-asserts here (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:1220`).
- The edit is `Task::flip` (`gutters.rs:44-54`): the middle byte, `x` for an unchecked one, a
  space for `x` or `X`. So `[\t]`, which pulldown-cmark reads as unchecked, flips to `[x]`, and a
  flip back writes `[ ]`: the tab is not restored. A vertical tab or a form feed goes the same way. `EditorState::apply_diff` runs one `Buffer::transact`: one undo step, merged with neither
  neighbour. The click then collapses the caret at the text start the layout holds, mapped through
  the flip. The flip keeps the byte count, so the text start does not move.
- A double click toggles twice: each press is one click (decision 3).

## D6. `⌘L`, the cycle

The action `editor::ToggleTask` is bound to `cmd-l` in the editor context
(`crates/app/src/keymap.rs:66`). The command palette lists it as "Toggle task" by its name; nothing
else registers it. `toggle_task` (`crates/app/src/editor/actions.rs:171-181`) passes the cached
parse and the selection to `cycle_tasks` (`crates/markdown/src/cycle.rs`). It applies the splices
as one `Diff` through `EditorState::apply_diff`, the click's path. An empty batch takes no lease,
since `as_mut` notifies and would schedule an autosave.

**The lines.** The selection's first line, then every later line that starts before the
selection's end. **(H)** So a selection that ends at column 0 of a later line drops that line, as
Zed does (`vendor/zed-reference/crates/editor/src/editor.rs`, around 5595 and 12843).

**Skipped lines.** A line whose start lies in a range of `structure.skipped`, ends included (D1).
**(I)** Without the front matter there, `⌘A ⌘L` rewrites the YAML. A line holding a task with no
box is there too: its checkbox cannot be cycled, and a box on its outer item would make two
checkboxes. An ATX heading's line is there too, so `# a`, `- # a` and `> # a` edit nothing: after
a checkbox, pulldown-cmark reads `# a` as the task's text, and the heading would turn to text. A
setext heading is not skipped. `a\n- ` is one, and `- ` typed under a line then `⌘L` must still
give `a\n- [ ] ` (the fallback). So `⌘L` on the `a` of `a\n===` still turns the heading to text;
level 0 accepts it. In a selection of more than one line, a blank line, of spaces and tabs only, is
skipped as well.

**The target.** The first cycled line decides it: checked when that line holds an unchecked task
(`Markdown::task_at`), else unchecked. Every cycled line takes the target, so a mixed selection
lands uniform:

| Line | Becomes |
|---|---|
| `- [ ] a`, `1. [ ] a` | `- [x] a`, `1. [x] a` |
| `- [x] a`, `- [X] a` | `- [ ] a` |
| `- a`, `* a`, `1. a`, `- 1. a` | `- [ ] a`, `* [ ] a`, `1. [ ] a`, `- 1. [ ] a` |
| `a`, `> a`, `  a` | `- [ ] a`, `> - [ ] a`, `  - [ ] a` |
| an empty line, a single-line selection | `- [ ] ` |

A written checkbox holds the target's byte, so under a ticked first line `c` becomes `- [x] c`.
Obsidian swaps a bullet for a checkbox the same way (forum 45383), and has turned plain text into a
task since 0.13.24 (forum 14634).

**The candidates.** Each cycled line has its splices to try, in order:

1. A task in the other state: its flip. A task in the target state: none.
2. A line where an item opens: the checkbox at the content start of the innermost item, the last
   of `structure.items` with its marker on the line. An empty item gains ` [ ] `, with a space of
   its own. This is the line's only candidate.
3. A line where no item opens: first the checkbox at the fallback's content start, when the
   fallback finds an item; then `- [ ] ` after the line's leading `[ \t>]*` (`line_start_prefix`,
   `parse.rs:221-231`).

`- [ ] ` is offered only where no item opens (Open question 6, decided). On a line where an item
opens it would land at the first item's marker, which would then read as the new task's text, so
that opener would be lost and the line would fail. So a line has at most two candidates, and only a
line where no item opens has two.

**The fallback.** A line where no item opens can still start with a marker that the parse reads
otherwise: a setext underline (`a\n-`), an empty item that cannot interrupt a paragraph (`a\n- `),
or a lazy number (`a\n2. b`). The fallback runs one pulldown-cmark pass, through the structure
collector, on that line's text after its leading `[ \t>]*`. It reads the text as `parse` reads a last
line. The first item it finds gives the content start, mapped back by the prefix's length (Open
question 5, decided).

- No marker grammar lives in `cycle.rs` or `structure.rs`: pulldown-cmark reads every marker.
- Columns count from the line's start, but the pass starts its tab stops at the text it is given.
  So the pass reads the text after the prefix preceded by `c mod 4` spaces, where `c` is the
  prefix's width in columns. Every tab then stops where it stops in the line, and the pad stays
  under the four columns that open an indented code block. The mapping back subtracts the pad as
  well. So `a\n -\t  ` gives `a\n -\t  [ ] `, as on `main`. In the line the tab spans two
  columns, so the blanks span four. Without the pad the tab would span three, the blanks five, and
  the checkbox would go past the tab alone.
- The candidates are built once, before the first round (`cycle.rs:46-52`). So the fallback costs
  one pass of O(line) per cycled line where no item opens, once per `⌘L`.
- It reads the first item, not the innermost. After a paragraph line, `2. 3. b` tries
  `2. [ ] 3. b`, which fails, since a lazy `2.` is text. The line then lands `- [ ] 2. 3. b`, as on
  `main`.
- `- - -` is a thematic break, alone as well as in the note: pulldown-cmark tries the break before
  the list marker (`pulldown-cmark-0.13.6/src/scanners.rs:315-322`). So the fallback finds no item,
  and the line takes `- [ ] - - -`. `main` gives `- - - [ ] `, three nested items. This is the one
  `⌘L` output T6a changes.

**The content start.** It follows `CommonMark` 5.2 for an item that starts with a paragraph.
Blanks after the marker count in columns, a tab stopping at the next multiple of 4 from the line's
start.

- Blanks of one to four columns: the content starts past all of them.
- Blanks of five columns or more: it starts past one blank.
- No blank, the line ending at the marker: it starts at the marker's end.

So `-   a\n\n      b` keeps `b` in the item, `-\t  ` becomes `-\t[ ]   `, and `- 1. a` becomes
`- 1. [ ] a`, a task item inside the outer one. An empty item counts its blanks the same way, so
`  -\t ` becomes `  -\t [ ] ` (Deviations, T6a).

**The batch, judged whole.** A round applies every line's current candidate to one copy of the
text, and parses the copy once.

- A line lands when its start, shifted by the splices before it, then holds a task in the target
  state, by `Markdown::task_at` on the copy's parse.
- **No markup turns to text.** A line also fails when an opener of the text no longer opens at its
  byte in the copy. Each opener of `structure` maps through the splices before it, an insertion at
  its byte moving it after the inserted text; the copy's own `structure` answers. The failure goes
  to the nearest cycled line at or above the lost opener. A splice reaches only its own line and
  those below it, as `---\n>\n---` turning into front matter shows. So `- [ ] ` before `> a`
  fails, and `- > a` edits nothing.
- A line that lands keeps its splice. Any other line moves to its next candidate, or drops out
  after its last.
- The first round in which every splice lands returns its batch. After three rounds the batch is
  empty.
- **A round of flips only parses nothing.** A flip rewrites one byte that pulldown-cmark reads
  inside the task marker in either state (`firstpass.rs:145-161`). `scan_task_list_marker` reads
  `[`, then `x`, `X` or a blank other than a line ending, then `]` and a whitespace byte
  (`pulldown-cmark-0.13.6/src/scanners.rs:378-413`, the blank at 387). A flip writes `x` or a
  space, both read there, and leaves the brackets and the byte after them. No block, opener or box moves,
  and the line holds a task in the target state. So that round lands as it stands, and `⌘L` on
  task lines alone runs no parse.

The argument: a batch is returned only from a round whose own parse shows each of its lines a task
in the target state, with every opener of the text still opening at its byte. Every splice lies
inside its line, so the result holds whatever a splice does to its neighbour's laziness; the empty
batch holds trivially. A failing round moves at least one line on through at most two candidates,
so the rounds end. The cap of three covers the second candidates (round 2), then the batch without
the lines no candidate makes a task (round 3). A longer chain edits nothing rather than guess.

So D1 and D6 stay one predicate, and the batch is judged whole: `a\n    b` selected cycles both
lines, `b` nesting under the new item, though `b` alone cannot be a task.

**Its cost.** A `⌘L` costs the cached parse, one fallback pass per cycled line where no item opens,
then at most three parses of a spliced copy, each one pulldown-cmark pass. A batch of flips only
costs none. Before T6a it also ran one pass for its
skipped lines and one more per round for the copy's openers: up to seven passes, three of them for
a single flip.

## D7. The struck text

`Styles` has `DONE` (`crates/markdown/src/styles.rs:34`, `1 << 12`). It is set on a checked task
item's own text: every paragraph directly inside the item, continuation and lazy lines included.

- **Set where the marker is decided.** The pending slot (D1) resolves after `Start(Item)`, or after
  one `Start(Paragraph)` in a loose item. When it resolves a checked task that gets a box, `DONE`
  goes to the item's `Open` and to every element opened inside it so far (`parse.rs:84-89`).
  Later paragraphs inherit it through `Open::enter` (`parse.rs:274-293`).
- **Not inherited by a block.** `Open::enter` drops `DONE` when a list, a quote or a code block
  opens inside the item, so nested items are not struck. Any other block of the item, a heading, a
  table or an HTML block, inherits it and is struck: it is the item's own text as much as a
  paragraph is.
- **Markers are not struck.** The contract: `DONE` is on every non-`MARKER` span of the item's own
  text, and a `MARKER` span may carry it. `markdown_style` (`crates/app/src/theme/markdown.rs:22`)
  strikes `DONE` only when `MARKER` is absent, and colours it `md_marker`. Gaps are styled
  `open.indent | MARKER`; the resolution adds `DONE` to `indent` too, so a continuation line's
  indent carries it. A loose item's own marker does not: its gap is styled before the slot
  resolves on the paragraph.
- Continuation and lazy lines restyle through the span diff (`changed.rs:113-115`), so a toggle
  restyles every row of the item.
- **A wrapped row strikes its trailing space.** The space that ends a wrapped row is the item's
  text, so the line runs past the last word.

One predicate decides both the box and `DONE`: the pending slot resolving a task that gets a box
(D10). A task that gets none (`- > - [ ] a`) gets neither box nor strike, so `⌘L` can never make a
struck line without a box.

## D8. Theme

No new colour token: `md_marker`, `accent` and `editor_background` exist
(`crates/app/src/theme/colors.rs`). The token count test (`crates/app/src/theme/file/tests.rs:9`)
stays at 41. `theme.rs` holds `TASK_BOX_SIZE` (16 px), `TASK_BOX_RADIUS` (4 px), `TASK_BOX_BORDER`
(1.5 px) and `TASK_CHECK_SIZE` (12 px), beside `QUOTE_BAR_WIDTH` (`crates/app/src/theme.rs:292-304`).
They come from mockup C: a 15 px box with a 4 px radius and a 1.5 px border, at 15 px text. At the
18 px body the box takes 16 px and the check 12 px. No gap constant is added: the box slot is the
advance of `• ` (D3), so the gap after the box is what the slot leaves, about 6 px at the 18 px
body, where mockup C had 10 px. A distinct box colour waits for a theme that asks for one.

## D9. Incremental draw

- A toggle is a one-byte splice in the task's own line (`changed.rs:17`). With `Gutter::same`
  comparing the task (A), no carried line keeps a stale box or a stale strike.
- A reveal flip dirties its line through `Carry::flipped`
  (`crates/app/src/editor/layout/dirty.rs:69-79`). Since T6b a task part's reveal changes the
  line's indent, so the line wraps at a new width and reshapes (`line_wrap`, D3). Its text and
  runs stay the same: the prefix range does not depend on the state. The rebuild the flip already
  causes gives the new indent and rows.
- `is_shaped_from` (`layout/mod.rs:113-123`) compares the conceals the selection touches, so a
  reveal reshapes and a caret move inside a revealed task part does not.
- The layout oracle of the incremental-draw spec (D10) holds tasks, toggles and `⌘L` batches in its
  alphabet (`crates/app/src/editor/layout/tests.rs`). Its toggle clicks a box of the carried layout
  through `DocumentLayout::task_at` (T6a).

## D10. The lead and the box rule

**The box rule.** A task item gets a box when its line holds all its leading quotes and, between
the quote part's end and the item's marker, no `>` (`task_gutter`, `gutters.rs:130-165`).
CommonMark opens a list item mid-line only after container markers, so that gap holds only blanks
and the markers of items that open on the line.

- `- [ ] a`, `  - [ ] a`, `> - [ ] a`, `1. [ ] a`, `- - [ ] a` and `1. - [ ] a` get a box.
- `- > - [ ] a` and `- a\n  > - [ ] b` do not: a quote inside a list item has no gutter
  (markdown-blocks, As built item 1). Its item keeps its `•`, it gets no strike (D7), and `⌘L`
  skips its line (D6).
- This rule replaces markdown-blocks S3 correction 13.

**Only the task's own bullet gives way to the box**, as Obsidian draws it. An ordered task's number
is content, the order of the steps, so it stays before the box. The outer items' bullets in the
lead keep their `•`. Their conceals start inside the gutter, so they are the gutter's own (D2), and
the lead shows them (D3).

## Tests

`inkra-markdown`, by example:

- `- [ ] a`, `- [x] a`, `- [X] a`, `* [ ] a`, `1. [ ] a`, `> - [ ] a`, `  - [ ] a`, `-    [ ] a`,
  loose `- [ ] a\n\n- [x] b`: a gutter with a task.
- `- [ ]a` and `-\n  [ ] a`: no task. `- [ ]\n` and `- [ ]` at the end of the text: a task with no
  text. `-\n`: keeps its `•`.
- `- - [ ] a`: a gutter task with the outer `•` in its lead; `- > - [ ] a`: no gutter task;
  `- a\n  - [ ] b`: the inner line has a gutter task.
- The conceal covers the task part and hides the box run. A caret at 0 in `- [ ] a` touches it, a
  caret at the text start does not, and a caret in the lead reveals the task part.
- The struck text (`DONE` without `MARKER`) of `- [x] a` is `a` only. It covers a continuation
  line's text but not its indent, every paragraph of a loose item, and no nested item, quote or
  code block inside the item.
- `Markdown::task_at` finds a task by its line's start, and nothing at any other byte (T6a).
- `Structure` (T6a): `- a` holds an item at `-` with content 2; `- 1. a` two items, the inner one's
  content 5; `-` an item with content 1; `  -\t ` content 5; `> - a` a quote at 0 and an item at 2;
  `- > a` an item at 0 and a quote at 2; `a\n-` no item. `skipped` holds a code block, the front
  matter, a table, an HTML block, an ATX heading (`# a`, and `- # a` from the line's start) and an
  unboxed task's line, each from its first line's start and without its last `\n`. A setext heading
  is not in it.
- The cycle: every row of D6's table; a selection ending at column 0; a code block, front matter, a
  table row, an HTML block and `# a`, `- # a`, `> # a` skipped; a mixed selection lands on the first line's next state; a
  splice that changes the next line's laziness; a lost quote fails its own line.
- The fallback (T6a): `a\n- ` and `a\n-`, caret on the second line, give `a\n- [ ] `, as on `main`;
  `a\n -\t  ` gives `a\n -\t  [ ] `, as on `main`, which only the pad gives; `- - -` gives
  `- [ ] - - -`, where `main` gives `- - - [ ] `. No current test holds any of these lines
  (`crates/markdown/src/cycle/tests.rs`, `batch_tests.rs`).
- The cycle's passes (T6a), counted under `cfg(test)`: a batch of flips only runs none; `⌘A ⌘L` on
  `a\nb` runs one parse of the copy and one fallback pass per line; on `- a\n- b` it runs one parse
  and no fallback pass.

`inkra-markdown`, proptest:

- The conceal properties and the G-properties, for a task gutter: `depth` may be 0 with a non-empty
  range when `task` is set (`parse/tests/gutters.rs:228-236`).
- The last line: `parse(text)` equals `parse(text + "\n")`, and no offset reaches past the text,
  `structure`'s included (T6a).
- The cycle: its splices, applied and reparsed, give the target state on every changed line and
  leave every other line byte-identical. Every opener still opens at its mapped byte, and the mapped
  selection stays inside the text and touches the same lines. The test oracles run their own
  pulldown-cmark passes, never `structure`, so they stay independent of it.

`inkra-editor`: `apply_diff` is one undo step, merges with no neighbour, maps the selection, and
edits nothing from a stale base.

`app`, headless `#[gpui::test]` against `MemFs`, a body glyph 10.8 px:

- Concealed (T6b), each `x` compared with another line's measured `x`, never a computed width:
  - `- [ ] a\n- b`: the box's left edge is the `•`'s left, and `a` starts where `b` does.
  - `- [ ] a\n  - [ ] c\n  - d`: the nested box's left and the nested `•` start where `a` does.
  - `1. [ ] a` paints `1. ` at 0 and its box at 3 glyphs. `- - [ ] a` paints `• `, then the box at 2
    glyphs. `- a\n  - [ ] b` keeps its blank lead's 2 glyphs before the box. `> - [ ] a` paints its
    box at the quote part's width.
- The box slot (T6b): at a body size where `• ` is narrower than `TASK_BOX_SIZE` (below 13.3 px
  headless), the text starts at the box's right edge.
- Toggling keeps a concealed line's text start (C).
- Revealed (T6b): a caret in the task part paints no box, and the gutter shows the raw task part,
  alone at the quote part's width or with the quote part at 0. Row 0's text moves right by the raw
  width less the concealed width, and its wrapped rows start at the same edge. The caret and a
  selection inside the task part sit at the shown part's `x`.
- In `> - [ ] a` a caret on `>` reveals the quote part only; a lazy line under it takes the quote
  part's width and paints no box.
- A click on a box toggles `[ ]`↔`[x]` and moves the caret to the text start; so does a click on
  the box of `1. [ ] a` (T6a). `[X]` toggles to `[ ]`, and `⌘Z` restores the text. A click on a
  carried line's box, after an edit on the line above, toggles that task (T6a).
- A click on a revealed checkbox places the caret and edits nothing. A press on a box then a drag
  selects nothing. `⌘`- and `⇧`-click do not toggle, nor does a window-activating click. A click
  below the last line does not toggle its box. A click on a layout older than the buffer edits
  nothing. A toggle with the view scrolled away from the old caret does not scroll.
- A click on the lead lands at the text start and toggles nothing.
- `simulate_keystrokes("cmd-l")`: on a plain line, on a task, on a selection of three lines, and on
  `- > - [ ] a`, which it leaves. `cmd-enter` on a task line holding a link still follows it.
- The layout oracle, its toggle through `DocumentLayout::task_at` (T6a). Only a concealed box can
  be clicked; the `⌘L` step still cycles revealed tasks.

Scene: `markdown-blocks` holds open, done, nested, ordered and quoted tasks, a task after a bullet
and after a number, a wrapped done task, and a revealed ordered task
(`crates/app/src/shot/scenes/markdown_blocks.rs:7-34`). T6b adds a plain bullet beside the tasks,
and a task and a bullet nested under an open task. Its `shows` is at
`crates/app/src/shot/scenes.rs:439-448`. Render it and look:
`cargo run -p inkra-app --features shot --example shot markdown-blocks`.

Mutants after each slice: `markdown` on the diff, and `app` with the command from `CLAUDE.md`.

## Slices

Each slice is vertical and ends green on `./scripts/check.sh`, with its scene rendered and looked
at. T1a through T5 are merged (History).

1. **T1a, the task gutter** (#73).
2. **T1b, the box paints** (#73).
3. **T2, the click** (#74).
4. **T3, the strike** (#75).
5. **T4a, `⌘L` on one line** (#76).
6. **T4b, `⌘L` on a selection** (#77).
7. **T5, the markers before the box** (#78).
8. **T6a, one parse** (~0.5 d). Open questions 5 and 6 are decided. It changes one `⌘L` output,
   `- - -` (D6), and nothing else visible.
   - `inkra-markdown`: `Structure` and `ListItem` in `structure.rs`, `Markdown::structure` and
     `Markdown::task_at` (D1). `cycle.rs` reads them and loses `Scan`, `Openers`, `Marker`,
     `marker`, `innermost`, `column_after`, `terminated` and its own `task_at` (D6). It gains the
     fallback, which runs the structure collector, and offers `- [ ] ` only where no item opens.
     A round of flips only parses nothing. `task_gutter`'s newline guard becomes a `debug_assert!`
     (D1).
   - `app`: `DocumentLayout::task_at` calls `Markdown::task_at`. `Measured`'s `checkbox` and `lead`
     become `task: Option<TaskShape>`, and `TaskBox` holds the line's `TaskShape` and `checked`
     (D3). Here `TaskShape` holds the checkbox's `x` span in place of `x`, measured as before.
   - Tests: the T6a items of Tests. Every other cycle test passes unchanged.
   - Measure before and after:
     `cargo bench -p inkra-markdown --bench parse -- --quick 'markdown_parse/500000$|cycle_tasks_all'`.
     On 8b9db360, `markdown_parse/500000` takes 5.58 ms and `cycle_tasks_all/500000` 15.0 ms. The
     first should stay within noise; the second should fall.
9. **T6b, natural widths** (~0.5 d), after T6a.
   - D3's concealed gutter with the box slot, its widened revealed gutter, and D4's square at
     `TaskShape`'s `x`. `measured_text` and its `X` go: the revealed width is the raw text's own.
     `Shown` and its split stay. The doc comment at `line.rs:89` is updated (D3).
   - Tests: the T6b items of Tests. In `crates/app/src/editor/tests/task_gutter.rs`:
     - `revealing_a_task_part_moves_no_text` is replaced: a reveal now moves row 0's text and the
       wrapped rows by the raw width less the concealed width.
     - `the_quote_part_and_the_task_part_reveal_apart` stays valid with new expected values. The
       concealed text start of `> - [ ] a` falls from 8 glyphs to 4, and with the task part
       revealed alone the text sits at the raw 8 glyphs, so `task_xs` no longer equals `xs`. The
       shown texts and their `x` stay.
     - `a_shown_part_maps_its_offsets_where_they_stand` stays valid: only its concealed text start
       falls from 8 glyphs to 4.
     - `a_click_lands_through_the_revealed_part_only` and
       `a_click_on_a_shown_quote_part_stays_out_of_the_task_part` pass unchanged. A click on the
       concealed quote part still lands at the text start.
     - `toggling_a_task_keeps_its_gutter_width` moves its expected `x` from 6 glyphs to 2.
   - They also move the expected `x` in `task_boxes.rs` and `task_lead.rs`.
   - Scene: both `markdown-blocks` scenes rendered and looked at. The note gains `- A plain
     bullet` beside the tasks, and under `- [ ] An open task` a nested task and a nested bullet.
     `shows` names them: each task's text in the bullet's text column, its box where the `•` is,
     the nested box and `•` under the open task's text. It also names the box after the lead, and
     the revealed ordered task read raw in a widened gutter, its text right of the box column.

## History

Newest last. Each entry says what changed and why.

**Slices.**

- **2026-11-16, approved.** From a brainstorm with the owner. An audit and an advisor review of
  markdown-blocks "S3 as planned" found defects A–I, which D1–D9 fix.
- **T1a, the task gutter** (#73). The marker hid into a hanging gutter of fixed width and revealed
  in place; the quote part revealed apart.
- **T1b, the box paints** (#73). The paint half of D4, D8 and the check icon. `prefix.rs` was split
  into `prefix/{gutter,margin}.rs` before it reached 400 lines.
- **T2, the click** (#74). `task_at`, D5, and the oracle with toggles. The owner added the
  `first_mouse` rule after the T2 review.
- **T3, the strike** (#75). D7. The T3 review struck a heading, a table or an HTML block inside a
  checked item, as its own text, and kept a loose item's own marker unstruck. The owner accepted
  the struck trailing space of a wrapped row.
- **T4a, `⌘L` on one line** (#76). The splice function, the binding, and a reparse per line. The
  T4a review skipped HTML blocks: `⌘L` on `<div>` made a task of it. The cross-slice review dropped
  `⌘L`'s own batch edit for the click's `apply_diff` path.
- **T4b, `⌘L` on a selection** (#77). The batch judged by one parse per round, capped at three
  rounds, so `⌘A ⌘L` stays bounded.
- **T5, the markers before the box** (#78). The owner: "la spec est mauvaise". It added the box
  rule, `Task::start` and `run`, the box-run conceal, the painted lead, one `TaskBox`, `⌘L` after
  the innermost marker, and the skip of an unboxed task. Its reviews added "no markup turns to
  text", and counted a marker's blanks in columns.
- **T6a, one parse** (2026-11-18). `⌘L` ran a pulldown-cmark pass of its own for skipped lines and
  openers, one more per round, and restated `CommonMark`'s marker grammar. The parse already sees
  all of it, so `Markdown` exposes it (D1), and a round of flips parses nothing.
- **T6b, natural widths** (2026-11-18). The owner dropped "no reveal moves text" for the task part:
  it never held for bold. Keeping it cost the `X` measure, a gap the width of the raw marker before
  the text, and a box where `[` stood rather than where the bullet did. A concealed task part takes
  the lead and the box slot; a revealed one widens the gutter to its raw text.
- **The owner's answers to the T6 review** (2026-11-18).
  - The revealed state is the widened gutter. The revealed task part stays in the gutter as raw
    text, a `Shown` painted by the split code that exists. The prefix range then does not depend on
    the state, and no `line_prefix` change is needed.
  - The box slot is the advance of `• `, so a task's text starts where a sibling bullet's does, and
    its children line up. `TASK_BOX_GAP` is not added. Where the box is wider than `• `, the slot
    is the box.
  - Open question 5 is decided: the fallback (D6). No marker grammar lives in `cycle.rs` or
    `structure.rs`. Without it, `a\n-` gave `a\n- [ ] -` and `a\n2. b` both selected gave
    `- [ ] a\n- [ ] 2. b`; the recommended marker reader would have restated `CommonMark`'s grammar
    in `structure.rs`. With it, `- - -` gives `- [ ] - - -`, since pulldown-cmark reads it as a
    thematic break.
  - Open question 6 is decided as recommended: `- [ ] ` is offered only where no item opens.
  - Open question 4 is decided: an ATX heading's line is skipped, so `# a` edits nothing. A
    setext heading is not, since `a\n- ` is one.
- **The T6 review's minor findings** (2026-11-18). `TaskShape`'s box `x` is one field. The column
  carry is checked against `LineStart`. pulldown-cmark reads a tab, a vertical tab or a form feed
  as an unchecked middle byte, so the checkbox term and decision 5 say so, and D5 records that a
  flip does not restore a tab. Stale lines in `paint.rs` and `mouse.rs` are updated.

**Reversed decisions.**

- **A shared `⌘↩`** (2026-11-16) became `⌘L`. `link_at` accepts a caret just past a link
  (`crates/markdown/src/links.rs:73-81`), so `⌘↩` at the end of `- [ ] Call [[Bob]]` would follow
  the link instead of toggling.
- **The width measured with a space** (T1a) became the width measured with `X`, by the owner. With
  a space, a revealed `[x]` touched the text and `[X]` overlapped it. T6b replaced both by the box
  slot.
- **No reveal moves text** (D3, T1a) was dropped for the task part by T6b, as above.
- **The box centred on the checkbox's `x` span** (D4, T1b) became the box in the bullet's place,
  after the lead (T6b).
- **A concealed task part paints only its box** (T1b) became the lead, then the box (T5).
  `1. [ ] a` had lost its number and showed a gap.
- **Markdown-blocks S3 correction 13** became the box rule (T5). It gave no box to `- - [ ] a` or
  `1. - [ ] a`, and `⌘L` on them added a second checkbox.
- **`- [ ]` at the note's end is no task** (Open question 2) was reversed by the owner after T4a:
  `⌘L` on that line added a second checkbox. `parse` now reads the last line as if a `\n` ended it.
- **A reparse per line** (T4a) became the batch of T4b. It cost up to two parses per line and judged
  each line alone.
- **The version checked twice** (T2) became one check in `apply_diff`, by the cross-slice review.
  `task_at` took the buffer's version and the editor re-checked the bytes.
- **"No markup turns to text" as a count per line** (T5 review) became a check per opener byte. A
  line could lose one `>` and gain another; the fuzzer found two such inputs, now pinned.
- **`⌘L`'s own pulldown-cmark passes** (D6, T4a and T5) became the parse's `structure` (T6a).
- **A blank lead shapes nothing** (T5) became a blank lead shaped for its width (T6b). With natural
  widths, a nested task would lose its indent.
- **The revealed task part in the line's text** (T6b as first approved, 2026-11-18) became the
  widened gutter, by the owner. That draft ended a revealed gutter at `quote_end`, so the prefix
  range depended on the state, rows rewrapped to the quote part's edge, and `Shown` lost its split.
- **`TASK_BOX_GAP`** (10 px, T6b as first approved) was dropped by the owner. The box slot is the
  advance of `• `, so the gap after the box is what that advance leaves.
- **A marker read by `cycle.rs`** (T4a to T5) became the fallback (T6a): pulldown-cmark reads a
  line where no item opens, and `cycle.rs` keeps no marker grammar.

**Superseded deviations.**

- **T1a, `Geometry` staged across slices.** Done: the quote part's width came in T1a and the
  checkbox's span in T1b. T6b replaced the span by the box's `x`.
- **T5, the skip pass reads a final `\n`.** Superseded by T6a: `parse` reads the text so, and the
  skipped lines come from it.
- **T5, `show` moved from `shape.rs` to `runs.rs`.** Done; D3 cites it there.
- **T5, no markup turns to text.** Now part of D6.
- **T2, a carried line holds no document offset of its task.** Now part of D4.

## Deviations

- **From Zed.** Zed renders a task as a `Checkbox` element in its separate preview pane
  (`vendor/zed-reference/crates/markdown/src/markdown.rs:2999-3032`). It writes all three bytes on
  toggle, debug-asserting lower-case `[x]`
  (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:1201-1229`). Inkra
  paints a box for the concealed task part in the live editor and writes the middle byte, so `[X]`
  toggles too. It checks the version before writing (G), and moves the caret. Zed's editor has no
  toggle action; `⌘L` follows Obsidian.
- **From markdown-blocks S3.** The conceal covers the task part (B); a concealed width does not
  depend on the state (C); boxes are found per line, not from a vector (E). There is no `marks.rs`
  module and no new colour token; the strike and the key are added.
- **From this spec, in T2.** `task_at` returns the task's flip as a `Diff` at the layout's version,
  and its text start; `apply_diff` makes the check (G). The hit test (F) is containment in the
  painted square, which fits inside row 0, so it has no separate row check.
- **From this spec, in T4b.** Every failing line moves to its next candidate in the same round, so a
  line can run out before the line above it settles. With `2. b` and `    c` selected after `x`,
  `c` is lazy under the failing `2. [ ] b` and drops out, though `- [ ] 2. b` would let it nest.
  Moving only the first failing line would land it but cost a parse per line on `⌘A`; the cap of
  three parses wins. The fallback (D6) keeps the example: `2. b` tries `2. [ ] b` first, as on
  `main`.
- **From this spec, in T5.**
  - `TaskBox` holds no checkbox offset. `Task` gained `start` and `run`, so a line-relative copy
    could not rebuild the whole `Task` to flip it; `task_at` reads the layout's own parse (D4).
  - The lead shapes through all its conceals, with no `is_active` filter. The cache is keyed by
    gutter text, so the first line with that text may have its task part revealed, and a filter
    would cache a raw lead. All are active whenever the task part is concealed, so the set is the
    same.
- **From the T6a brief.** An empty item's content start counts its blanks as an item with a
  paragraph does. `CommonMark` 5.2 and pulldown-cmark put it one column past the marker
  (`pulldown-cmark-0.13.6/src/scanners.rs:353-373`). So `⌘L` on `  -\t ` still writes `  -\t [ ] `,
  as before T6a.

## Deferred

| Item | Trigger |
|---|---|
| Progress count on a parent task, tree guides, folding children | The owner asks (level 1 of the brainstorm). |
| Extra statuses `[/]`, `[-]`, `[>]` | The owner asks (level 2); needs our own marker scan, pulldown-cmark reads only a blank, `x` or `X`. |
| Dates, priorities, a vault-wide task view | The owner asks (level 3). |
| `↩` at the end of a task starts `- [ ] ` | List continuation is built. |
| A pointer cursor over a box | A link gets one first. |
| A distinct box colour token | A theme asks for one. |

## Open questions

1. **After `⌘Z` the caret sits inside the checkbox**, so the task part shows raw in the widened
   gutter (T6b) until the caret leaves. Recommended: accept; the reveal rule says so.
2. **`- [ ]` with no newline at the end of the note is no task.** Decided: reversed by the owner
   after T4a (History).
3. **A click on the box of a task with no text (`- [ ]\n`) reveals its marker.** Its text start is
   the conceal's end, and a caret at a conceal's end touches it
   (`crates/markdown/src/reveal.rs:15-18`). Since T6b the raw `- [x]` then paints in the widened
   gutter. A typed space conceals it again. A letter typed right after the checkbox makes the
   line text, as `- [x]a` is no task. Recommended: accept for level 0; list continuation
   (Deferred) writes the space.
4. **`⌘L` on `# a` gave `- [ ] # a`**, and the heading turned to the task's text. Decided by the
   owner on 2026-11-18: an ATX heading's line is skipped (D6), so `# a` edits nothing. A setext
   heading is not, so the `- ` typed under a line still cycles.
5. **A marker the parse reads as no item** (T6a). Decided by the owner on 2026-11-18: the
   fallback (D6) reads such a line through pulldown-cmark, and `cycle.rs` keeps no marker grammar.
   Without it, `a\n-` gave `a\n- [ ] -`, and `a\n2. b` both selected gave `- [ ] a\n- [ ] 2. b`.
   The one output that changes is `- - -` (History).
6. **Candidate 3 never lands on a line where an item opens** (T6a). Decided by the owner on
   2026-11-18, as recommended: `- [ ] ` is offered only where no item opens (D6). No current test
   changes.
