# Task boxes

Status: **approved** 2026-11-16, from a brainstorm with the owner. The owner approved T6a and T6b
on 2026-11-18, then reshaped them the same day through the answers to the T6 review and to Open
questions 5 and 6. In the real font, T6b as built glued a task's text to its box. The owner then
moved bullets into the box's slot (decision 7). T6c is designed, and the owner decided its Open
questions 7 and 8 on 2026-11-18/19. Open question 9 waits on the rendered scene. Open question 10
was decided before T6c1 started, and T6c2 took its fallback. T6c1 and T6c2 are built, and the
cross-slice review's fixes are in (History). This spec replaces S3 of
`2026-09-30-markdown-blocks-design.md` ("S3 as planned", 2026-11-11), whose parser facts and
pending-slot rule it cites rather than copies. An audit and an advisor review (Opus) of that plan
found the defects this spec fixes; they are cited by letter (A–I). D1–D11 state the design as it
stands after T6c; History records how it got there.

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
6. **A list line takes its natural width** (2026-11-18). Concealed, its gutter holds the quote
   part, then the slots of its item part (D3, D11). Revealed, the raw item part paints
   right-aligned against the text start. The gutter widens only where that raw text is wider than
   the slots. So revealing a task can move its text, as a revealed `**` does, and revealing a
   bullet does not.
7. **Bullets share the box's slot** (2026-11-18, after T6b's glued text). Every list item in the
   gutter shows its marker in a slot: a bullet as `•`, a task as its box, a number as typed. Each
   item's text starts past its slots, and its wrapped rows and later lines start there too. A slot
   is `TASK_BOX_SIZE` plus `LIST_SLOT_GAP`, so a box always has a gap after it. The numbers of one
   list share one slot width and are right-aligned in it, so their dots line up (2026-11-19, Open
   question 8).

## Terms

- **checkbox**: the three bytes `[`, a middle byte, `]`. pulldown-cmark reads `x` or `X` as
  checked. It reads a space, a tab, a vertical tab or a form feed as unchecked
  (`pulldown-cmark-0.13.6/src/scanners.rs:386-394`).
- **gutter item**: a list item whose ancestors are leading quotes, gutter items and their lists
  (D11). In `- > - a` the outer item is a gutter item and the inner one is not.
- **marker line**: the line that holds an item's marker.
- **item part**: the gutter's bytes past the quote part, on a line that a gutter item holds. On a
  marker line it runs through the last mark and one space or tab. On any other line it is its
  leading blanks: below the hang column, or before the leaf text start on a code or HTML block's
  line (D11). It never covers a byte of a code or HTML block's text. It is hidden in both states.
- **leaf text start**: the column where pulldown-cmark starts a code or HTML block line's text:
  the column of its first text event, less the leftover columns that event carries (D11). A fence
  line has no text event.
- **slot**: an area `TASK_BOX_SIZE + LIST_SLOT_GAP` wide, the standard slot. A number's slot takes
  its list's number width instead, never narrower than the standard slot (D3). An item part is a
  row of slots.
- **width key**: what a list's number width depends on: the digit count of the list's longest
  number, and its `.` or `)` (D1, D3). A bullet list has none.
- **mark**: what a slot shows while concealed: a bullet's `•`, a number as typed, or the box. A slot
  of an item whose marker stands on an earlier line is blank.
- **box**: the square painted in a task's slot.
- **hang column**: the column after the marker line's item part, counted from the line's start by
  `column_after` (D11).
- **text start**: the first byte past the line's item part. For a task, that is the byte after the
  checkbox and its one space or tab. For an item with no text, it is the line's end.
- **opener**: a list item or a block quote that opens in the text, at its byte: an item's marker, a
  quote's start.
- **content start**: the byte where an item's text starts, or would start, on its marker line (D6).
- **fallback**: the pulldown-cmark pass over one line's text, after its prefix, that gives a
  content start on a line where no item opens (D6).

T6c retires these terms: task part, box run, lead, box slot.

## D1. The data

`Gutter` (`crates/markdown/src/gutters.rs:12-21`) trades `task` for `item: Option<ItemPart>`.
`Markdown` gains `width_keys: Vec<WidthKey>`, one vector for the note that every
`ItemPart::keys` indexes (Open question 10).

```rust
pub struct ItemPart {
    /// Where it starts: past the quote part's last `>` and its blank, or the line's start.
    pub start: ByteOffset,
    /// Its blank slots: one per column of each gutter item holding the line whose marker stands
    /// on an earlier line (D11).
    pub indent: usize,
    /// Where `Markdown::width_keys` holds the width key of each number slot on the line, blank or
    /// marked, in slot order; empty under no ordered list (D1, D3).
    pub keys: Range<usize>,
    /// The box, when the innermost gutter item opening on the line is a task.
    pub task: Option<Task>,
}

/// What a list's number width depends on: its longest number (D3).
pub struct WidthKey {
    pub digits: u8,
    /// `.` or `)`; a new delimiter starts a new list.
    pub delimiter: u8,
}

pub struct Task {
    /// The three checkbox bytes.
    pub checkbox: Range<ByteOffset>,
    pub checked: bool,
}

/// What one slot of a marker line shows besides the box.
pub enum ItemMarker {
    /// A `-`, `*` or `+`, painted as `•`.
    Bullet,
    /// A number's digits and its `.` or `)`, painted as typed.
    Number(Range<ByteOffset>),
}
```

- **Which lines.** A line gets a `Gutter` when it is quoted. It also gets one when a gutter item
  holds it and the line is not blank, or when it is a blank line inside a code block that a gutter
  item holds (D11).
- **The range.** `range` runs through the quote part, then the item part. `Gutter::quote_end`
  (`gutters.rs:64-66`) returns `item.start`, or `range.end` when the line has no item part.
- **The marks.** `ItemPart::marks(&self, part: &str) -> impl Iterator<Item = ItemMarker>`
  (`gutters.rs:73-101`) yields a marker line's marks in slot order, before the box. `part` is the
  item part's text, from `start` to the gutter's end. The marks are the item part's non-blank runs
  before the checkbox: a run that starts with a digit is a number, any other a bullet. A task's own
  bullet, the last run before its checkbox, is dropped, because the box takes its slot (D10). An
  ordered task's number stays. On a line that is not a marker line, the item part holds only
  blanks, so it yields none. So `app` holds no bullet grammar.
- **Slot order.** A line's slots are its `indent` blank slots, then one slot per mark in order, then
  the box. The number slots take the keys that `keys` indexes in that order: the blank ones
  first, then one per `ItemMarker::Number`.
- **`Task` loses `start` and `run`.** The item part's start and its marks now hold that data. The
  item conceal covers the box's bytes (D2). `Task::flip` is unchanged.
- **The marker.** The pending-slot rule decides an item's marker (markdown-blocks, S3 correction 7;
  Zed `vendor/zed-reference/crates/markdown/src/markdown.rs:3820-3836`). `Items` resolves it at the
  item's first event that is not one `Start(Paragraph)`. The parser facts table of markdown-blocks
  holds for pulldown-cmark 0.13.6 (`Cargo.lock:4068-4069`).
- **The records.**
  - `Items` lives in `crates/markdown/src/items.rs` (`items.rs:39-57`); T6c1 moved it out of
    `gutters.rs`. Each ordered list's width key is in `Items::list_keys`.
  - At each gutter item's `Start(Item)` it records the item's range and its index in
    `structure.items`, which already holds the marker (D1, the structure). So no record repeats the
    marker. When the marker resolves, it records the box.
  - It keeps one record per open list. At `End(List)` that record holds the list's width key: the
    most digits of any of its markers, which `Items` counted at each `Start(Item)`, and its
    delimiter. So the parse learns a list's width key at `End(List)`, in its one pass.
  - It records each text event of a code or HTML block that a gutter item holds as a `LeafText`
    (`items.rs:62-66`, pushed in `Items::event` at 110-119): its start, and the leftover columns an
    empty event carries (D11).
- **The walk.**
  - After the parse's one pass, one walk over the lines builds every gutter. It builds the quote
    part with `gutter` (`gutters.rs:174-195`), then the item part (D11).
  - It appends each holding list's width key to `Markdown::width_keys`, one vector for the whole
    note, and `keys` holds their range. The pass has ended, so every key is final, and no line
    is back-filled.
  - The walk visits only lines inside a leading quote or a gutter item. It jumps from one container
    to the next.
  - The walk is `Items::gutters` (`items.rs:170-236`). It returns an `ItemGutters`
    (`items.rs:266-275`). It takes the quote lines from `quote_gutters` (`gutters.rs:135-158`) and
    adds the item part with `item_part` (`items.rs:240-262`). A line's items are the records whose
    marker stands on it, by `line_start`.
  - Between two marker lines, `Walk::held_lines` (`crates/markdown/src/items/walk.rs:56-101`)
    visits each line that the held items hold, and stops at the first line that none holds. `Held`
    (`walk.rs:14-18`) keeps each held item's record and its hang column.
  - `Walk::leaf_text` (`walk.rs:105-108`) gives a line's first `LeafText`, and `Walk::in_block`
    (`walk.rs:111-114`) tells a fence or a blank line inside a code block from any other line. Each
    reads its records in order, so the walk passes each once.
- **The quote part counts columns.** `gutter` counts the blanks before each `>` in columns, as
  `LineStart` does (`Blanks`, `gutters.rs:199-233`; `pulldown-cmark-0.13.6/src/scanners.rs:160-187`;
  `pulldown-cmark-0.13.6/src/firstpass.rs:109-114`).
  - A tab stops at the next multiple of 4 from the line's start.
  - The one blank after a `>` takes one column. When it is a tab, the tab's other columns count
    before the next `>`.
  - A `>` opens when the blanks before it span at most three columns. Each `>` takes up to three
    columns before it, entering a tab whole (`pulldown-cmark-0.13.6/src/parse.rs:1515-1518`).
    pulldown-cmark opens a quote only where that same scan finds it, so `gutter` makes one scan per
    `>`, continued or opening.
  - Before T6c1, `gutter` counted only spaces. So on `> \t> - [ ] a` it found one `>` of the two
    that pulldown-cmark nests, and `> \t> a` showed one bar for two levels.
- **One line.** pulldown-cmark reads a checkbox only on its item's marker line. It scans the task
  marker right after the list marker, and only when the rest of that line is not blank
  (`pulldown-cmark-0.13.6/src/firstpass.rs:139-161`). A `debug_assert!` states this, and
  `-\n  [ ] a` is no task.
