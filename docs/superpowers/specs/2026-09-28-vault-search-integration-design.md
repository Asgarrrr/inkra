# Vault search in the app (`⌘⇧F`)

Date: 2026-09-28. Status: **shipped**, in four slices.

Slice 6 of [`2026-09-24-search-design.md`](2026-09-24-search-design.md) (the
search spec): the delegate, headers, `cmd-shift-f`, `HitActivated`,
`open_note` with a range, `pending_reveal`, the key tests and the shots. The
index side is built: `search`, `Searched::merge`, word-start matching, limits
and cancel ([`2026-09-28-search-word-start-design.md`](2026-09-28-search-word-start-design.md),
the word-start spec). This spec records the decisions the search spec leaves
open, and two changes to `inkra-index` that the rows need.

The search spec's D7 (picker), D8 (modal layer), D9 (rows and states) and D10
(opening a hit) hold, as amended by the word-start spec. Where this spec
differs, it says so under "Amendments".

A first draft was reviewed by a red team and a blue team. The blue team
checked each red finding in the code. Every confirmed finding is folded in
below; a **Corrected** note marks the change.

## Decisions taken with the user

- The run logic of quick open moves to `picker/` and both delegates use it
  (S1).
- A hit row shows its line number (S2).
- `⌘⇧F` opens with an empty field, as `⌘P` does.

## S1. `picker/runs.rs`: one run logic for two pickers

Quick open holds the cancel flag, `search_id`, `Scan`, `Run`, `next_run`,
an `IndexWatch` and `refresh_or_defer` (`quick_open/mod.rs`,
`quick_open/scan.rs`). Vault search needs the same rules: a keystroke
supersedes a background run, a refresh never cancels a scan in flight (D7),
and a notify during loads is throttled by `BUILDING_REFRESH`. Two users make
this a real seam (CLAUDE.md, "Abstract only at a real seam").

- A `Runs` struct in `picker/runs.rs` owns those fields and the state
  machine. Each delegate owns one.
- The `Run` → wait mapping stays in each delegate's `update_matches`. Quick
  open waits `NOTES_CHANGED_REFRESH` on `Run::Refresh` and keeps its
  `NotesChanged` subscription. Vault search never subscribes to
  `NotesChanged`; its `Run::Refresh` arm waits nothing, as `Run::Notify`,
  never `unreachable!`.
- The excerpt machinery (`rows_shown`, `ask_excerpts`) stays in quick open.
  A vault-search hit carries its `context`, so its rows are built when the
  results land.
- **The move is its own commit, with no behavior change.** The quick-open
  tests do not change and are its oracle. `./scripts/test-names.sh` is diffed
  before and after.

**As built (slice 1).** `Runs` is private state with a small API: `start`
returns a `Started` (`id: RunId`, `cancel`, `run`, `behind_user`),
`scanning(&started)` marks that run's scan in flight, `is_current(RunId)`
checks a result, `confirmed` drops a pending re-run. The scan in flight is
`Option<Scan { user, rerun }>`. The parts that must call back into the `Picker`
(`refresh_or_defer`, `landed`, `observe`) are generic over
`D: PickerDelegate + AsMut<Runs>`: a std trait, so no one-implementer trait.
The tests changed only where a moved field is read, through `delegate.runs`.

Amended 2026-09-29: `Runs` now owns the whole run through the `IndexDelegate`
trait (`Runs::update_matches`, `Runs::apply`); `AsMut<Runs>` and `confirmed`
are gone, replaced by `Picker::confirmed`. See [`2026-09-29-query-run-design.md`](2026-09-29-query-run-design.md).

## S2. `inkra-index`: the line and the per-note cap

### `Hit.line`

`Hit` gains `line: u32`: the 1-based line of the hit's start in the saved
text. It is a logical line, not a wrapped row. The corpus is LF-only:
`normalize_line_endings` runs on every load, and a save writes the `Buffer`'s
LF text. So the count of `\n` before the start, plus one, is the line.

- `locate` stays as it is. A second pass over its returned ranges counts `\n`
  between consecutive starts.
- **Corrected.** The draft counted during the walk. `light` may still extend
  the last range when a later word merges into it (`find/highlight.rs:67-72`),
  so a range is final only once `locate` returns.
