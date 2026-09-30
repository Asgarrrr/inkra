# Tabs

Status: **vetted 2026-09-29** by an advisor (prior art) and a code-grounded
critic. Each slice gets its own plan, red/blue reviewed before code.

## Why

Inkra shows one note at a time. Reading a note beside the one being written
means leaving it, and following a chain of links cannot be walked back. Tabs
are the "panes" of the product description, one pane first. The modular
architecture design places them in `app` (Where future work lands).

## What the user decided

Taken in conversation on 2026-09-29, one question at a time:

1. **A tab is a slot with its own history**, as in Obsidian. Opening a note —
   from the sidebar, a link, backlinks, `⌘P` or `⌘⇧F` — replaces the active
   tab's note and pushes the old one on that tab's back stack. A new tab opens
   only on request. Zed's model (every open item is a tab, plus a preview tab)
   was rejected: an IDE model, and tabs pile up while following links.
2. **Tabs do not survive a restart.** A launch opens one tab, as today. A tab's
   state is plain data so restoring it later is a serialization, not a rewrite.
3. **The same note may show in two tabs**, each with its own caret and scroll.
   History makes it unavoidable: two tabs' back stacks can both hold a note.
4. **A tab may be empty.** `⌘T` opens an empty tab showing today's empty state.
   Closing the last tab leaves one empty tab: there is always at least one tab.
   After a close, the most recently used remaining tab activates (Zed's default).
5. **The tab bar spans the note column only**, between the sidebar and the
   backlinks column, which keep their full height. Visual design comes later;
   this spec fixes the structure, not the look.
6. **In scope besides the core:** reopen a closed tab (`⌘⇧T`) and reorder by
   drag. Out: splits, pinned tabs, restore on launch, window title.
7. **One `EditorView`** at most, rebound on a tab switch (A3).

## Keys

| Action | Key |
|---|---|
| New empty tab | `⌘T` |
| Close tab | `⌘W`, the tab's ×, middle click |
| Next / previous tab | `⌃Tab` / `⌃⇧Tab`, `⌘⇧]` / `⌘⇧[` |
| Tab N / last tab | `⌘1`…`⌘8` / `⌘9` |
| Back / forward in the tab's history | `⌘[` / `⌘]` |
| Reopen closed tab | `⌘⇧T` |
| Open in a new tab: sidebar, backlinks | `⌘`-click |
| Open in a new tab: `⌘P`, `⌘⇧F` | `⌘↵` |
| Follow a link in a new tab | `⌘⇧`-click, `⌘⇧↵` |

`⌘`-click and `⌘↵` already follow a link in place; the new-tab variant adds `⇧`
rather than moving a shipped gesture. A new tab opens in the foreground: one
rule for every gesture.

A tab key pressed while a picker is open dismisses the picker first, then acts.

## What Zed does

`vendor/zed-reference/crates/workspace/src/pane.rs` (`P`):

- `Pane` holds `items`, `active_item_index`, and `activation_history`, an MRU
  keyed by entity id (`P:399-450`, `update_history` `P:1518`).
- A new item inserts right after the active one (`P:1265-1274`).
- Every close goes through `close_items(predicate)`; `_remove_item`
  (`P:2135-2244`) picks the next active item from the MRU, else the left
  neighbour.
- `NavHistory` is per pane: back, forward and closed stacks, 1024 entries
  each (`P:323`, `P:472-485`). `navigate_history_impl`
  (`workspace.rs:3162-3204`) pops until an entry differs from the item showing.
- The tab bar is an `h_flex` with `overflow_x_scroll` and a tracked scroll
  handle, scrolled to the active tab on activation (`P:3685-3695`, `P:1510`).
  Reorder is `on_drag`/`drag_over`/`on_drop` per tab (`P:2953-2995`).

VS Code reuses one editor per group and saves view state when its input
changes (`editorWithViewState.ts`): prior art for decision 7. Its view state is
Monaco's `firstPosition` plus `firstPositionDeltaTop`, the shape of our
`TopLine`.

## Deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| A tab is an item (a view) | A tab is a slot holding a `Place` and a history | Decision 1 |
| History per pane | History per tab | Obsidian's model; VS Code users asked for per-group history (vscode#45233) |
| One view entity per item | At most one `EditorView`, rebound | A3 |
| Preview tabs, pinned tabs, `max_tabs` | None | Replacement already keeps the bar short |
| Close activates by MRU, setting for neighbour | MRU only | No setting until someone asks |
| Back pops within an item too | History is note-granular | Deferred, with its trigger |

## Architecture

### A1. The model: `inkra-tabs`

A new crate, `inkra-tabs`, holds `Tabs<P>`. It depends on no workspace crate
and not on gpui, reads no clock and no file, and is generic over `P`, the
place a tab shows. It is a crate rather than a module so `cargo mutants`
covers it and its tests need no `Window`. It joins the gpui isolation check,
the fmt lists in `check.sh` and CLAUDE.md, and `[workspace.dependencies]`.

Tabs have stable `TabId`s; indices shift on close and drag. The MRU, a
pending load and the closed stack refer to ids.

Every operation that leaves a tab takes the leaving tab's current place as an
argument, so a stale place never enters a stack. The model returns the place
to show; `app` applies it.

Invariants, each a proptest over random operation sequences:

1. There is at least one tab; closing the last tab leaves one empty tab.
2. Tab ids are unique; the active id is a tab.
3. The MRU is a permutation of the tab ids, the active id first. A reorder
   changes neither the MRU nor the active id.
4. Back pops until an entry's note differs from the note showing. Adjacent
   duplicates never stay visible to the user.
5. An empty place is never pushed on a back or forward stack.
6. Each back, forward and closed stack holds at most 1024 entries; the oldest
   drops first.
7. A closed tab with no note and no history is not pushed on the closed stack.
8. `⌘⇧T` reinserts a tab at its old index, clamped to the bar.

### A2. A place

`app` supplies `P`, plain data:

- `Place { note: Option<EntryId>, view: Option<ViewState> }`. `note: None` is an
  empty tab.
- `ViewState { selection: Selection, top: TopLine }`. Not `View`: in gpui a
  view is a rendering entity. `Selection` keeps its
  direction and goal column (`inkra-editor`); `TopLine { offset, below }` is
  the viewport's own shape and survives soft wrap.
- Restoring clips every offset with `clip_grapheme`.

**The active tab's view lives in the `EditorView`.** It is read into the tab
when the tab is left. If a scroll request is still waiting (two switches
inside one frame), the capture reads the request, not the last drawn frame.

**Parked anchors (S2).** An inactive tab showing a note keeps its view as
`Buffer` anchors, so an edit to that note in another tab, an undo or a reload
moves them with the text. Anchors are not in `P`: `app` keeps one table,
`TabId → (Entity<Buffer>, anchors)`, and `park` and `unpark` are its only
writers. Unpark resolves, forgets, and yields a plain `ViewState`. On
`NoteClosed` the entry drops without forgetting: the buffer is gone. A place
entering a back, forward or closed stack is plain offsets, so anchors stay
bounded by the number of tabs. Biases: the selection as
`EditorState::map_through`, the top line `Left`.

Each park adds anchor slots `Buffer` never reuses, and every edit walks them.
That is a few slots per switch for the session's lifetime: harmless at human
scale, and recorded as Deferred.

### A3. At most one `EditorView`

**Switching to a tab whose note is loaded is synchronous.** One update
captures the leaving view, rebinds the editor (`open_buffer`) and restores the
arriving view, calls `set_current`, and moves focus. It drops `activation`.
Two tabs on the same note skip the rebind and only restore the view.

**Opening a note not yet loaded stays async**, and the model commits when the
buffer lands, keyed by the `TabId`. A load that resolves `None` leaves the tab
as it was. Every tab operation drops `activation`.

**The first frame after a switch shows the restored top line.** Today a
scroll request needs a drawn layout, so a rebind would paint the note's top
for one frame (vault-search spec, As built, slice 4). S0 resolves the request
in the element's prepaint instead: the roadmap item "`TextElement` owns its
vertical scroll", whose trigger this feature fires.

**An empty active tab has no editor**, as after a trash today: the shell's
root keeps focus, so `⌘T`, `⌘P`, `⌘W` still reach it. `NoteStore` gains
`clear_current`, which schedules the save of the note left, so `current`
stays the note on screen.

**Switching seals the undo group**: capture calls `finalize_transaction`, so
typing on both sides of `⌃Tab` on the same note is two undo steps. This is an
exception to CLAUDE.md's "neither side when it edits nothing", written there.
The undo stack is per note, shared by the tabs showing it.

A switch drops an IME composition and keeps the composed text, as `rebind`
does today and as Zed does on blur. A test pins it.

`showing` becomes the active tab's note; `set_showing` stays its one writer,
so backlinks and the quick-open MRU follow it.

### A4. Events that touch tabs

- **`NoteClosed` (trash of a note):** every tab showing it becomes empty and
  keeps its own history; entries for it leave every stack, and adjacent
  duplicates collapse. A closed-stack entry left with no note and no history
  drops. Obsidian closed a deleted note's tab in 1.3 and reverted it in 1.3.4
  after users lost their history.
- **Trash of a folder** emits `NoteClosed` for the folder only; its open notes
  become gone and show the banner.
- **Gone on disk:** no change; the note stays open under its banner.
- **Rename:** `EntryId` is stable. The shell observes the vault so tab titles
  follow a rename.

### A5. The shell and the tab bar

`shell.rs` is 363 lines; S0 splits it by concern before tabs land:
`shell/activation.rs` (opening, `show_note`, `set_showing`), `shell/render.rs`,
then `shell/tabs.rs` (tab actions, capture, restore) and `shell/tab_bar.rs`.

The tab bar is a row above the note column: one tab per entry, the note's
name ("New tab" when empty; the file name in single-file mode), and a close
button. The active tab is highlighted; the row scrolls horizontally and keeps
the active tab in view. Colours and sizes come from `theme.rs`. The backlinks
column shows whenever a vault is open, empty for an empty tab, so the bar does
not jump 260 px on a switch. The launch seeds the first tab with the note it
opened, if any.

Every opener carries a target: `NoteActivated`, `HitActivated` and
`LinkFollowed` gain `open: Open { Here, NewTab }`, and `NoteActivated` moves
out of `sidebar` to a neutral module (S3). The sidebar opens through
`show_note`, so clicking the note showing keeps its view and pushes nothing.

## Slices

Each slice is vertical: data, behaviour, tests, a rendered scene where it draws.

| Slice | Delivers |
|---|---|
| S0 | `shell.rs` split (move-only commit, `test-names.sh`); `EditorView::view_state` and `open_buffer(buffer, Option<ViewState>)`; the scroll request resolved in prepaint, so a restored top line is right on the first frame; editor tests: restore after rebind, a wrapped top row, a reversed selection, IME dropped |
| S1 | `inkra-tabs` with `Tabs<P>`: ids, open/activate/close, MRU, invariants 1–3 and 8's index rule as proptests; synchronous switch, async open committed on landing; `clear_current`; `⌘T`, `⌘W`, next/previous, `⌘1`–`⌘9`; views stored as plain offsets; trash empties tabs; tab bar with click, ×, titles following renames; picker dismissed on tab keys; a `tabs` shot scene |
| S2 | Parked anchors, `park`/`unpark` as the only writers; tests: edit the same note in another tab, reload a parked note, trash while parked |
| S3 | `Open` on the three events; `⌘`-click, `⌘↵`, `⌘⇧`-click, `⌘⇧↵`; middle click closes |
| S4 | Per-tab history: `⌘[`, `⌘]`, invariants 4–6, trash pruning; the sidebar through `show_note` |
| S5 | `⌘⇧T` with history, invariant 7 |
| S6 | Drag to reorder |

Each slice: an Opus plan, red then blue review of the plan, Opus implements,
then red/blue on the diff, `/code-review`, a reviewer against this spec, and an
Opus quality pass, then `./scripts/check.sh` and a render.

## As built (S0)

Plan: `docs/superpowers/plans/2026-09-29-tabs-s0.md`. Built as planned, with
these deviations:

- **Names after review.** `View` is `ViewState` and `view()` is
  `view_state()`, because gpui already uses "view" for a rendering entity.
  `Viewport::top` is `Viewport::capture`.
- **The `expect(dead_code)` tripwire is on `EditorView::view_state` only.**
  `ViewState::clipped` constructs a `ViewState` in production code, so
  `ViewState` is live. rustc treats an `expect`ed item as a live root, so
  `Viewport::capture` is live through `view_state`. On either, the expectation
  was unfulfilled and failed the build. S1 calling `view_state` still trips
  the one that is left.
- **`Viewport::resolve` returns how far it moved the offset.** The `div`
  already placed this frame with the old offset. The element shifts its own
  paint origin by the pixel-snapped difference, so this frame paints where the
  next one will.
- **`restore_top` bounds `below` by the target row's height in this frame's
  layout.** A row that got shorter would otherwise overshoot into the next
  row, for example a 44 px heading row restored as a 28 px body row. The
  worst case is the row's bottom at the viewport top, with the next row whole.
  A wrapped line that un-wraps cannot overshoot: `below` is measured within
  one sub-row, and every body row is the same height.
  `a_restored_top_line_stops_at_the_end_of_a_row_that_got_shorter` pins it.
- **P6 has no window-level test.** No window gives the scroll `div` zero
  height: its `p_4` padding keeps 32 px, and a resize to zero centred the hit
  in those 32 px. `a_reveal_with_no_viewport_height_waits_for_a_frame_with_one`
  resolves a `Viewport` that was never prepainted, then swaps it in. The next
  frame centres the hit. The test has no notify log, because its own notify
  would trip the log. The two first-paint tests pin "no notify from a draw".
- **The reload-restore test applies its diff after `open_buffer`**, in the
  same update. Before `open_buffer`, the rebind's own `refresh_markdown`
  reads the new text, so `render`'s call would go untested. Thirty headings
  above the top line make stale spans visible.
- **`Viewport::apply` takes `&ScrollTo`.** It is private now, and clippy's
  `needless_pass_by_value` fires on a private function.
- **The wrapped-row test scrolls whole pixels** (11 px into row 61). A
  fractional offset comes back pixel-snapped, so a round trip would differ by
  the snap.

A mutation check confirmed that two tests can fail. `render` without
`refresh_markdown` fails the reload-restore test. `paint` with the unshifted
bounds fails the click test and three painted-top assertions.

Levers for later, each with its trigger:

| Lever | Trigger |
|---|---|
| Make an anchor the scroll's source of truth (`TopLine`, as Zed's `ScrollAnchor`), not the `div`'s pixel offset. This removes `text_top`, the prepaint shift and the only-child invariant (roadmap, Editor) | A resize shifting the text under the viewport is reported, or virtualization needs the visible rows before layout |
| `Selection::map_offsets` in `inkra-editor`, shared by `ViewState::clipped` and `map_through` | A third place maps every offset of a selection |
| A clipped-selection type for `rebind`'s precondition | S1 adds a second caller of `rebind` |
| Notify on reload, so `render` stops calling `refresh_markdown` | A profile shows `render`'s version check, or a second view reads the spans outside `render` |

**For S1 and S2.** `open_buffer` clips once, synchronously. A write that
bypasses `edit_buffer` before the first prepaint is not clipped again. No
path reaches this today.

## Risks

- **Prepaint scroll (S0)** moves `Viewport` into the element's prepaint, the
  largest change here. If it proves larger than the slice, the fallback is an
  accepted one-frame flash, recorded, not silent.
- **The activation task.** A landing load writes into its own `TabId` only.
- **Anchor lifecycle (S2).** `Anchor` is a bare index: resolved against
  another buffer it gives a wrong offset, not a panic. The table keys the
  buffer with its anchors so they cannot be separated.

## Deferred

| Item | Trigger |
|---|---|
| Restore tabs on launch | A user asks, or a crash loses a working set |
| Splits | A user wants two notes side by side |
| Back and forward within one note (a link or search hit in the note showing) | A user reports back skipping a jump, as Obsidian users did |
| Window title | A window switcher or screen reader user asks |
| Close others / close to the right, pinned tabs | The tab bar is regularly crowded |
| Anchor slot reuse in `Buffer` | A profile shows `shift_anchors` in an edit |
| Evicting a note no tab or history references from `NoteStore` | Memory measured to grow with notes visited |