- **The last line.** `parse` reads the text's last line as if a `\n` ended it, so `- [ ]` there is a
  task with no text, as `- [ ]\n` is. No output reaches the added byte: no span or marker holds a
  `\n`, and every range ends before its line ending. A proptest pins it over every offset
  `Markdown` holds (`parse/tests/last_line.rs`).
- **One lookup.** `Markdown::task_at(line_start) -> Option<&Task>` (`gutters.rs:107-118`) returns
  `gutter.item?.task` for the gutter that starts at the line's start. It finds that gutter by binary search over `gutters`.
  `cycle.rs`, `DocumentLayout::task_at` and the tests call it.
- **(A) Change detection.** `Gutter::same` (`crates/markdown/src/changed.rs:169-198`) compares the
  item part: its mapped start, its `indent`, the width keys its `keys` indexes in each parse, and
  its task's mapped checkbox and `checked`. Without it, a toggle, a nesting change or a list that gains a digit would leave a
  carried line stale. `changed` walks no new vector (D9).

**The structure `⌘L` reads.** `Markdown` has `structure: Structure`, filled in the parse's one
pulldown-cmark pass (`parse.rs:90-147`):

```rust
pub struct Structure {
    /// Each list item's marker, the bullet or the first digit, in document order.
    pub items: Vec<ByteOffset>,
    /// Each block quote's start, where pulldown-cmark starts its range, in document order.
    pub quotes: Vec<ByteOffset>,
    /// Each code block, front matter, table, HTML block and ATX heading, from its first line's
    /// start to its last line's end, and each line holding a task with no box. Sorted; none holds
    /// another.
    pub skipped: Vec<Range<ByteOffset>>,
}
```

- `crates/markdown/src/structure.rs` holds the type and the collector the parse loop calls once
  per element it opens, in its `Event::Start` arm. `parse.rs` gains only the field and the call.
- An item holds its marker only. `⌘L` reads the content start of the one item it needs per line,
  from the line's start (D6), so the parse reads no blank after a marker.
- `Items` decides the box, so it records each line holding a task with no box. The parse merges
  those lines into `skipped`, in order.
- A range in `skipped` ends before its last `\n`, as a `CodeBlock`'s does, so no offset reaches
  past the text.
- The layout reads none of `structure`, so `changed` compares none of it.

**Its cost per keystroke.** It adds no scan. The parse loop already visits every event; the
structure adds one push per item, quote and skipped range. The 500 KB note of the parse bench
(`crates/markdown/benches/parse.rs`) holds about 4,800 items, 1,200 quotes and 2,400 skipped
ranges, a code block and an ATX heading per section. That is about 8,400 pushes and 90 KB, beside a
parse of 5.58 ms on 8b9db360. Measured on T6a, alternating with 003bbe95, the collector costs 1.5 to
2% of `parse` (History).

**The gutters' cost per keystroke (T6c).**

- It adds no pass: pulldown-cmark still runs once.
- The quote walk visits every line from the first quote to the last (`quote_gutters`,
  `gutters.rs:135-158`). The item walk visits each marker line, then each line an item holds up to
  the next marker line (`Walk::held_lines`).
- On the 500 KB note of the parse bench (`crates/markdown/benches/parse.rs:11-27`), each section
  holds two quote lines and four list lines: a bullet, a task, a nested bullet and a number.
- The gutter-item records reuse `structure.items` for their markers, so no record repeats one.
- T6c adds about 9,400 pushes: the item and list records, about 3,600 gutters of about 90 bytes,
  and about 1,200 item conceals. A `marks` vector would have added 3,600 more (History).
- A line under an ordered list allocates no key vector of its own: its keys go to the shared
  `Markdown::width_keys` (Open question 10).
- Outside a quote, a line's `quotes` stays an empty `Vec`, which allocates nothing.
- On the bench note, `1. a numbered item` holds the blank line after it, so `held_lines` visits
  about 1,200 lines on 500 KB: one blank line per section, outside a code block. A note with long
  items costs more, one visit per held line.
- `held_lines` releases the items that end before a line, and checks that one still holds it,
  before it searches the line's end. So a call that holds nothing searches no line end.
- The hang column is computed once per marker line, from that line's item part, and kept in
  `Held`. A blank line outside a code block pays nothing for it.
- A code or HTML block's text event costs one push when a gutter item holds it, and its held line
  reads that record once (D11). The bench note's code blocks lie outside every item, so they push
  nothing.
- T6a's 8,400 pushes cost 1.5 to 2% of `parse` (History), so T6c may sit close to its gate.
- **The parse gate.** `markdown_parse/500000` stays within 3% of `task-boxes-t6b`, measured in
  alternating rounds. Past the gate, profile before merging.
- **The layout gate.** `cargo bench -p inkra-app --features bench --bench layout` stays within 5%
  of `task-boxes-t6b`, measured in alternating rounds.
  - It measures one keystroke's or caret move's `shape_document` (`crates/app/benches/layout.rs:117-161`).
    Its prose holds a bullet and a task, so it covers a dirty list line.
  - It does not measure an open or a resize, where every list line now builds a `Prefix`. No bench
    gates that path, because the layout bench shapes only after an edit or a caret move.

**Columns count from the line's start, as in pulldown-cmark.** `⌘L` counts an item's blanks in
columns from its line's start: every byte is one column, and a tab stops at the next multiple of 4.
`LineStart` counts a tab's width from the byte after the last tab it scanned, which lies on a stop,
so its stops count from the line's start too (`pulldown-cmark-0.13.6/src/scanners.rs:160-187`, the
tab at 173-181).

- A quote between two items counts as bytes: in `- > - a`, the count passes the outer `-`, the
  blank, the `>` and its blank, and the inner `-` sits at column 4.
- A tab after a `>` needs no special case. The `>` takes one of its columns, and `LineStart` keeps
  the rest as indent before the next marker (`spaces_remaining`, line 181). In `- >\t- a` the tab
  spans column 3 alone; in `>\t- a` it spans columns 1 to 3, and two remain after the `>`. In both,
  the inner `-` sits at column 4 and its content start is the `a`.

## D2. The conceal

**(B)** An item part with a mark has one conceal, the item conceal (`gutter_conceals`,
`gutters.rs:239-265`).

- Its element and its marker run from the item part's start through the end of its last mark: a
  bullet, a number's `.` or `)`, or a checkbox's `]`.
- An item part with no mark emits no conceal: blanks show nothing to reveal. That covers every line
  but a marker line, and an empty item part, as on line 2 of `- a\nb`. A marker is never empty
  (`crates/markdown/src/parse.rs:19`).
- The replacement is `""`.
- Why: with the checkbox alone as the element, `⌃A` on `- [ ] a` put the caret at 0 and revealed
  nothing. Typing then landed before a hidden bullet.

Behaviour:

- On a marker line, a caret anywhere in the item part reveals it. That covers its blanks, its outer
  markers, and the one blank after its last mark, which sits at the element's end.
- On a marker line, the text start lies one byte past the element's end. So a caret there does not
  reveal, just as a caret at a bullet's text never revealed its `•` (live-preview D2). After a click
  on a box, the box stays painted.
- An item part with no mark never reveals, so it never widens. A caret in its blanks or at its text
  start keeps the line's `x`.
- Home goes to the line's start (`to_row_edge`, `crates/app/src/text_input/actions.rs:84-103`). On
  a marker line it reveals the item part, as it revealed a bullet. On any other line it lands
  before the hidden blanks and reveals nothing: the caret paints at the text's `x`, and a letter
  typed there lands before the blanks.
- A gutter item's bullet has no `•` conceal, because its slot paints it. Any other item's bullet
  keeps its conceal (`bullet`, `crates/markdown/src/items.rs:305-314`).
- The quote part and the item part reveal apart (markdown-blocks S1 review). In `> - a`, a caret on
  `>` reveals the quote part only.
- The marker equals the element, because no other conceal starts in an item part. An item part
  holds blanks, markers and a checkbox. A gap conceal skips a line's leading `[ \t>]*`, and a fence
  or heading conceal starts at or past the text start.

The `debug_assert!` in `line_prefix` (`crates/app/src/editor/layout/prefix.rs:131-142`) counts a
gutter's own conceals: its quotes, then one for its item part when that part has a mark. The
G-property in `parse/tests/gutters.rs` checks the same count, and that a part with no mark has no
conceal.

## D3. The gutter and its width

The gutter is a hanging indent: the line's text, wrapped rows included, starts at its right edge
(markdown-blocks D3). `line_prefix` (`crates/app/src/editor/layout/prefix.rs:113-178`) builds it
from the line's `Gutter` and its own conceals. `GutterCarry::layout`
(`crates/app/src/editor/layout/prefix/gutter.rs:152-205`) measures it, and `Marks::layout`
(`prefix/marks.rs:46-107`) lays out the item part's slots and marks.

**Concealed.** The gutter is the quote part, then the slots of the item part.

- The quote part is measured on its own text and draws its bars. `GutterCarry` keys that geometry
  by the quote part's text, once per `shape_document`. A lazy line carries it (D).
- The item part belongs to the line. Its slots are its `indent` blank slots, then one slot per mark
  in order, then the box (D1).
- A slot is `TASK_BOX_SIZE + LIST_SLOT_GAP` wide (D8), except a number's.
- The box sits at its slot's left, centred on row 0.
- A `•` centres on the box's axis: the middle of its advance sits half of `TASK_BOX_SIZE` right of
  its slot's left.
- Marks shape at the body size with one run of `MARKER`'s style (`show`, `Shape::Mark`,
  `crates/app/src/editor/layout/runs.rs:158-187`). Each distinct mark text shapes once per
  `shape_document`.
- Every mark carries `MARKER`. `MARKER` colours a run `md_marker` whatever else it carries
  (`crates/app/src/theme/markdown.rs:43-45`), so the mark's text alone can key its shape.
- The gutter's width is the quote part's width plus the slots' widths. The text starts there on row
  0, and wrapped rows start at the same edge.
- A heading in an item keeps body-size slots, as a quoted heading keeps a body-size gutter
  (markdown-blocks D3). So the slots of one list line up.
- On a heading line, a `•` or a number paints with its baseline on the heading's first-row baseline.
  The box stays centred on row 0, as on any line (D4).

**One number width per list** (Open question 8, decided).