- **As built.** The pass reads no `cancel`. It rereads a prefix `locate`
  already read under `cancel`, at +1.2 ms at worst over 10 MB; `search`
  still reads `cancel` between notes.
- Cost: O(bytes up to the last hit) of one note, a byte scan. It adds to the
  10 MB late-hit case, which is already over budget (48.7 ms against 16,
  word-start spec, Budget). `cargo bench -p inkra-index --bench search` runs
  before and after, and the word-start spec's table records the new figure.
- The "two boxes agree" oracle also checks each `line` against a naive count
  of `\n` in `text[..start]`.

### `Searched::capped`

A header shows how many hits its note gave. A note cut short must say so.

- **Corrected.** The draft showed `100+` when a note gave `HITS_PER_NOTE`
  hits. `Searched` has no per-note flag, so a note with exactly 100 hits
  would show `100+`. A note can also be cut at fewer than 100, by the global
  limit: `room` is `HITS_PER_NOTE.min(limit - hits.len())` (`run.rs:53`), and
  `Searched::merge` truncates a second time, across ranges.
- `Searched` gains `capped: Vec<EntryId>`, in vault order.
  - `search` pushes a note when its walk stopped past `room`
    (`lit.len() > room` before the truncate).
  - `merge` pushes the note of the last kept hit when the dropped hit that
    follows is of the same note, and keeps each range's `capped` notes that
    still have a kept hit.
- The oracle: `merge` over any split still equals one range, `capped`
  included.
- Invariant, stated at `merge`: the ranges of `chunks` are disjoint and
  contiguous, so the hits of one note never span two ranges.

### As built (slice 2)

- **Hits are grouped by note.** `Searched { notes: Vec<NoteHits>,
  limit_reached }`, and `NoteHits { note, hits, capped: bool }`, never
  empty, in vault order. `Hit` loses `note`: a hit's note is its group's.
  The flat `capped: Vec<EntryId>` needed a `retain`, an `extend` and a
  `dedup` in `merge` to stay true; a flag on the group holds by
  construction.
- `merge` concatenates the groups in range order and keeps hits down to
  `limit`. The last kept group is truncated, and `capped` is set when any
  hit of it was dropped. `limit_reached` keeps its rule: `limit` hits kept.
- `Hit.line` is a `usize`: no saturation branch.
- `locate` returns up to `room + 1` ranges; `search` sets `capped` from
  `lit.len() > room`, then truncates.
- `lines` takes the final ranges by value and pairs each with its line. It
  counts with `str::matches('\n')`: a byte filter-count fails clippy's
  `naive_bytecount`, and `bytecount` would be a new dependency.
- The late-hit case: 49.8 ms before, 51.0 ms after (+1.2 ms, the line
  count over 10 MB), with the cancelling block walk. Measured A/B,
  interleaved twice on the same machine state, since the load drifted by
  60 % between two full runs. The grouped shape without the block walk:
  50.4 ms against 50.9 ms for the first build, same method.

## S3. The delegate: `vault_search/`

Modelled on `quick_open/`: `mod.rs` (the delegate), `search.rs` (the
background part), `rows.rs` (built rows), `render.rs`, and tests in their own
files. As built, the background part is `index::fan_out`, shared with quick
open, so neither has a `search.rs`.

**Per keystroke.** One `lookup`, then `search` over each range of
`chunks(num_cpus)`, then `Searched::merge`, limit 1,000. All of it runs off
the foreground, in `index::fan_out`, shared with quick open. A field whose
text has no term (`""`, `"   "`, `"[["`) runs no lookup. `search` cannot check that, since the
caller takes the lookup, so a delegate test does (word-start spec).

**A changing vault.** The delegate observes `IndexSync` through `IndexWatch`:
at once when built, throttled by `BUILDING_REFRESH` while loads land. A
refresh keeps the selected hit, keyed by `(note, range.start)`; else the first
hit is selected and scrolled to. A refresh never cancels a scan in flight
(S1).

**Rows.** Built once, when the results land, never per frame: `SharedString`s
and highlight runs. Every row is one `UI_LINE_HEIGHT` tall, so
`uniform_list` measures from row 0.