- A number's slot is its list's number width: the widest number's advance plus `LIST_SLOT_GAP`. It
  is never narrower than the standard slot. The layout reads the widest number from the list's
  width key, below.
- The number is right-aligned in its slot: its right edge sits `LIST_SLOT_GAP` left of the slot's
  right edge. So the dots of one list line up.
- So the text of one ordered list starts at one `x`. Its children and its other lines start at that
  text, because their blank slots for the list's column take the same width.
- **The key.** The width comes from the width key that `ItemPart::keys` indexes (D1): the digit
  count of the list's longest number, and its delimiter.
  - It is the smallest key the layout can measure without the list's other lines. A marker's range
    would point at another line, which `show` cannot shape with this line's runs. A copy of the
    marker's text would allocate.
  - It holds no number, so the slot fits every number of that many digits. Digits differ in advance
    in the UI font, so that can be a few pixels wider than the list's widest number, never narrower.
  - An edit that keeps every list's longest digit count leaves every key, so it dirties no other
    line (D9).
  - `GutterCarry` holds the note's `Markdown::width_keys` for one `shape_document`
    (`GutterCarry::new`, `crates/app/src/editor/layout/shape.rs:67`). It gives `Marks::layout` the
    keys of each line's item part.
- **The measure.** Once per distinct key per `shape_document`, the layout shapes the key's widest
  number: the widest digit, `digits` times, then the delimiter.
  - It shapes in the body font with `MARKER`'s run style, as the marks paint.
  - The widest digit comes from one shape of `0123456789`, once per `shape_document`: the digit
    with the largest glyph advance (`widest_digit`, `prefix/marks.rs`).
  - `Marks` (`prefix/marks.rs:36-41`) keeps the slot per key, `Marks::numbers`, and the widest
    digit for one `shape_document`, beside the marks' shapes. `GutterCarry` holds it beside the quote parts.

The expected geometry, concealed:

- In `- [ ] a\n- b`, the box and the `•` share a vertical axis. `a` and `b` start at one `x`, the
  slot's right edge.
- In `- a\n  - b`, `b`'s slot starts at `a`'s text. `    - b` and `\t- b` under `- a` do the same.
- In `8. a\n9. b\n10. c`, one list, the slot fits two digits. The three dots share one `x`, and `a`,
  `b` and `c` start at one `x`. In `8. a\n9. b\n10. c\n    - d`, `d`'s slot starts at `c`'s text.
- In `1. a\n- b`, two lists, `a` and `b` start at one `x` when `1.` plus the gap fits the standard
  slot. In the UI font at 18 px it does (D8). A list of `1)` to `9)` does not fit, so its own text
  starts further right, at one `x` for the whole list.

**(C) A toggle never changes a concealed width.** A slot's width depends on its mark's kind, its
list's width key and the font. The checkbox's state changes none of these, and `DONE` changes only
a mark's colour (D7). So a toggle never rewraps a concealed line.

**Revealed.** A caret in a marker line's item part shows its raw text (D2). An item part with no
mark has no conceal, so it never reveals and never widens.

- The raw item part paints with its end at the text start. It is right-aligned, as a revealed
  heading marker is (live-preview D12).
- The width is the larger of two values: the concealed width, and the quote part plus the raw item
  part. So a reveal moves the text only where the raw text is wider than its slots. A top-level
  task's `- [ ] ` is wider; a bullet's `- ` never is, because it is narrower than a slot.
- The prefix range is `gutter.range` in both states. `line_prefix` still splits its conceals at
  `gutter.range.end` (`prefix.rs:121-127`). So the text and its conceals do not depend on the state.
- `Shown` holds the raw shape of the gutter's text, split at `quote_end` (`Shown::new`,
  `prefix/gutter.rs:58-83`). Each revealed part paints as its own piece, never the whole text as
  one:
  - the quote part's piece at 0;
  - the item part's piece at the width minus its own advance.
  - So a selection that reveals both parts paints two pieces, and moves neither sideways.
- The line wraps at the column minus its indent (`line_wrap`,
  `crates/app/src/editor/layout/shape.rs:344-353`). So a widened gutter rewraps the line, as a
  revealed `**` does.
- The caret and a click in the raw part map through `Shown` (`GutterLayout::offset_for_x`,
  `prefix/gutter.rs:27-30`). A click on a concealed slot, or left of the raw piece, lands at the
  text start.
- The quote part keeps its behaviour:
  - Revealed, its raw text paints at 0.
  - Concealed, only its bars paint.
  - Its raw width equals its concealed width.

**Where the raw width comes from.** `layout` already shapes the gutter's whole text raw while a part
is revealed, to paint it (`prefix/gutter.rs:160-166`). So the raw width costs no extra shape. It
belongs to the line and is never cached.

**The records.**

- `Measured` is gone. `GutterCarry::quotes` (`prefix/gutter.rs:118-126`) maps each quote part's
  text to its `Geometry`. `Geometry::measure` never reads `Reveal`.
- `GutterLayout` (`prefix/gutter.rs:14-23`) replaces `task_box` with the line's concealed marks
  (`Mark`, `prefix/marks.rs`). Each
  entry holds a mark's `x` and what it paints: a shaped `•`, a shaped number, or the box with its
  `checked`. The field is set only while the item part is concealed.
- These items are removed: `TaskShape`, `TaskBox`, `Hidden`, `GutterLayout::lead`, `Prefix::lead`
  and `GutterCarry::slot`.
- `Geometry` (`prefix/gutter.rs:105-113`) keeps the width, the bars' `x` and the quote part's width.
- The 256-byte assertion on `LineLayout` (`crates/app/src/editor/layout/line.rs:99`) stays the
  check. The marks and `Shown` stay boxed, so T6c adds no inline field.
- The doc comment on `LineLayout::prefix` (`line.rs:91-92`) names the item part. The comment on
  `display_line` (`shape.rs:403-404`) says a reveal changes only the wrap width; its T6b form, "no
  reveal can re-wrap its line", was stale.

**(D) A lazy line carries the quote part only.**

- `GutterCarry::pass` (`prefix/gutter.rs:142-147`) carries `Geometry::quote_part`. `layout` gives it
  to a line that misses some of its `>`.
- The item part is never carried: each line gets its own from the parse (D11).
- Under `> - [ ] a\nb`, line `b` takes the quote part's width plus one blank slot. So it starts
  under `a`, and it paints no box.
- A mark comes only from the line's own item part, never from carried geometry.

## D4. Paint and hit test, per line

No box vector. **(E)** A vector built in `shape_document` beside `bars` would be a second O(lines)
pass per keystroke, for data each line already holds. `layout/mod.rs` also stands at the 400-line
limit of an unlisted file (`scripts/line-ceilings.sh:7`).

- **The square.** `LineLayout::box_square` (`line.rs:134-137`) gives the square that a concealed box
  paints and that a click hits. It is `TASK_BOX_SIZE` wide from its slot's left, centred on row 0.
  `paint_marks` builds it from the box mark's `x`; `LineLayout::task_box` (`line.rs:108-111`) finds
  that mark for the hit test and returns its square, or `None`.
- **Paint.**
  - The visible-line loop calls `paint_marks` on each line (`crates/app/src/editor/layout/paint.rs:206`).
    `paint_marks` (`paint.rs:227-269`) replaced T5's `paint_task`.
  - It paints each concealed mark at its `x`. A `•` or a number paints through `ShapedLine::paint`.
  - A text mark paints at the `y` that puts its baseline on the line's row-0 baseline. gpui
    sets a baseline at half the leading plus the ascent
    (`vendor/zed-reference/crates/gpui/src/text_system/line.rs:557-558`). So the offset is the
    line's baseline minus the mark's: 0 on a body line, more on a heading.
  - The line's ascent and descent come from its fonts' metrics at its size, never from its shape:
    `LineLayout::text_metrics` (`line.rs:113-131`) takes the largest over the line's runs, as
    `gpui_apple` lays a line out (`vendor/zed-reference/crates/gpui_apple/src/text_system.rs:567-570`).
    An empty line keeps one run but shapes no glyph, and its shape reports an ascent and a descent
    of 0. So the mark of `- ` or `1. ` painted about 6 px high before the cross-slice review.
  - The box paints through `paint_task_box` (272-316), which no longer takes a lead. Unchecked:
    `outline` in `md_marker`. Checked: `fill` in `accent`, then the check by `window.paint_svg` in
    `editor_background`.
  - The marks paint with the gutter, after the selection (`crates/app/src/text_input/paint.rs:103-106`).
    gpui: `vendor/zed-reference/crates/gpui/src/window.rs`, `paint_quad` 4934, `paint_svg` 5235,
    `fill` 8128, `outline` 8140.
- **Hit test.** `DocumentLayout::task_at(local) -> Option<(Diff, ByteOffset)>` (`paint.rs:322-334`)
  returns the task's flip and its text start (D5). It finds the line under `local.y` and requires
  `local` inside its square. **(F)** `row_for_y` clamps
  (`crates/app/src/editor/layout/mod.rs:241-254`), so the square, not the line lookup, rejects a
  click below the last line. The square fits inside row 0: 16 px in a row of 28 px at least.
- The task is the one in the layout's own parse, `Markdown::task_at` at the line's start (D1). A
  carried line holds no document offset.
- **A revealed item part has no square.** A click on it places the caret on the byte under the click
  (D3).
- A click on a `•` or a number lands at the text start, as a click on any concealed slot does.

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
  `layout/mod.rs:83-84`), and `Buffer::apply_diff` refuses a diff whose base is not the buffer's
  version. After a reload, the last layout describes the old text until the next render
  (`crates/app/src/editor/reload.rs:24-26`). Three matching bytes would not prove the task is the
  same one. A mismatch edits nothing, and the click places the caret as any click does. Zed only
  debug-asserts here (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:1220`).
- The edit is `Task::flip` (`gutters.rs:124-130`): the middle byte, `x` for an unchecked one, a
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
   `parse.rs:209-217`).

`- [ ] ` is offered only where no item opens (Open question 6, decided). On a line where an item
opens it would land at the first item's marker, which would then read as the new task's text, so
that opener would be lost and the line would fail. So a line has at most two candidates, and only a
line where no item opens has two.

**The fallback.** A line where no item opens can still start with a marker that the parse reads
otherwise: a setext underline (`a\n-`), an empty item that cannot interrupt a paragraph (`a\n- `),
or a lazy number (`a\n2. b`). The fallback runs one pulldown-cmark pass, through the structure
collector, on that line's text after its leading `[ \t>]*`. It reads the text as `parse` reads a last
line. The innermost item it finds, the last that opens, gives the content start, as candidate 2
takes it, mapped back by the prefix's length (Open question 5, decided).

- No marker grammar lives in `cycle.rs` or `structure.rs`: pulldown-cmark reads every marker.
- Columns count from the line's start, but the pass starts its tab stops at the text it is given.
  So the pass reads the text after the prefix preceded by `c mod 4` spaces, where `c` is the
  prefix's width in columns. Every tab then stops where it stops in the line, and the pad stays
  under the four columns that open an indented code block. The mapping back subtracts the pad as
  well. So `a\n -\t  ` gives `a\n -\t  [ ] `, as on `main`. In the line the tab spans two
  columns, so the blanks span four. Without the pad the tab would span three, the blanks five, and
  the checkbox would go past the tab alone.
- The candidates are built once, before the first round (`cycle.rs:44-51`). So the fallback costs
  one pass of O(line) per cycled line where no item opens, once per `⌘L`.
- It reads the innermost item, as candidate 2 does. After a paragraph line, `2. 3. b` tries
  `2. 3. [ ] b`, which fails, since a lazy `2.` is text. The line then lands `- [ ] 2. 3. b`, as on
  `main`. Under a new item, `a\n2. 3. b` selected gives `- [ ] a\n2. 3. [ ] b`, and `a\n\t- - b`
  gives `- [ ] a\n\t- - [ ] b`, as on `main`.
- A thematic break of `-` or `*` reads as one alone as well as in the note: pulldown-cmark tries
  the break before the list marker (`pulldown-cmark-0.13.6/src/scanners.rs:315-322`). So the
  fallback finds no item, and the line takes `- [ ] ` in front: `- - -` gives `- [ ] - - -`, and
  `* **` gives `- [ ] * **`. `main` gave `- - - [ ] `, three nested items, and `* [ ] **`. With the
  skipped ATX headings (above), these are the `⌘L` outputs T6a changes.

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
  goes to the item's `Open` and to every element opened inside it so far (`parse.rs:92-97`).
  Later paragraphs inherit it through `Open::enter` (`parse/open.rs:30-56`).
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
- Continuation and lazy lines restyle through the span diff (`changed.rs:128-135`), so a toggle
  restyles every row of the item.
- **A wrapped row strikes its trailing space.** The space that ends a wrapped row is the item's
  text, so the line runs past the last word.

One predicate decides both the box and `DONE`: the pending slot resolving a task that gets a box
(D10). A task that gets none (`- > - [ ] a`) gets neither box nor strike, so `⌘L` can never make a
struck line without a box.

## D8. Theme

D8 adds no colour token: `md_marker`, `accent` and `editor_background` exist
(`crates/app/src/theme/colors.rs`). The token count test (`crates/app/src/theme/file/tests.rs:9`)
stays at 41.

`theme.rs` holds these constants beside `QUOTE_BAR_WIDTH` (`crates/app/src/theme.rs:292-304`):

- `TASK_BOX_SIZE` (16 px)
- `TASK_BOX_RADIUS` (4 px)
- `TASK_BOX_BORDER` (1.5 px)
- `TASK_CHECK_SIZE` (12 px)
- `LIST_SLOT_GAP` (10 px)

They come from mockup C: a 15 px box with a 4 px radius, a 1.5 px border and a 10 px gap, at 15 px
text. At the 18 px body the box takes 16 px and the check 12 px. The gap keeps mockup C's 10 px,
because the box grew by only one pixel (Open question 9). A slot is then 26 px.

Measured at 18 px in the body font, `NSFont.systemFont(ofSize: 18)` (History):

- `1.` to `9.` plus the gap fit one slot, with under 1 px to spare: `4.` is 15.36 px.
- `2)` to `9)` are 16.2 to 17.5 px, so a list of them widens its number width (D3).
- `10.` is 22.92 px, so a list that reaches 10 widens too.

A distinct box colour waits for a theme that asks for one.

## D9. Incremental draw

- **A toggle.** A toggle is a one-byte splice in the task's own line (`changed.rs:17`).
  `Gutter::same` compares the task (A), so no carried line keeps a stale box or a stale strike.
- **A nesting change.** `Gutter::same` compares the item part. An edit that changes nesting, such as
  two blanks typed before `- b`, changes the gutters of exactly the lines whose item part changed.
  The middle walk dirties those lines, so no carry rule is added.
- **A list's width key.** Every line under an ordered list indexes its key (D1), and `Gutter::same`
  compares the keys each parse's `width_keys` holds there, not the ranges. So a ninth item
  becoming a tenth changes the key of every line in the list and under it, and the list rebuilds
  once. An edit that keeps every list's longest digit count dirties no other line.
- **No marks vector.** A mark changes only through an edit on its own line, which is dirty already,
  or through a change of the gutter, which `Gutter::same` sees. So `changed` walks no vector for
  marks.
- **Lazy lines.** A line that is lazy in a quote still follows the rebuild of the line above
  (`lazy_gutter`, `shape.rs:168-175`). A line that is lazy only in an item needs no such rule,
  because its item part is its own.
- **A reveal flip.** A reveal flip dirties its line through `Carry::flipped`
  (`crates/app/src/editor/layout/dirty.rs:69-79`). A bullet's reveal keeps the gutter's width. A
  task's reveal can widen it, so that line reshapes at a new wrap width. The text and its runs stay
  the same, because the prefix range does not depend on the state.
- **Caret moves.** `is_shaped_from` (`layout/mod.rs:115-125`) compares the conceals the selection
  touches. So a reveal reshapes, and a caret move inside a revealed item part does not.
- **The oracle.** The layout oracle of the incremental-draw spec (D10) holds tasks, toggles and `⌘L`
  batches in its alphabet (`crates/app/src/editor/layout/tests.rs`). Its toggle clicks a box of the
  carried layout through `DocumentLayout::task_at` (T6a). T6c adds bullets, numbers, a tenth item,
  two leading blanks and a lazy line, so nesting and width edits run against a full layout.

## D10. The box rule

**The box rule.** A task item gets a box when it is a gutter item (D11).

- This is the same rule that gives an item its slots. `Open::enter` evaluates it once per element.
- It replaced T5's byte scan for a `>` before the marker (`task_gutter`, removed in T6c1).
- The two rules agree once `gutter` counts columns before each `>` (D1). A marker line opens an
  item, so it is never lazy, and it holds all its `>`. So the scan's first check,
  `quotes.len() == depth`, holds on every gutter item's marker line.
- The scan's second check stays as a `debug_assert!`. pulldown-cmark opens a list item mid-line
  only after container markers. So a gutter item's line holds no `>` between its quote part and its
  marker.
- Without the column count, `> \t> - [ ] a` found one `>` of two. T5 gave it no box, the gutter-item
  rule gave it one, and the assertion fired.

Examples:

- `- [ ] a`, `  - [ ] a`, `> - [ ] a`, `> \t> - [ ] a`, `1. [ ] a`, `- - [ ] a` and `1. - [ ] a` get
  a box.
- `- > - [ ] a` and `- a\n  > - [ ] b` do not. A quote inside a list item has no gutter
  (markdown-blocks, As built item 1), so the task behind it is not a gutter item. It keeps its `•`
  and gets no strike (D7), and `⌘L` skips its line (D6). Its line still shows the outer item's slot.
- One rule still decides the box, `DONE` and the skipped line.

**Only the task's own bullet gives way to the box**, as Obsidian draws it. An ordered task's number
is content: it gives the order of the steps. So the number keeps its own slot before the box. Outer
items that open on the same line keep their marks in their own slots.

## D11. Items in the gutter

**Which items.**

- A gutter item is a list item whose ancestors are leading quotes, gutter items and their lists.
- `Open::enter` (`crates/markdown/src/parse/open.rs:30-56`) decides this once per element, beside
  `leading`; `Open::in_gutter` (`parse/open.rs:59-61`) reads it for the next item. A list or an item is `gutter` when its parent is `gutter` or when it has no parent. A
  quote is `gutter` when it is leading.
- Only a gutter item gets slots.
- Any other item keeps its marker inline, as `•` or as the number, behind the `>` that stops the
  chain (markdown-blocks, As built item 1).

**Its columns.** A gutter item has one column per mark it shows:

- a bullet item has one column, its `•`;
- a bullet task has one column, its box, because the box takes the bullet's place (D10);
- an ordered item has one column, its number;
- an ordered task has two columns: its number, then its box.

**Nesting by level** (Open question 7, decided).

- A line's item part has one slot per column of each gutter item that holds the line, outermost
  first.
- Columns of an item whose marker stands on an earlier line are blank slots.
- Columns of the items that open on the line show their marks.
- So a child's first slot starts at its parent's text. An ordered task's children start under its
  text, past its number and its box.
- The source's blanks set no width: `  - b`, `    - b` and `\t- b` under `- a` lay out alike.
- A number's column takes its list's number width, blank or marked (D3). So a list's children start
  at its text, however wide its numbers are.

**The marker line.**

- Its item part runs from the quote part's end through its last mark, plus at most one space or tab
  after that mark.
- It holds the blanks before the first marker, the marker of every gutter item that opens on the
  line, and a task's checkbox.
- Further blanks stay in the text, as they do after a checkbox: `-   a` shows two spaces before `a`.

**Every other line of a gutter item.** This covers a paragraph's later lines, a later paragraph, a
code block, a table, and a quote inside the item.

- Its item part is its blanks from the quote part's end, as long as each blank's column is below the
  innermost gutter item's hang column.
- The hang column is the column after the marker line's item part, by `column_after`
  (`crates/markdown/src/items.rs:296-301`). So in `-\ta\n    b`, both lines hide four columns, and
  `b` starts where `a` does.
- So its text starts where the item's text starts, as a wrapped row does.
- Columns count from the line's start, and a tab stops at the next multiple of 4 (D1, columns).
- The walk computes the hang column when it meets the marker line, and keeps it in `Held::hang`.

**A code or HTML block's line** takes another limit: an item part never covers a byte of a code or
HTML block's text, which is a leaf's text.

- pulldown-cmark gives every line of a code or HTML block one or two text events
  (`pulldown-cmark-0.13.6/src/firstpass.rs`, `append_code_text` and `append_html_line`). When the
  line's containers leave columns to the text, the first is an empty event at the line's text
  start, holding those columns as spaces: a split tab's other columns, or an HTML block's own
  indent on its first line. A fence line gets no event.
- So a line's **leaf text start** is the column of its first text event, less the columns an empty
  event holds: `column_after(0, line..start) - leftover`. The parse records each event a gutter
  item holds as a `LeafText` in `Items::event`, in its one pass (D1).
- The item part of a line with a leaf text start is its blanks from the quote part's end whose
  column end is at or before that column. So a tab hides only when all its columns lie before the
  text, and a split tab stays in the text.
- A line of a code block with no text event is a fence: its item part is all its leading blanks.
- So in `- [ ] a\n\n  ```\n  def f():\n      return 1`, the hang column is 6, but `return`'s line
  hides two blanks and keeps its four columns of code indent.