- **Header**, not selectable: the note's name, its folder dimmed, then its hit
  count, or `{n}+` when its `NoteHits` is `capped`. The path comes from
  the vault's entry on landing, as the backlinks panel reads it. A hit whose
  note has no entry any more is dropped with its header.
- **Hit**: the line number, dimmed, in a column as wide as the largest listed
  line number, then `context` with `context_match` in `theme::match_text`.
  Control chars draw as spaces (`one_line`), one byte for one, so the
  highlight stays valid.

**States.** D9's table, plus the first row below:

| State | Shows |
|---|---|
| Before the first build | "Scanning the vault…", as quick open |
| Empty query | nothing; placeholder "Search notes…" |
| Text, no term, or no match | "No matches" |
| No match while indexing | "No matches in 12,340 of 50,000 notes indexed so far" |
| Indexing, with matches | footer "Indexing 12,340 of 50,000 notes" |
| `limit_reached` | footer "1,000+ results; refine the query" |

**Labels.** The dialog is "Search notes". A header's `row_label` is
"`name`, `N` matches"; a hit's is "line `N`: `context`".

### As built (slice 3)

- **`HitActivated` from this slice.** The delegate emits `HitActivated
  { note, range }`, and the shell maps it to `show_note(note)` for now. Slice
  4 then changes only the shell's handler; with `NoteActivated` it would also
  change the event, the delegate's `confirm` and its tests. The shell's
  `toggle_picker_with` maps a picker's event for both pickers.
- **The empty text is read when drawn.** `PickerDelegate::empty_text(cx)`
  returns `Option<SharedString>`, since "No matches in …" is not static. The
  delegate stores what the rows answer (`Answered`: blank, scanning, no
  term, a query), and "No matches in …" reads the progress live, as the
  footer does.
- **The footer.** `limit_reached` wins over "Indexing …": more loads can only
  add hits past the limit. No rows, no footer: "No matches in …" already
  says how far indexing is. The indexing footer is
  `IndexSync::indexing_footer`, for both pickers, so `picker/` holds no footer text.
- **Shared with quick open.** `SEARCH_DEBOUNCE`, `Runs::settle` (a run that
  scans nothing), the test hold on scans (`Runs::scanning` returns what a scan
  awaits), and `index::fan_out`: the lookup, one scan per core-sized range,
  the merge and the cancel check, generic over the scan and the merge.
  Amended 2026-09-29: `index::fan_out` is gone; `IndexSnapshot::search` and
  `find_notes` run the lookup, the split and the merge inside `inkra-index`,
  and `Runs` drives the rest. See [`2026-09-29-query-run-design.md`](2026-09-29-query-run-design.md).
- **Revealing a selection.** The picker scrolls the unselectable rows right
  before the selected one into view with it, so `up` onto a note's first hit
  shows its header too.
- **Rows.** A header's label says "1 match" for one uncut hit. A hit's
  context drops its leading whitespace, capped at the match's start, so an
  indented line does not push the match right. Hit rows are inset (`pl_4`)
  under their header, whatever the width of the line column.
- **Groups.** Each header after the first has a hairline above it
  (`theme::group_rule`), drawn absolute so every row keeps one height for
  `uniform_list`. Zed inserts an `Entry::Separator` row instead; a row of
  another height would break `uniform_list`'s measure from row 0.
- **The line column** is `digits × theme::ui_digit_width()`
  (`0.6 × UI_TEXT_SIZE`), right-aligned, as Zed's `ilog10 + 1` × 0.5 rem.
  Digits are proportional in the UI font, so the column aligns right edges,
  not digits. The `vault-search-results` shot shows 998 to 1001 unclipped.
- **Measured.** `Results::new` builds 1,010 rows in 0.35 ms (release, M-series),
  far under the 8 ms foreground turn.
- **Known costs, by design.**
  - Each chunk runs with the full limit: `merge` over any split equals one
    range only if any one chunk may hold all the first 1,000 hits. So a
    common word builds up to `num_cpus × 1,000` contexts off the foreground
    before the merge keeps 1,000. Quick open does the same with 100.
  - No `NotesChanged` (S1): while loads land, a trashed note's rows stay up
    to `BUILDING_REFRESH`; once built, the refresh is at once.
  - Leaving the scanning state takes the first build's notify, so the first
    rows can wait up to `BUILDING_REFRESH` while loads follow the build.