- A fence's own indent hides too: in `- a\n\n     ```\n     x\n       y`, the fence's line and
  `x`'s line hide five blanks, and `y`'s line five of seven. So a revealed fence stands over its
  code.
- In `- a\n  ```\n\tx`, the tab spans columns 0 to 3, the text starts at column 2, and the tab stays
  in the text. In `- a\n\n\t\tx\n\t\ty`, an indented block, the first tab hides on both code lines
  and the second stays, so `x` and `y` line up as in the source. Before the cross-slice review,
  `x`'s line hid both tabs and `y`'s line one, so the two painted one tab apart.
- In `- [ ] a\n\n   <div>\n   x`, the HTML block's first line hides two blanks: pulldown-cmark gives
  the block its own indent of one column. Its second line hides three.
- Only blanks hide. In `- a\n  > -     x`, the inner item's code text lies past `> -`, and the item
  part is `  `.
- The parse styles no blank of a split tab: the leftover columns are no byte, and the tab is not
  code text. The tab keeps its gap style and stays in the line's text with its full advance.
- The limit comes from pulldown-cmark's own events, not from a column the walk derives, because
  pulldown-cmark has already removed the fence's indent and split the tab. One rule covers fenced,
  indented, quoted and item-nested blocks, and a quote marker before the leftover columns, as in
  `- >```\n\t\t>`, stays a marker.
- `column_after` lives in `items.rs`, and `cycle.rs` calls it there. So does `marker_end`
  (`items.rs:288-293`), the marker's end that `checkbox_at` reads (`cycle.rs:148-162`): one byte for
  a bullet, the digits and the `.` or `)` for a number.

**A lazy line** hides the blanks it has and starts at the same place, because it continues the
item's paragraph, as a wrapped row does. A quoted lazy line carries its quote part and takes its
own item part (D3, D).

**Blank lines.**

- A blank line inside a code block that a gutter item holds gets the item part. pulldown-cmark gives
  it a text event, so its item part ends at its leaf text start, as any code line's does. So its
  caret sits at the block's text, inside the block's background.
- Any other line of blanks only gets no item part. Typing on that line can leave the item, so its
  caret does not sit at the item's text. A quoted blank line keeps its quote part.
- So the first letter typed on such a line jumps it to the item's text. That jump is accepted
  (History, the T6c review).

**Which lines an item holds.** pulldown-cmark decides this.

- An item's range from its `Start(Item)` event holds its lazy lines, and the blank lines after it.
- The walk enters an item at its marker's line. The range can start on the line before
  (`marker_start`, `crates/markdown/src/items.rs:279-284`).

Only a marker line's item part has a conceal (D2), so a row whose line opens no item has none.

| Text | Line | Item part | Slots |
|---|---|---|---|
| `- a` | 1 | `- ` | `•` |
| `1. a` | 1 | `1. ` | `1.` |
| `- [ ] a` | 1 | `- [ ] ` | box |
| `1. [ ] a` | 1 | `1. [ ] ` | `1.`, box |
| `- - a` | 1 | `- - ` | `•`, `•` |
| `-   a` | 1 | `- ` | `•` |
| `- a\n  - b` | 2 | `  - ` | blank, `•` |
| `1. [ ] a\n   - b` | 2 | `   - ` | blank, blank, `•` |
| `- a\n  b` | 2 | `  ` | blank |
| `-\ta\n    b` | 2 | `    ` | blank |
| `- a\nb` | 2 | empty | blank |
| `- a\n  > - b` | 2 | `  ` | blank; `> • b` stays in the text |
| `- [ ] a\n\n  ```\n  def f():\n      return 1` | 5 | `  ` | blank; four columns stay in the code |
| `- a\n\n\t\tx\n\t\ty` | 3, 4 | `\t` | blank; the second tab stays in the code |
| `- [ ] a\n\n   <div>\n   x` | 3 | `  ` | blank; one column stays in the HTML |
| `- > - a` | 1 | `- ` | `•`; `> • a` stays in the text |

## Tests

`inkra-markdown`, by example:

- `- [ ] a`, `- [x] a`, `- [X] a`, `* [ ] a`, `1. [ ] a`, `> - [ ] a`, `  - [ ] a`, `-    [ ] a`,
  loose `- [ ] a\n\n- [x] b`: a gutter with a task.
- `- [ ]a` and `-\n  [ ] a`: no task. `- [ ]\n` and `- [ ]` at the end of the text: a task with no
  text.
- `- > - [ ] a`: no box; `- a\n  - [ ] b`: the inner line has a box. `> \t> - [ ] a`: two quotes
  and a box (D1, D10).
- Item parts and marks (T6c1):
  - every marker-line row of D11's table;
  - `* a`, `+ a` and `12) a`;
  - `- 1. [ ] a`, whose marks are a bullet and a number, then the box;
  - `-\n`, with the item part `-`;
  - `> - a`, with the quote part `> ` and the item part `- `;
  - `ItemPart::marks` yields nothing on a line that is not a marker line.
- Width keys (T6c1): `8. a\n9. b\n10. c` gives each line the key of two digits and `.`;
  `10. a\n    - b` gives line 2 that key for its blank slot; `- a\n  1) b` gives line 2 one key, of
  one digit and `)`.
- No gutter item's bullet has a `•` conceal. The inner bullet of `- > - a` keeps its conceal (T6c1).
- The item conceal (T6c1):
  - Its element and marker run through the last mark.
  - A caret at 0 touches it, and so does a caret at the mark's end.
  - A caret at the text start does not touch it.
- `> \t> a` has two quotes, as pulldown-cmark nests them (T6c1).
- An item's other lines (T6c2):
  - On the second line of `- a\n  b`, the item part covers the blanks and emits no conceal, as
    every item part with no mark (cross-slice review).
  - On the second line of `- a\nb`, the item part is empty and emits no conceal.
  - `- a\n  - b\n    c` and `- a\n  - b\n  c` give line 3 two blank slots.
  - A fenced code block in an item gives each of its lines the item part `  `, its blank lines
    included.
  - `-\ta\n\tb` hides the tab, and `-\ta\n    b` hides the four spaces.
  - `-   a\n    b` hides two of the four blanks.
  - `- a\n\n  b` gives the blank line no gutter.
  - A code block's line hides its blanks up to its fence or its leaf text start: in a task, under
    `1. [ ]`, under an outer item after its inner one closed, with a split tab, with a fence
    indented past its item (its fence line too), in a quote, and under a held line's own `> -`
    (D11).
  - An indented code block's lines in `- a\n\n\t\tx\n\t\ty` both hide one tab (cross-slice review,
    M2).
  - An HTML block's lines hide their blanks up to their leaf text start: in `- [ ] a\n\n   <div>\n   x`
    the first hides two blanks (cross-slice review).
  - In `- >```\n\t\t>` and `2) >~~~\n\t\t>`, the second line's `>` stays a quote marker, not code
    text; both panicked a debug build before the cross-slice review (M1).
- No item part covers a byte of a leaf's text: a `CODE_BLOCK` span or an HTML event's range, on
  items holding code and HTML blocks (T6c2, cross-slice review). Its alphabet draws from items,
  quotes, fences, tabs and `<div>`, and each property built on it also checks that `parse` never
  panics.
- `changed` (T6c1, T6c2): each of these edits dirties the lines whose gutter changed, and only
  those:
  - a bullet turned into a number;
  - two blanks typed before `- b` under `- a`;
  - a blank line that ends an item;
  - a tenth item added to a list of nine, which dirties the whole list and its children;
  - a letter typed in the eighth item's text, which dirties that line alone.
- The struck text (`DONE` without `MARKER`) of `- [x] a` is `a` only. It covers a continuation
  line's text but not its indent, every paragraph of a loose item, and no nested item, quote or
  code block inside the item.