## S4. The shell and the editor

**The key.** `cmd-shift-f` → `SearchVault`, in the `AppShell` context, as
`cmd-p`. It goes through `toggle_picker`: it replaces quick open, a second
press dismisses it, and it does nothing when no vault is open.

**Opening a hit.** The delegate emits `HitActivated { note, range }`. The
shell's `show_hit` follows D10:

- The note is showing: select the range and reveal it, without rebinding,
  as `show_note` avoids reopening. The pending activation of any other note
  is dropped: the pick is the last word (D8).
- Else `open_note(id, Some(range))`. Inside the activation's `update_in`,
  right after `open_buffer`, the editor selects the range. So the first
  layout is shaped with the selection, and its conceals reveal around it.
- A superseded activation is a dropped `Task`, so it applies no stale range.

**The editor.** `move_cursor` is `pub(super)` (`editor/motion.rs:22`), so the
shell cannot call it.

- A new `pub(crate) EditorView::select_range(range, window, cx)` clips both
  ends with `Buffer::clip` and selects through `move_cursor`. The saved
  offsets may not fit a note with unsaved edits; the clip keeps them valid.
- The shell calls it as `editor.update(cx, |editor, cx|
  editor.select_range(range, window, cx))`, with the `window` of its own
  `update_in`. **Corrected:** the draft implied a nested `update_in`, which
  does not compile: `Context` does not implement `VisualContext`.
- `EditorView` gains `pending_reveal`. `open_buffer` clears `last_text_top`,
  so `reveal_cursor` cannot run before the first paint. The flag is consumed
  after the first paint with a non-zero viewport: it calls `reveal_cursor`
  and notifies. The note shows at its top for one frame, then scrolls (D10).
  Superseded by tabs S0: the request resolves in `TextElement::prepaint`.

### As built (slice 4)

- **No `show_hit`.** `show_note` and `open_note` take `at:
  Option<Range<ByteOffset>>`; every other caller passes `None`. A
  `show_hit` would repeat `show_note`'s showing test, its dropped activation
  and its focus move.
- **The note showing.** `select_range` runs through `cx.with_window`, before
  the focus move and whether or not a picker is open: `HitActivated` is
  delivered before the picker's `Dismissed`.
- **`select_range`** snaps the start back and the end forward to grapheme
  boundaries (`Buffer::clip_grapheme`, which clips first), then selects
  through the `move_cursor` gate. The head, where the caret draws, is the
  range's end. Selecting before the first layout shapes that layout with the
  selection's elements revealed.
- **A hit lands centred**, as Zed's project search jumps with
  `Autoscroll::center()`. A caret move keeps the minimal reveal with its
  three-line margin. The strategy is data: `Reveal::Minimal` or
  `Reveal::Center`, carried by the request.
- **`pending_reveal` is not a flag.** The reload's `restore_top` becomes
  `scroll_to: Option<ScrollTo>`, `Top(TopLine)` or `Cursor(Reveal)`: one
  slot for a scroll decided before its geometry exists, in `editor/scroll.rs`.
  `restore_scroll` takes it in `render`, reshapes first (a reload may have
  landed since the last paint shaped `last_layout`), then applies it.
  `reveal_cursor` asks for `Cursor` itself when it has no geometry (no paint
  yet, or a zero viewport), so no caller knows about the first paint.
- **`request_scroll`** is the one way in, for a reveal and a reload alike. A
  reveal replaces whatever waits: the user asked for it after the reload. A
  reload's top line only fills an empty slot: a waiting one is already in the
  text the next layout describes, and a waiting reveal wins (`map_through`
  already moved the caret). While any request waits, the layout may predate
  the text, so `reveal_cursor` does not scroll at once: it requests too.
  Superseded by `2026-09-29-editor-geometry-design.md`: `scroll.rs` is now
  `viewport.rs`, `scroll_to` is now `Viewport`, `request_scroll` is
  `Viewport::request`.
- **The frame after the first paint.** `paint` records `last_text_top`; when
  that is the first since `open_buffer` and a `scroll_to` waits, it defers a
  notify with `defer_in`. A notify during a draw marks no frame
  (`invalidate_view` sets `dirty` only outside a draw phase). Only the first
  paint asks, so a zero viewport waits for the next render, with no redraw
  loop. Superseded by tabs S0: no notify from a draw; prepaint resolves.
- **Render, not prepaint.** Zed autoscrolls in `EditorElement::prepaint`
  because its element owns its scroll. Here the parent `div` owns it and has
  already offset its children when `TextElement::prepaint` runs, so a reveal
  there also lands a frame late. `restore_scroll` already applied a reload's
  scroll in `render`, before prepaint reads the offset; the reveal joins it.
  Superseded by tabs S0: prepaint resolves and shifts the element's own paint.
- **Shot.** `vault-search-opened`: `step`, first found on line 998 of a
  1,130-line note, selected on the centred line.
- **Known limit.** A hit carries offsets in the saved text. In a note with
  unsaved edits it may select shifted text; the clip only keeps it valid.
  Searching unsaved buffers is deferred (search spec).
- **Deferred.** The note shows at its top for one frame, then scrolls, and
  the first paint defers a notify for it. A `TextElement` that owns its
  vertical scroll, as Zed's `EditorElement` does, would reveal in prepaint
  and remove both. Recorded in `docs/roadmap.md`, Editor. Superseded by
  tabs S0, which removes both without the element owning its scroll.

## Tests

Headless `#[gpui::test]` against `MemFs`, each key with
`simulate_keystrokes`:

- `cmd-shift-f` opens the picker; a second press dismisses it; it replaces
  an open quick open; it does nothing without a vault.
- `up` and `down` skip headers and stop at the ends.
- `enter` opens the selected hit; `escape` dismisses and restores focus;
  `enter` typed before the results land confirms the first hit.
- A stale result is dropped. With `num_cpus` overridden, the merged result
  equals one range.
- A blank or `[[` query runs no lookup.
- A hit far down a long note opens with its range selected and the caret
  visible. A hit in the note showing selects without rebinding: the
  editor keeps the same `Buffer` entity and its undo stack.
- Trashing a listed note removes its header and hits.
- A refresh during loads keeps the selected hit.
- `inkra-index`: `line` and `capped` in the oracles (S2); `cargo mutants` on
  `index` after S2.

**Shots**, each looked at: `vault-search-results` (headers, line numbers, a
`{n}+` header), `vault-search-empty`, `vault-search-no-match`,
`vault-search-indexing`, `vault-search-limit`, `vault-search-scanning`, and
`vault-search-opened`, the editor after a hit far down a note opens.

## Slices

Each merges alone with `check.sh` green.

1. **`Runs`**: the move of S1, no behavior change.
2. **The index**: `Hit.line`, `NoteHits::capped`, the oracles, the bench
   figure.
3. **The picker**: the delegate, rows, states, `cmd-shift-f`, the key tests,
   the shots. Confirming a hit opens its note, at the top.
4. **Opening at the hit**: `HitActivated`, `show_hit`, `open_note` with a
   range, `select_range`, `pending_reveal`, their tests, the
   `vault-search-opened` shot. As built: no `show_hit` and no
   `pending_reveal`; see S4.

## Deviations from Zed's Text Finder

`vendor/zed-reference/crates/search/src/text_finder/` is the reference. Not
taken, for a note app in v1:

- Collapsible headers.
- Streaming results: the search spec's Evolution row "Streaming results".
- A preview of the hit.
- Multi-select and its checkboxes.
- A header's file icon and disclosure chevron, and the separator row between
  notes (slice 3): a hairline above each header marks each group.

## Amendments to the search spec

- **D9**: the table gains "Before the first build: Scanning the vault…" for
  both pickers. A capped note's header shows `{n}+` from `NoteHits::capped`.
- **D1 / the word-start spec's S2**: `Hit` gains `line` and loses `note`;
  `Searched` lists `NoteHits` groups, each with `capped`.
- **Slices**: slice 6 is split as above.
- **Roadmap**: when slice 4 ships, `docs/roadmap.md` marks vault search as
  shipped, and CLAUDE.md drops "(vault search not yet built)".