- `Markdown::task_at` finds a task by its line's start, and nothing at any other byte (T6a).
- `Structure` (T6a): `- a` holds an item at 0; `- 1. a` items at 0 and 2; `  -\t ` an item at 2;
  `> - a` a quote at 0 and an item at 2; `- > a` an item at 0 and a quote at 2; `a\n-` no item. `skipped` holds a code block, the front
  matter, a table, an HTML block, an ATX heading (`# a`, and `- # a` from the line's start) and an
  unboxed task's line, each from its first line's start and without its last `\n`. A setext heading
  is not in it.
- The cycle: every row of D6's table; a selection ending at column 0; a code block, front matter, a
  table row, an HTML block and `# a`, `- # a`, `> # a` skipped; a mixed selection lands on the first line's next state; a
  splice that changes the next line's laziness; a lost quote fails its own line.
- The fallback (T6a): `a\n- ` and `a\n-`, caret on the second line, give `a\n- [ ] `, as on `main`;
  `a\n -\t  ` gives `a\n -\t  [ ] `, as on `main`, which only the pad gives; `- - -` gives
  `- [ ] - - -` and `* **` gives `- [ ] * **`, where `main` gives `- - - [ ] ` and `* [ ] **`. Under `⌘A`,
  `a\n\t- - b`, `a\n\t* - b` and `a\n2. 3. b` take the checkbox after their innermost item, as on
  `main`. No current test holds any of these lines (`crates/markdown/src/cycle/tests.rs`,
  `batch_tests.rs`).
- The content start (T6a): `> -\t  ` gives `> -\t  [ ] `, its blanks counted from the line's start.
- The cycle's passes (T6a), counted under `cfg(test)`: a batch of flips only runs none; `⌘A ⌘L` on
  `a\nb` runs one parse of the copy and one fallback pass per line; on `- a\n- b` it runs one parse
  and no fallback pass.

`inkra-markdown`, proptest:

- The conceal properties and the G-properties, for a gutter with an item part: `depth` may be 0
  with a non-empty range when `item` is set (`parse/tests/gutters.rs:246-249`). T6c1 adds:
  - Each item part lies in its line, past its quote part.
  - Its own conceals are its quotes plus at most one item conceal, never an empty one, and none
    for a part with no mark (cross-slice review).
  - Every gutter item's marker line holds a gutter whose marks are that item's markers.
  - A gutter item's marker line holds all its quotes, and its box agrees with T5's scan.
- Every line in a gutter item's range that is not blank holds an item part (T6c2). The oracle
  collects item ranges through its own pulldown-cmark pass. A held line where no gutter item opens
  holds blanks only.
- The last line: `parse(text)` equals `parse(text + "\n")`, and no offset reaches past the text,
  `structure`'s included (T6a).
- The cycle: its splices, applied and reparsed, give the target state on every changed line and
  leave every other line byte-identical. Every opener still opens at its mapped byte, and the mapped
  selection stays inside the text and touches the same lines. The test oracles run their own
  pulldown-cmark passes, never `structure`, so they stay independent of it.

`inkra-editor`: `apply_diff` is one undo step, merges with no neighbour, maps the selection, and
edits nothing from a stale base.

`app`, headless `#[gpui::test]` against `MemFs`. The body is 18 px, a glyph is 10.8 px, and a
standard slot is 26 px. Each `x` is compared with another line's measured `x` or with the slot's
constants, never with a glyph's computed width.

- Concealed (T6c1):
  - `- [ ] a\n- b`: the box and `b`'s slot both start at 0. The middle of the `•` sits on the box's
    axis. `a` starts where `b` does, `TASK_BOX_SIZE + LIST_SLOT_GAP` right of the box's left edge.
  - `- a\n  - b\n    - c`: each child's slot starts at its parent's text. `- a\n    - b` and
    `- a\n\t- b` lay out as `- a\n  - b`.
  - `1. [ ] a\n   - b`: `b`'s slot starts at `a`'s text, past the slot of `1.` and the box's slot.
  - `8. a\n9. b\n10. c\n    - d`: `a`, `b` and `c` start at one `x`, and the right edges of `8.`,
    `9.` and `10.` share one `x`. `d`'s slot starts at `c`'s text.
  - At a 12 px body, where `1.` plus the gap fits a slot, `1. a\n- b`: `a` starts where `b` does.
  - `- - [ ] a` paints a `•` in slot 0 and the box in slot 1. `> - [ ] a` paints its box at the
    quote part's width.
  - `- # a`: the `•`'s baseline is the heading's first-row baseline.
  - `- a`, `- `, `1. ` and `-`: the mark's baseline is the one the body font's metrics give row 0,
    so an empty item's mark sits as a typed one's (D4, cross-slice review, M3). The headless text
    system reports a descent below zero where `gpui_apple` reports it above, so the test negates the
    font's descent as `gpui_apple` does.
  - A wrapped bullet's second row starts at its text.
- Toggling keeps a concealed line's text start (C).
- Revealed (T6c1):
  - A caret at 0 of `- a` paints `- ` raw, ending at the text start, and the text keeps its `x`.
  - The same holds for a caret at 0 of the second line of `- a\n  - b`.
  - A caret in the item part of `- [ ] a` moves row 0's text and its wrapped rows right. The shift
    is the raw width minus one slot.
  - A selection over both parts of `> - a` paints `> ` at 0 and `- ` ending at the text start.
  - The caret and a selection inside the item part sit at the shown part's `x`.
- In `> - [ ] a` a caret on `>` reveals the quote part only.
- A click on a box toggles `[ ]`↔`[x]` and moves the caret to the text start; so does a click on
  the box of `1. [ ] a` (T6a). `[X]` toggles to `[ ]`, and `⌘Z` restores the text. A click on a
  carried line's box, after an edit on the line above, toggles that task (T6a).
- A click on a revealed checkbox places the caret and edits nothing. A press on a box then a drag
  selects nothing. `⌘`- and `⇧`-click do not toggle, nor does a window-activating click. A click
  below the last line does not toggle its box. A click on a layout older than the buffer edits
  nothing. A toggle with the view scrolled away from the old caret does not scroll.
- A click on a `•` or a number lands at the text start and edits nothing (T6c1).
- An item's other lines (T6c2):
  - In `- a\n  b`, `- a\nb`, `- a\n\n  b`, `-\ta\n    b` and `> - a\nb`, `b` starts where `a`
    does.
  - Under `> - [ ] a\nb`, `b` starts under `a` and paints no box.
  - A code block in an item starts its background at the item's text, minus the outset
    (`crates/app/src/editor/layout/blocks.rs:118-123`). A caret on its blank line sits inside it.
  - Home on the second line of `- a\n  b` puts the caret at the line's start. A letter typed there
    lands before the blanks.
  - In `- [ ] a\n      b`, a caret at the second line's start or at `b` reveals nothing: both sit
    at `b`'s `x`, and no raw piece paints (cross-slice review).
  - A wrapped continuation line's rows start at `a`'s `x`.
- `simulate_keystrokes("cmd-l")`: on a plain line, on a task, on a selection of three lines, and on
  `- > - [ ] a`, which it leaves. `cmd-enter` on a task line holding a link still follows it.
- The layout oracle, its toggle through `DocumentLayout::task_at` (T6a). Only a concealed box can
  be clicked; the `⌘L` step still cycles revealed tasks.

Scenes: `markdown-lists` and `markdown-lists-end`, on one note (`LISTS`,
`crates/app/src/shot/scenes/markdown_blocks.rs:56-103`), in that file's `SCENES` (121-142) beside
the `markdown-blocks` scenes. `scenes.rs` sits at its line ceiling, so the note gets no module of
its own. The note does not fit one window: `markdown-lists` shows its start, `markdown-lists-end`
its end. Together they show:

- an ordered list from 8 to 11, at the note's start so D3's number width shows in
  `markdown-lists`: it crosses 10, 9 has a second line, and 10 a child;
- bullets three levels deep, nested by two spaces, by four spaces and by a tab;
- a wrapped bullet;
- a bullet with a continuation line and a lazy line;
- tasks beside bullets;
- a list of `1)` to `3)`;
- an ordered task with a child, then an empty `2. `;
- a bullet holding a heading, then an empty `- `;
- a code block in a task, with a blank line and a line indented past the code's own start;
- the caret on a nested bullet's `-`, with its raw `  - ` ending at its text, which does not move.

Re-render `markdown-blocks` (`crates/app/src/shot/scenes/markdown_blocks.rs:7-38`, its `shows` at
124-129) and the concealment scene, `editor-concealed` (`scenes.rs:119-123`). The box now has its gap, and
the quoted task's lazy line starts under its text. Re-render `code-blocks` too, for its block in a
list. To render and look: `cargo run -p inkra-app --features shot --example shot markdown-lists`.

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
8. **T6a, one parse** (~0.5 d). Open questions 5 and 6 are decided. It changes two `⌘L` outputs,
   on a thematic break of `-` or `*` and on an ATX heading's line (D6), and nothing else visible.
   - `inkra-markdown`: `Structure` in `structure.rs`, `Markdown::structure` and
     `Markdown::task_at` (D1). `cycle.rs` reads them and loses `Scan`, `Openers`, `Marker`,
     `marker`, `innermost`, `terminated` and its own `task_at` (D6). It reads the content start of
     the one item it needs per line, and gains the fallback, which runs the structure collector, and
     offers `- [ ] ` only where no item opens.
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
   - T6b does not merge alone. In the real UI font its slot is the box, so the text touches the box
     (History). It merges together with T6c1 and T6c2.
10. **T6c1, the slots** (~1 d). Stacked on T6b: `task-boxes-t6c1` on `task-boxes-t6b`.
    - `inkra-markdown`:
      - `Open::gutter` (D11).
      - `gutter` counts columns before each `>` (D1).
      - `items.rs`, with `Items`, the gutter-item and list records, the width keys, and the walk for
        marker lines.
      - `ItemPart`, `WidthKey`, `ItemMarker` and `ItemPart::marks`; `Task` loses `start` and `run`.
      - One conceal per item part, none when it is empty, and `•` conceals only for other items
        (D2).
      - `Gutter::same` over the item part and its keys (D1, D9).
      - `Open` moved to `parse/open.rs` first, in a commit of its own, so `parse.rs` stays under
        400 lines. `parse.rs` gains only the gutter flag's read and the walk's call.
    - `app`:
      - `LIST_SLOT_GAP`.
      - The quote geometry, keyed by the quote part's text.
      - Slots and marks per line, the number widths per key, and `paint_marks` with the baseline
        offset (D3, D4).
      - The raw item part, right-aligned, in two pieces (D3).
      - The marks live in `prefix/marks.rs`, beside `prefix/gutter.rs`.
    - Interim state: lines other than a marker line still show their blanks in the text. The scene
      shows this, and T6c2 makes those lines start at the item's text.
    - Tests:
      - the T6c1 items of Tests;
      - delete `crates/app/src/editor/layout/tests/task_slot.rs`, because the slot is now a
        constant;
      - in `task_widths.rs`, `task_lead.rs`, `task_gutter.rs`, `task_boxes.rs`, `task_click.rs` and
        `conceal.rs`, move the expected `x` and lead texts to slots and marks.
    - Measure, in alternating rounds with `task-boxes-t6b`:
      - `cargo bench -p inkra-markdown --bench parse -- --quick 'markdown_parse/500000$|cycle_tasks_all'`.
        Gate: `markdown_parse/500000` within 3%.
      - `cargo bench -p inkra-app --features bench --bench layout`. Gate: within 5% (D1).
    - Scene: the marker lines of `markdown-lists` and `markdown-lists-end`; re-render
      `markdown-blocks`.
11. **T6c2, an item's other lines** (~0.5 d). Stacked on T6c1: `task-boxes-t6c2` on
    `task-boxes-t6c1`.
    - `inkra-markdown`:
      - The hang column, after the marker line's item part (D11).
      - The walk over every line of a gutter item: a paragraph's later lines, lazy lines, a code
        block and its blank lines.
      - No item part on any other line of blanks only (D11).
      - No conceal on an item part with no mark (D2; the cross-slice review dropped the conceal
        T6c2 first built).
    - `app`: no new record, because a line with only blank slots lays out by T6c1's code. Check that
      a code block in an item starts its background at the item's text.
    - Tests: the T6c2 items of Tests. The oracle's alphabet gains a lazy line and two leading
      blanks.
    - Measure as for T6c1, with the same gates against `task-boxes-t6b`.
    - Scene: the whole of `markdown-lists` and `markdown-lists-end`; re-render `code-blocks`.

**The merge.**

- T6b, T6c1 and T6c2 merge together. So `main` never holds the glued text or lines that do not
  start at their item's text.
- Review each slice on its own diff: `git diff task-boxes-t6b...task-boxes-t6c1`, then
  `task-boxes-t6c1...task-boxes-t6c2`.
- After T6c2, one cross-slice review reads the whole stack against this spec.
- Then open one PR from `task-boxes-t6c2` into `main` and squash-merge it.
- A single PR avoids the known trap of a stacked PR merged into a dead base.

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
    in `structure.rs`. With it, a thematic break of `-` or `*` takes `- [ ] ` in front: `- - -`
    gives `- [ ] - - -`, since pulldown-cmark reads it as a thematic break.
  - Open question 6 is decided as recommended: `- [ ] ` is offered only where no item opens.
  - Open question 4 is decided: an ATX heading's line is skipped, so `# a` edits nothing. A
    setext heading is not, since `a\n- ` is one.
- **The T6 review's minor findings** (2026-11-18). `TaskShape`'s box `x` is one field. The column
  carry is checked against `LineStart`. pulldown-cmark reads a tab, a vertical tab or a form feed
  as an unchecked middle byte, so the checkbox term and decision 5 say so, and D5 records that a
  flip does not restore a tab. Stale lines in `paint.rs` and `mouse.rs` are updated.
- **The T6a review** (2026-11-18). The fallback read the first item, so `a\n\t- - b` selected
  lost `main`'s nested task; it reads the innermost, as candidate 2 does. `parse` ran about 3%
  slower, so an item now holds its marker only, and `⌘L` reads the one content start it needs per
  line from the line's start. `parse` is then 1.5 to 2% slower than on 003bbe95. Without the
  collector call it matches 003bbe95, and no single arm of the collector holds the difference (D1).
- **T6b's glued text** (2026-11-18).
  - Measured at 18 px in the body font, `NSFont.systemFont(ofSize: 18)`: `• ` is 12.58 px, `1.`
    12.29, `2)` to `9)` 16.2 to 17.5, and `10.` 22.92. The box is 16 px.
  - So T6b's slot was the box, and the text touched it. The text also started about 3.4 px right of
    a sibling bullet's text.
  - The headless `• ` is 21.6 px, so every test passed; only the scene showed the defect.
  - The owner decided that bullets move into the box's slot, and that the slot gains a gap
    (decision 7).
- **T6c, the slots** (designed 2026-11-18).
  - Every gutter item shows its marker in a slot and nests one slot per column.
  - Its other lines start at its text (D3, D11).
  - The box rule became the gutter-item rule (D10).
- **The owner's answers on T6c** (2026-11-18/19).
  - Open question 7 is decided as recommended: nest by level (D11).
  - Open question 8 is decided: numbers in slots, with one number width per list (D3). The width
    is the widest number's advance plus `LIST_SLOT_GAP`, never narrower than the standard slot.
    The numbers are right-aligned in it. The owner asked for this so that the `.` does not shift
    from one number to the next.
  - Open question 9 stays open, to settle on the rendered scene.
- **The T6c review** (red team, 2026-11-18). It found the design sound to build after these fixes:
  - **M1.** The gutter-item rule did not match T5's scan on `> \t> - [ ] a`, because `gutter`
    counted only spaces before a `>`. `gutter` now counts columns (D1), so the rules agree (D10).
  - **M2.** A number slot per number left a ragged text column for `10.` and for `2)` to `9)`, and
    children off their parent's text. One number width per list fixes both (D3, Open question 8).
  - **M3.** `Mark` and `Markdown::marks`, a sorted vector of marks walked by `changed`, caught no
    change that `Gutter::same` misses. They are dropped for `ItemPart::marks` (D1, D9), which also
    drops 3,600 pushes on the bench note.
  - **m1.** An empty item part emits no conceal (D2).
  - **m2.** The hang column follows the marker line's item part, so `-\ta\n    b` lines up (D11).
  - **m3.** A revealed gutter always paints two pieces (D3). "The whole raw text paints at 0" moved
    the item part sideways when a selection widened to the `>`.
  - **m4.** A blank line in a code block that a gutter item holds gets the item part. On any other
    blank line, the jump on the first typed letter is accepted (D11).
  - **m5.** A text mark on a heading line paints on the heading's first-row baseline (D3, D4).
  - **m6.** The cost paragraph counts about 9,400 pushes, reuses `structure.items`, and adds the
    layout gate (D1).
  - The width key, the digit count and the delimiter, was chosen as the smallest key the layout can
    measure (D3).
- **T6c1, the slots** (2026-11-19, `task-boxes-t6c1`). Marker lines show their marks in slots; other
  lines of an item keep their blanks until T6c2.
  - `Open` moved to `parse/open.rs` in a commit of its own; `Items` moved to `items.rs`.
  - `gutter` makes one blank scan per `>`. The separate check for an opening quote was dead:
    pulldown-cmark opens a quote only where the same scan finds it, and mutants showed no input
    tells the two apart.
  - The walk groups a line's items by `line_start`. The comparison against the line's end it
    replaced had a mutant no test could kill, as no marker sits on a `\n`.
  - `ItemPart::marks` takes the item part's text and drops a task's own bullet (D1).
  - `Marks` takes each glyph's advance through a function, so a test fakes uneven digits: headless
    glyphs all advance alike, and no window test can tell the widest digit.
  - The list scenes live in `scenes/markdown_blocks.rs` (Tests). The code-blocks scenes moved into
    their module first, to make room in `scenes.rs`.
  - Measured in alternating rounds against `task-boxes-t6b`: `markdown_parse/500000` +1.6%, every
    layout case within 5%.
- **T6c2, an item's other lines** (2026-11-19, `task-boxes-t6c2`). Every line a gutter item holds
  starts at its text.
  - Between two marker lines, `Walk::held_lines` visits the held lines and stops at the first
    line that no item holds. A blank line takes an item part only inside a code block, which
    `Walk::in_block` reads from the parse's code block ranges.
  - A line with blank slots only lays out by T6c1's code. `app` gained the keys' plumbing only:
    `GutterCarry` holds `width_keys`, and `Marks::layout` takes each line's keys (D3).
  - The held-lines property checks both directions: a held line that is not blank has an item
    part, and no other line has one.
  - T6c1's interim values moved: the code block in an item starts its background at the item's
    text, and the quoted task's lazy line `  b` starts under `a`.
  - Measured in alternating rounds against `task-boxes-t6b`. The layout cases are 3.6 to 6.1%
    faster on the median of 8 rounds, and 1.8 to 2.8% fewer instructions per iteration.
    `markdown_parse/500000` is 5.7% slower, past its 3% gate; T6c1 alone is 1.5 to 1.9% slower.
  - A profile put most of the difference in the walk: its line scans and its per-line setup.
  - Two changes closed the gate. The walk finds a marker line's next line by a byte search.
    `held_lines` releases and checks the held items before it searches a line's end, so a call
    that holds nothing scans no line.
    The keys moved to one shared vector, Open question 10's fallback: `Markdown::width_keys`, which
    each `ItemPart::numbers`, now `keys`, indexes by range. `Gutter::same` compares the keys each parse holds
    there, so a list typed above another leaves the other's lines clean.
  - Measured after them in 12 alternating rounds against `task-boxes-t6b`:
    `markdown_parse/500000` +2.8% on the median, +1.7 to +4.2% per round. In 3 rounds of three:
    T6b 2.924 ms, T6c1 2.960 ms (+1.3%), T6c2 2.973 ms (+1.7%). The layout cases stay 4.7 to 11%
    faster over 3 rounds, with 1.6 to 2.9% fewer instructions per iteration.
  - The review found a code block in a task losing its indent. Under `- [ ] a`, the code line
    `      return 1` hid all six blanks, the hang column of `- [ ] `, so `return` painted under
    `def`. A code block's line now hides only the blanks before its fence or its code text, and
    the parse styles a split tab as code text (D11, Deviations).
  - The held-lines property now also checks that a held line where no gutter item opens holds
    blanks only. It caught a first form of the fix, which ended the item part at the code text even
    past a held line's own `> -`.
  - `Held::hang` is computed at the marker line. Against the lazy form it measured +0.17% at
    500000, the median of 6 rounds' deltas, within noise.
  - Measured after the review fixes in 12 alternating rounds against `task-boxes-t6b`:
    `markdown_parse/500000` +1.6% on the median, −0.2 to +2.8% per round. Over 6 alternating
    layout rounds, the cases run −0.1 to +2.5% on the median, with 1.7 to 2.9% fewer instructions
    per iteration; wall time swung by up to 5% between runs of 3 rounds.
- **The cross-slice review** (2026-11-19, on `task-boxes-t6...task-boxes-t6c2`).
  - **M1.** `Out::text` styled a split tab as code text, and asserted that pulldown-cmark's empty
    leftover text followed a tab. In `- >```\n\t\t>` and `2) >~~~\n\t\t>` it follows a `>`, so a
    debug build panicked and a release build styled the `>` as code. **M2.** The walk ended a code
    line's item part at its first `CODE_BLOCK` span, so `- a\n\n\t\tx\n\t\ty` hid two tabs on one
    line and one on the next. Both are replaced by the leaf text start, read from pulldown-cmark's
    own text events (D11). `Out::text` and its assertion are gone, and the walk searches no span.
  - **M3.** An empty item's mark painted about 6 px high: the line's baseline came from its shape,
    which reports no ascent for a line with no glyph. It now comes from the line's fonts' metrics
    (D4).
  - **m1.** A held line's item part with no mark revealed when the caret touched it, so a task's
    continuation line widened. A part with no mark now has no conceal (D2, D3).
  - **m2.** An HTML block's own indent was hidden under a task. HTML text events join the leaf
    rule, and the leaf property covers HTML (D11, Tests).
  - The tests and the spec say item part and marks, not task part and lead. `Items::numbers`
    became `Items::list_keys` and `ItemPart::numbers` became `ItemPart::keys`, so `numbers` names
    only `Marks`' slot per key. `Items::gutters` returns an `ItemGutters`, and the walk moved to
    `items/walk.rs`.
  - `markdown-lists` opens on the 8 to 11 list, so D3's number width shows; the note gains an empty
    `2. ` and an empty `- `.

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
- **The box slot as the advance of `• `** (T6b) became the standard slot,
  `TASK_BOX_SIZE + LIST_SLOT_GAP`, shared by bullets, numbers and boxes (T6c). In the real UI font
  `• ` is narrower than the box, so the text touched the box.
- **`TASK_BOX_GAP` dropped** (T6b) was reversed. It returns as `LIST_SLOT_GAP` (T6c).
- **The painted lead** (T5) and **a blank lead shaped for its width** (T6b) became slots: one per
  column of each gutter item, blank for an item that opens on an earlier line (T6c). With the lead,
  a child sat under its parent's box: two spaces are about 9 px at 18 px.
- **A bullet as an inline `•`** (live-preview D2) became a mark in its slot for a gutter item
  (T6c).
- **The revealed task part painted from the quote part's width** (T6b) became the raw item part,
  ending at the text start (T6c). From the quote part's width, a nested line's raw text was
  narrower than its slots, so its text jumped left.
- **A lazy line takes no item part** (D, T1a) changed: a lazy line now takes its own item part
  beside the carried quote part (T6c).
- **The box rule as a byte scan for `>`** (T5, `task_gutter`) became the gutter-item rule in
  `Open::enter` (T6c). The results are the same once `gutter` counts columns.
- **A quote part found by spaces only** (markdown-blocks S1) became one counted in columns (T6c).
  `> \t> a` showed one bar for two levels.
- **A code line's item part ending at its first `CODE_BLOCK` span** (the T6c2 review) became the
  leaf text start (the cross-slice review). The split tab it styled as code text is again a gap.
- **An item part with no mark concealed and revealed** (T6c2) became an item part with no conceal
  (the cross-slice review): its reveal widened a task's continuation line.
- **A mark's baseline from the line's shape** (T6c1) became the baseline from the line's font
  metrics (the cross-slice review). An empty line's shape reports no ascent.

**Superseded deviations.**

- **T1a, `Geometry` staged across slices.** Done: the quote part's width came in T1a and the
  checkbox's span in T1b. T6b replaced the span by the box's `x`.
- **T5, the skip pass reads a final `\n`.** Superseded by T6a: `parse` reads the text so, and the
  skipped lines come from it.
- **T5, `show` moved from `shape.rs` to `runs.rs`.** Done; D3 cites it there.
- **T5, no markup turns to text.** Now part of D6.
- **T2, a carried line holds no document offset of its task.** Now part of D4.
- **T5, `TaskBox` holds no checkbox offset.** Superseded by T6c: `TaskBox` is gone, and the click
  still reads the layout's own parse (D4).
- **T5, the lead shapes through all its conceals.** Superseded by T6c: marks in slots replace the
  lead.

## Deviations

- **From Zed.** Zed renders a task as a `Checkbox` element in its separate preview pane
  (`vendor/zed-reference/crates/markdown/src/markdown.rs:2999-3032`). It writes all three bytes on
  toggle, debug-asserting lower-case `[x]`
  (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:1201-1229`). Inkra
  paints a box for the concealed item part in the live editor and writes the middle byte, so `[X]`
  toggles too. It checks the version before writing (G), and moves the caret. Zed's editor has no
  toggle action; `⌘L` follows Obsidian.
- **From markdown-blocks S3.** The conceal covers the item part (B); a concealed width does not
  depend on the state (C); boxes are found per line, not from a vector (E). The marks are laid out
  per line in `prefix/marks.rs`, never as a document-wide vector, and there is no new colour token;
  the strike and the key are added.
- **From this spec, in T2.** `task_at` returns the task's flip as a `Diff` at the layout's version,
  and its text start; `apply_diff` makes the check (G). The hit test (F) is containment in the
  painted square, which fits inside row 0, so it has no separate row check.
- **From this spec, in T4b.** Every failing line moves to its next candidate in the same round, so a
  line can run out before the line above it settles. With `2. b` and `    c` selected after `x`,
  `c` is lazy under the failing `2. [ ] b` and drops out, though `- [ ] 2. b` would let it nest.
  Moving only the first failing line would land it but cost a parse per line on `⌘A`; the cap of
  three parses wins. The fallback (D6) keeps the example: `2. b` tries `2. [ ] b` first, as on
  `main`.
- **From the T6a brief.** An empty item's content start counts its blanks as an item with a
  paragraph does. `CommonMark` 5.2 and pulldown-cmark put it one column past the marker
  (`pulldown-cmark-0.13.6/src/scanners.rs:353-373`). So `⌘L` on `  -\t ` still writes `  -\t [ ] `,
  as before T6a.
- **From Zed, for lists.**
  - Zed's preview draws an item as a row: the marker at its natural width, `gap_1`, then the content
    in a `flex_1` column. So wrapped content starts under the content
    (`vendor/zed-reference/crates/markdown/src/markdown.rs:2328-2349`).
  - A nested list pads 10 px from its parent's content (`pl_2p5`, 2987-2996).
  - A task's checkbox centres in a slot one line tall (3020-3033).
  - Inkra follows the wrapped content and the nesting by level.
  - Inkra gives bullet and box one slot width, and the numbers of a list one width of their own. So
    siblings line up, and so do the dots of one list. Zed's marker column varies with each marker.
  - Inkra adds no extra pad, because the slot's gap gives the space.
- **From the live-preview spec, D2.**
  - A gutter item's bullet leaves the text for its slot. Its element runs from the item part's start
    through the bullet.
  - A caret at the text start still does not reveal it.
  - A click on its `•` lands at the text start, as a click on any concealed gutter does. So a click
    no longer reveals the `-`; Home and the arrow keys still do.
- **From this spec, in T6c2.** D11 as first built hid every held line's blanks below the hang
  column, a code block's lines included. A task's hang column is 6 while its text starts at column
  2, so its code lost up to four columns of its own indent (History, the T6c2 review). A code or
  HTML block's line now stops at its fence or its leaf text start, which the walk reads from
  pulldown-cmark's text events (D11).
  - A fence line hides all its leading blanks, so a revealed fence stands over its code.
  - An HTML block's first line keeps its own indent in the text, because pulldown-cmark gives it to
    the block's text.
- **From markdown-blocks D3.** A lazy line in an item starts at the item's text. Its quote part is
  carried from the line above, and its item part is its own.

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

1. **After `⌘Z` the caret sits inside the checkbox**, so the item part shows raw, right-aligned
   against the text start (D3), until the caret leaves. Recommended: accept; the reveal rule says so.
2. **`- [ ]` with no newline at the end of the note is no task.** Decided: reversed by the owner
   after T4a (History).
3. **A click on the box of a task with no text (`- [ ]\n`) reveals its marker.** Its text start is
   the conceal's end, and a caret at a conceal's end touches it
   (`crates/markdown/src/reveal.rs:15-18`). The raw `- [x]` then paints right-aligned against the
   text start (D3). A typed space conceals it again. A letter typed right after the checkbox makes the
   line text, as `- [x]a` is no task. Recommended: accept for level 0; list continuation
   (Deferred) writes the space.
4. **`⌘L` on `# a` gave `- [ ] # a`**, and the heading turned to the task's text. Decided by the
   owner on 2026-11-18: an ATX heading's line is skipped (D6), so `# a` edits nothing. A setext
   heading is not, so the `- ` typed under a line still cycles.
5. **A marker the parse reads as no item** (T6a). Decided by the owner on 2026-11-18: the
   fallback (D6) reads such a line through pulldown-cmark, and `cycle.rs` keeps no marker grammar.
   Without it, `a\n-` gave `a\n- [ ] -`, and `a\n2. b` both selected gave `- [ ] a\n- [ ] 2. b`.
   The one output that changes is a thematic break of `-` or `*`, as `- - -` (History).
6. **Candidate 3 never lands on a line where an item opens** (T6a). Decided by the owner on
   2026-11-18, as recommended: `- [ ] ` is offered only where no item opens (D6). No current test
   changes.
7. **Nest by level, or by the source's blanks** (T6c). Decided by the owner on 2026-11-18/19, as
   recommended: by level (D11).
   - What it gives: each level is one slot wide, so a child sits under its parent's text, as in
     Obsidian. `  - `, `    - ` and a tab nest alike.
   - What it needs: the gutter flag in `Open::enter` and the walk's item stack. T6c needs both anyway
     to make an item's other lines start at its text.
   - A caret in the hidden blanks reveals the item part raw and right-aligned. So a typed blank
     moves almost nothing until it changes the nesting; then the line moves one slot.
   - The alternative kept the blanks' advance as the indent, about 9 px for two spaces at 18 px. A
     child then sat under its parent's box.
8. **Put a number in a slot, or keep it as a lead** (T6c). Decided by the owner on 2026-11-19: in a
   slot, with one number width per list (D3).
   - The width is the widest number's advance plus `LIST_SLOT_GAP`, never narrower than the
     standard slot. The numbers are right-aligned in it, so the dots line up.
   - So the text of a list running 8 to 11 is one column, and its children and other lines start at
     that text.
   - Going from 9 to 10 items dirties the whole list once (D9).
9. **A 10 px gap, and a `•` centred on the box's axis** (T6c). Open: settle it on the rendered
   `markdown-lists` scene. It is a constant, so it does not block T6c.
   - With a 10 px gap, mockup C keeps its gap and a slot is 26 px. `1.` to `9.` then fit it with
     under 1 px to spare (D8).
   - The alternatives are 8 px, which gives a 24 px slot, or 12 px, which is mockup C's gap scaled
     with the text.
10. **How a line holds more than one width key** (T6c). Closed in T6c2: `Markdown::width_keys`
    holds every line's keys in one vector, and `ItemPart::keys` is each line's range in it. It is
    exact at any depth and allocates no vector per line (D1, History).
    - A line under two ordered lists, as in `10. a\n    1) b\n       c`, has a number slot for each.
      Each takes its own list's width, so the line needs both keys.
    - Decided on 2026-11-19, before T6c1: a `Vec<WidthKey>` per line, with the shared vector as the
      fallback past the parse gate (3%). The per-line vector allocated once per line under an
      ordered list, about 1,200 times on the parse bench's note, and T6c2 measured 5.7%, so the
      fallback was taken.
    - The rejected alternative was one `Option<WidthKey>`, for the innermost ordered list. An outer
      ordered list's slots then take the standard width, so a line under two ordered lists starts
      off its parent's text when the outer list has wide numbers.
