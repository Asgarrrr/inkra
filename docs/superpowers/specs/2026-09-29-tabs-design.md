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
| Back and forward buttons only | A right-click or a hold on either arrow lists the tab's history | A note several steps back is one gesture away, as in a browser's back menu (Obsidian chrome D4) |

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
9. A walk of n steps arrives where n walks would, captures once, and pushes
   the places it passes as stored. A stop no longer at its steps, or no longer
   on its note, walks nothing and calls no capture.

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

## As built (S1)

Plan: `docs/superpowers/plans/2026-09-29-tabs-s1.md`. Built as planned, with
these deviations and facts:

- **`⌘}` / `⌘{`, not `⌘⇧]` / `⌘⇧[`.** macOS clears `shift` for a non-letter
  key and reports the shifted character (`gpui_macos/src/events.rs:470-479`).
  A `cmd-shift-]` binding passes `simulate_keystrokes` and never fires on
  hardware. `⌃Tab` / `⌃⇧Tab` work on every layout.
- **`leaving` is a closure.** `Tabs` calls it from one private place,
  `leave_active`, once per real leave: `insert`, `activate` of another tab,
  and `close` or `commit` of the active tab. A close or a commit of an
  inactive id never calls it, because the app's closure seals an undo group.
- **The model's shapes.** `close` returns `Closed { place, index, shown }`;
  `step` takes a `Direction`. Closing the last tab leaves one empty tab with a
  fresh id, so a load keyed by the old id lands nowhere.
- **Growth for S4 and S5.** `Tab<P>` gains private `back` and `forward` stacks.
  `commit` pushes the `leaving` value on `back`. `back` and `forward` return
  `Option<P>` like `activate`. `close` pushes its `Closed` on a closed stack,
  and `reopen` inserts at `Closed::index`. S4 adds a `Place` trait bound with
  two implementers, the app's `Place` and the proptest fake.
- **T4 keeps `activation` in two cases.** Closing an inactive tab and `⌘N` on
  the active tab change nothing on screen, so a pending load still lands.
  Every other tab operation drops it: a pending load is cancelled by any
  switch, new tab or close of the active tab. The red team noted this as UX;
  it is intended.
- **One field binds the editor to its note.** `AppShell.bound:
  Option<Bound { note, editor }>` replaces `editor` and `showing`, so an editor
  with no note (T2) cannot be written. `showing()` reads it; `set_showing`
  stays its one writer and keeps the side effects (recent, backlinks).
  `drop_editor` became `unfocus_editor`: it moves focus off the editor, and
  `set_showing(None)` drops it.
- **Same-note binds restore only.** A sidebar click on the note showing still
  goes through `open_note`. It now keeps the caret and the scroll, where the
  old `open_buffer(None)` reset them.
- **The launch seeds the first tab** with `NoteStore::current()`. A file launch
  therefore starts with `showing() == Some(DETACHED)`, not `None`.
- **Invariant panics.** `expect_used` is denied workspace-wide, so
  `active_index` and `last` use `unwrap_or_else(|| panic!(..))` with a
  `# Panics` doc naming the invariant.
- **Smaller deviations.** `Tabs`' private list is `list` (clippy's
  `struct_field_names`). `check` lives in the model's `tests.rs`. Each action
  landed with its handler; the `⌘1`–`⌘8` handler is `activate_nth_tab`.
  `tab_scroll` landed with the bar. The `tabs` scene shows four tabs, not
  three.
- **Test adjustments.** The inactive-close undo test types `x` after coming
  back: the plan's order seals `x` on the switch, so its assertion failed with
  correct code. Two tests prove "no reparse": a write with no notify before
  the switch, then `parsed_at` must not move (a `cfg(test)` accessor). The
  keystroke test in `shell/tests/activation.rs` now adopts its note, since an
  editor with no note is no longer representable.
- **Stale views on one note** stay plain offsets until S2; returning to a tab
  whose note another tab emptied clips its caret to the end
  (`a_stale_view_in_another_tab_on_the_same_note_clips_on_return`).
- **Single-file mode has no backlinks column.** The column shows only while a
  vault is open. Before S1 a launched file showed it, always empty.
- **The active tab stays in the bar.** Every change to the tab set or the
  active tab ends with `keep_active_tab_visible`. A close to the left of the
  active tab shifts it too, so a switch alone is not enough. The test calls
  the × handler directly: a tab scrolled out of the bar cannot be clicked.
- **Known gap.** The sidebar highlight does not follow the active tab. This
  predates tabs; S4 fixes it when the sidebar opens through `show_note`.

Lever for S4:

| Lever | Trigger |
|---|---|
| `Tabs` keeps no place for the active tab: `leave_active` writes it, `activate` takes it. This removes T1's pair (`bound.note` and `active_place().note`) and the stale `view` of the active tab | S4 reshapes `commit` and `leave_active` for the back stack |

Done in S4 (As built, S4).

## As built (S2)

Plan: `docs/superpowers/plans/2026-09-29-tabs-s2.md`. Built as planned, with
these facts and deviations:

- **The table has three mutations.** `park` and `unpark` are its writers.
  `drop_note`, through the shell's `forget_note`, is the third: on
  `NoteClosed` it removes every entry of the note and forgets no anchor,
  because the buffer is gone. One note can be parked under several tabs, so it
  `retain`s, never a single `remove`.
- **The goal rule.** `unpark` returns the parked `Selection`, goal included,
  only when `Buffer::version` is the version at park and the resolved range is
  the parked range. The version alone is not enough: an undo can restore the
  version with an anchor moved. Otherwise the selection is what `map_through`
  gives, with `Goal::None`. `Selection::settled` is the piece the two share.
- **I2 holds.** `Tabs` stores no view for any tab: `park` returns
  `Place { note, view: None }`, and `commit` overwrites the value `leaving`
  wrote. The shell tests' `check` asserts I1 and I2 after every step.
- **The S5 hand-off.** Closing an inactive tab unparks it and discards the
  view. S5 passes that view to the closed-stack entry as plain offsets.
- **The replaced S1 test.**
  `a_stale_view_in_another_tab_on_the_same_note_clips_on_return` is gone.
  `a_parked_selection_partly_deleted_in_another_tab_keeps_its_remaining_text`
  replaces it, and fails on S1's code.
- **`forget_note` landed with the table (task 3), not with the reload tests
  (task 4).** Extending `check` with I1 turned two S1 trash tests red, so the
  commit that adds `check`'s I1 also adds `forget_note`. Task 4 adds only
  tests; its trash tests fail when `drop_note` is removed.
- **The two-tabs reload test uses different carets.** With one caret for both
  tabs, a tab that came back with no view kept the other tab's caret and
  passed. Line 50 in the active tab and line 120 in the parked one make both
  mappings visible.
- **The sequence proptest draws every char once.** The chars come from a
  shuffled pool of distinct punctuation and symbols, ASCII and multibyte, with
  one newline. Each char is then its own token, so `diff` reproduces the
  literal edit. `prop_assume!` stays as a guard: at most 5 rejects in 3000
  cases.
- **The task-2 commit carried two `expect(dead_code)` tripwires**, on
  `EditorView::park` and `ParkedView::unpark`, as S0 did. The shell calls both
  in task 3, which removes them.
- **Shared shell test helpers** (`check`, `select`, `text_of`, `tall_notes`,
  `scroll_to`, `trash`, `at`) moved to `shell/tests/mod.rs`.
- **Two tests guard S1 behaviour and were not red first.**
  `a_switch_with_no_edit_keeps_a_reversed_selection_and_its_goal` and
  `park_seals_the_undo_group` pass on S1's code. They fail if parking drops
  the goal or skips the undo seal.
- **After review.** `a_gone_then_restored_note_moves_its_parked_view` passed
  at once: a guard, and it fails when `arrive` drops the unparked view.
  `closing_the_last_tab_leaves_nothing_parked` is gone. The last tab is
  active, so it has no entry. The close's own `unpark` would also hide a leak
  from its `leaving` closure. So its second step could not fail, and
  `closing_the_active_tab_shows_the_next_tab_with_its_moved_caret` covers its
  first.
- **`cargo mutants -f` without `-p` is ignored** while `.cargo/mutants.toml`
  sets `examine_globs`: it listed all 1687 mutants. The editor run used
  `-F '^crates/editor/src/(parked|selection|state)\.rs:'`, then
  `-p inkra-editor` for the whole crate. The app run's `-p inkra-app -f` works
  as written.

The bench (`cargo bench -p inkra-text -- --quick`, not a gate): one insert
200 bytes before the end of a 10 KB document (233 lines).

| Case | Time |
|---|---|
| `buffer_insert_after_parks/0` | 272 ns |
| `buffer_insert_after_parks/1000` (3000 forgotten slots) | 977 ns |
| `buffer_insert_char/end/1000` (43 KB), for scale | 1.09 µs |

A thousand switches on one note add about 0.7 µs to a keystroke. "Anchor slot
reuse" stays Deferred.

## As built (S3)

Plan: `docs/superpowers/plans/2026-09-29-tabs-s3.md`. Built as planned, with
these facts and deviations:

- **The new tab is inserted when its note lands.** `open_in_new_tab` spawns
  the load into the one `activation` slot. The landing checks that the source
  tab is still active, then inserts `Place { note, view: None }` at
  `active_index() + 1`, parks the tab left through `insert`'s closure, binds,
  selects `at`, focuses and scrolls the tab into the bar, in one `update_in`.
  Opening into an empty tab first would drop the editor for a frame, and a
  second quick `⌘`-click's `show_place` would cancel the first open and leave
  its tab empty for good.
- **One pending open.** `Here` and `NewTab` share `activation`: the last pick
  wins, and every op that changes the active tab cancels it (T4). A trash that
  emptied the active tab once dropped `activation` too; the failed-trash
  change reversed that (As built, a failed trash keeps its tabs): a trash
  outcome keeps a pending open. `show_place` is `show` plus the focus;
  `NoteClosed` calls `display`, with no focus move, so a trash from the tree
  keeps the focus there. `AppShell::new`
  takes the `Window` for that `subscribe_in`, as Zed's views do. The landing's source check is backed by that drop, as
  `commit`'s is: removing it breaks no test.
- **A new tab on the note showing duplicates its view.** `bind`'s same-note
  branch keeps the editor's caret and top line, and the tab left parks the
  same view. Another note starts at its top.
- **`SecondaryConfirm`, after Zed's `menu::SecondaryConfirm`.** `⌘↵` in a
  picker is `picker::SecondaryConfirm`, bound in the `Picker` context.
  `PickerDelegate::confirm` takes an `Open`, and `confirm_on_update` is an
  `Option<Open>`. The palette ignores `open`: `⌘↵` runs the command.
- **`⌘`-click on a picker row opens in a new tab**, as Zed's row click passes
  `modifiers().secondary()`. This is an addition to the Keys table.
- **Where each click reads `⌘`.** A sidebar row, a backlink row and a link act
  at mouse-down and read the press's modifiers. A picker row acts at mouse-up
  and reads the release's. The plan placed backlinks at mouse-up; they already
  acted at mouse-down, and S3 keeps that.
- **No open lands on a note being trashed.** `NoteStore::open_note` returns
  `None` unless the lifecycle is `Open`, on the fast path and after a load.
  Before S3, a `Here` open landed on a `Trashing(Some(_))` note.
- **A sidebar click during a rename does nothing.** `Sidebar::activate`
  returns early while the name field is open, as `step` does. The field still
  loses the keys: the tree's tracked focus takes them on any press inside it.
- **Middle click** closes the tab under the pointer through `close_tab`. The ×
  has no aux listener, so a middle click on it closes its tab.
- **A palette sort key (after review).** With a note showing, `new tab` listed
  "Follow link in new tab" above "New tab", and `↩` ran it. The palette now
  sorts a match at the name's first word before a later match, between
  "recent" and "alphabetical" (command-palette spec, D3). A recent command
  still lists first.
- **`Open::from_click` reads `Modifiers::secondary`**, `⌘` on macOS, as Zed's
  row click does.
- **Test adjustments.** Four tests passed when first run, each a
  guard: a `⌘`-click on a folder that emits an open, a landing that does not
  focus, and `follow` dropping `open` each fail one. The tab-switch test fails
  only with both the drop and the source check removed.

## As built (S4)

Plan: `docs/superpowers/plans/2026-09-29-tabs-s4.md`, with its binding red/blue
section. Built as planned, with these facts and deviations:

- **The S1 lever landed first, as a pure refactor.** `Tabs` holds no place for
  the active tab (H8): `Tab::place()` is `None` exactly for it, and `showing()`
  is its note. `leave_active` stays the one caller of `leaving`. It returns
  the tab left and the place, and each op puts the place where it belongs: in
  the tab left (`insert`, `activate`), in `Closed.place`, on back (`commit`),
  or on the other stack (`back`, `forward`). `commit` lost its `place`
  argument. `forget` returns nothing; `forget_note` reads `showing()` first.
- **T1 is retired, not renamed.** It compared `bound.note` with the active
  tab's stored place, which is gone. The shell tests' `check` asserts H8
  instead.
- **The fresh last tab holds `Some(P::default())`**, so `Closed.shown` takes
  its place through one `take()` that panics on a broken H8, for both paths.
- **The trait.** `Place: Default`, with one associated
  `Note: Copy + Eq` and `note() -> Option<Note>`. The app's `Place`
  (`Note = EntryId`) and the proptest `Fake` implement it. `forget` takes a
  note, not a predicate.
- **Three deviations from Zed's `NavHistory`.**
  - Per tab, not per pane (Deviations from Zed).
  - Adjacent duplicates only. A push onto a top on the same note replaces the
    top; Zed removes every entry at that location. `a, b, a` stays, as in a
    browser.
  - Plain offsets, not anchors. An entry is clipped at restore; A2 keeps
    stacks plain, so anchors stay bounded by the number of tabs.
    `a_history_entry_whose_text_shrank_clips_on_return` pins it.
- **T4's third no-op case.** `⌘[` or `⌘]` with no entry on another note
  changes nothing on screen, so a pending open still lands. When a place
  arrives, `show_place` drops `activation`, as every switch does.
- **One walk for both directions.** `back` and `forward` share a private
  `walk` with a `fn` selector (`type Walk<P>`). It pops before `leaving` runs,
  since the app's `leaving` seals an undo group. Revised 2026-11-06: `Way`
  (`Back`, `Forward`) replaces `type Walk` and picks the two stacks, and the
  private `walk` takes a step count. `stops` lists where successive walks
  arrive; `walk_to` reaches one of them in one walk (invariant 9).
- **The sidebar opens through `open`.** A click on the note showing takes
  `show_note`'s same-note branch: nothing loads, nothing is pushed, forward
  stays, and the editor takes focus.
- **The highlight rule: a compare-and-set (after review).** Tree gestures
  write `Sidebar::selected` at once: a click on any row, the arrows, a create,
  the trash neighbour. Each of them goes through `Sidebar::select`, which
  bumps a private generation, `cursor`. `set_showing(Some(id))` calls
  `Sidebar::follow(id, since)`, which moves `selected` to `id` only while the
  generation is still `since`. A synchronous tab op (switch, back, forward,
  close, `⌘T`, `NoteClosed`) reads `since` in its own update, so it always
  moves the highlight. An async landing (`open_note`, `open_in_new_tab`)
  captures `since` when it spawns and passes it through `land` and `bind`. So
  a landing never undoes a cursor the user moved after the open began: a tree
  command would act on that row. `set_showing(None)` leaves `selected`, so a
  tree trash keeps the neighbour it moved to, and `follow` does nothing while
  a name is typed. There is no reveal (Deferred). This closes S1's known gap.
  It replaces the plan's red/blue item 1: with a click writing nothing, an
  arrow pressed during a pending open was overwritten by the landing.
- **Test adjustments.**
  - Four tests passed when first run, each a guard.
    `a_tab_switch_pushes_nothing` gives tab 1 a note behind and a note ahead
    before the switches: it fails if a switch clears the forward of the tab
    left, or pushes onto the tab arriving (both checked by injecting the
    bug). `back_with_nothing_behind_keeps_a_pending_open` fails if a back with
    nothing behind drops `activation`.
    `a_tab_emptied_by_a_trash_goes_back_to_its_last_note` needs no pruning.
    `a_sidebar_click_on_the_note_showing_focuses_the_editor` is a guard by
    plan.
  - `a_jump_inside_the_note_showing_pushes_nothing` jumps inside `a` after
    `⌘[`, with `b` ahead: a push would clear forward, and it fails when the
    same-note path goes through `open_note`. The pop loop hides a push on the
    back stack, so forward is the only witness. It calls `open` through the
    shell's entity: inside a window update, `show_note`'s `with_window`
    returns `None`.
  - `a_superseded_activation_leaves_the_sidebar_selection_on_the_note_showing`
    clicks a blocked row in an empty tab, then switches to the tab on `b`: the
    synchronous switch moves the highlight over the pending click.
  - `cmd_backspace_asks_before_trashing_the_selected_note` drops its `down`:
    the landing of `a` selects it, so `down` would move to `b`.
    `a_command_runs_where_it_was_listed_though_a_note_lands` keeps its
    pre-S4 expectation (`a` renamed); its arrows (`down up`) now come after
    the open of `b` began, since an arrow before it is overwritten by design.
  - `a_landing_keeps_a_tree_cursor_moved_since_its_open_began` is the review's
    scenario; `a_click_on_a_note_row_selects_it_at_once` pins the synchronous
    write. The selection tests live in `shell/tests/sidebar_selection.rs`.
  - The model's `op()` strategy weights commits and walks, so stacks grow
    deep enough to walk. `view_of` moved to the shared shell test helpers.
- **Mutants.** `cargo mutants -p inkra-tabs --test-package inkra-tabs`: 65
  mutants after review, 48 caught, 17 unviable, none missed, no exclusion
  added.

Lever, with its trigger:

| Lever | Trigger |
|---|---|
| Make H8 unrepresentable: a zipper (tabs left of the active one, the active tab with no place, tabs right of it), so no `Option` place and no H8 panic | A third H8 panic site, or H8 failing in a proptest |

## As built (S5)

Plan: `docs/superpowers/plans/2026-09-30-tabs-s5.md`, with its binding red/blue
section. Built as planned, with these facts and deviations:

- **The closed stack lives in `Tabs`.** A private `ClosedTab { place, index,
  back, forward }` per entry, in a `VecDeque`, oldest first. `close` pushes one
  through a private `remember`: a blank entry (no note, no history) is skipped
  (invariant 7), and a push onto 1024 entries drops the oldest (invariant 6).
  There is no adjacency rule: two closed tabs on one note are two tabs to
  reopen. The public `Closed { place, index, shown }` is unchanged; `place` is
  the entry's place, cloned, so the `Place` trait gains a `Clone` bound.
- **`close(id, leaving, resolve)`.** The entry's place is `leaving()` for the
  active tab and `resolve(stored)` for an inactive one; each runs only in its
  case. `close_tab` unparks the closed id **before** `close` and moves the view
  into `resolve`, so the closed stack keeps plain offsets (A2). The old
  `let _ = self.parked.unpark(..)` after `close` is gone. One closure taking
  `Option<P>` was rejected: `leave_active` stays the one caller of `leaving`.
- **`reopen(leaving)` is `insert` plus the entry's stacks.** It pops the
  newest entry first and returns `None`, calling no `leaving`, when there is
  none. Else it calls `insert(entry.index, leaving)`, which clamps, parks the
  tab left, takes a fresh id and puts it first in the MRU; the new tab takes
  the entry's `back` and `forward`, and `reopen` returns the entry's place.
  `leaving` runs once, inside `insert`. `insert` keeps its two arguments: the
  red/blue shape passed `entry.place` to it, but the active tab holds no place
  (H8).
- **The app's reopen is a tab switch.** `reopen_closed_tab` dismisses the
  picker, parks the tab left through `insert`'s closure, and shows the place
  through `show_place`, with no `arrive`: the fresh id has no parked entry, and
  the view is plain, clipped at restore. The sidebar follows through
  `set_showing`.
- **Four deviations from Zed's closed stack** (`pane.rs:479`,
  `reopen_closed_item`, `workspace.rs:3310-3321`):
  - The tab comes back at its old index, clamped (invariant 8), as a browser
    does. Zed reopens beside the active item.
  - With its history. Zed's entry is one item; ours is the whole tab.
  - No skip. Zed skips an entry whose item is still in the pane; a tab is a
    slot, and one note may show in two tabs.
  - An empty tab with no history is not kept (invariant 7). Zed keeps no entry
    for a preview item, the nearest case.
- **A trash prunes the closed stack.** `forget` empties each entry on the
  note, drops the note from both its stacks, then drops every entry left
  blank. Order is kept. `forget_note` already called `tabs.forget`.
- **T4's fourth no-op case.** `⌘⇧T` with nothing closed changes nothing on
  screen, so a pending open still lands. A reopen that brings a tab drops
  `activation` through `show_place`.
- **Reopening after the last tab closed keeps the empty tab** beside the
  reopened one: removing it would be a special case, and `⌘W` closes it.
- **`⌘⇧T` is `cmd-shift-t`.** macOS keeps `shift` for a letter key, as for
  `cmd-shift-n`. The palette lists "Reopen closed tab, ⇧⌘T".
- **Test adjustments.**
  - The model harness passes a `resolve` that marks the place (`RESOLVED`), so
    the `Op::Close` arm tells a resolved place from a stored one. `Op::Reopen`
    joins the ops; its arm asserts C2, C4 and C5 against the newest entry.
    `check` asserts C1 and C3 on the closed stack. `entries` moved to the
    model's `tests.rs`.
  - `a_tab_switch_changes_no_history` skips the reopened tab: its history is
    the entry's, which the `Op::Reopen` arm asserts.
  - `reopen_pops_the_newest_closed_tab` drains the whole stack and compares it
    with the entries reversed.
  - Two units give their tab a note first
    (`closing_an_inactive_tab_keeps_the_place_resolve_gives`,
    `a_reopened_tab_gets_a_fresh_id`): after invariant 7 a blank tab is not
    kept, and they would pass vacuously.
  - `trashing_a_closed_tabs_note_keeps_its_history` passed before the pruning
    landed: `show_place`'s missing-buffer branch empties the tab. It stays as a
    guard: it fails when a trash drops the entry outright (checked by
    injecting the bug).
  - The five Task 4 guards passed when first run. Three were checked by
    injecting their bug: no `keep_active_tab_visible`, `activation` dropped
    unconditionally, no `dismiss_modal`.
  - `start_open` and `selected` moved to the shared shell test helpers.
- **Mutants.** `cargo mutants -p inkra-tabs --test-package inkra-tabs`: 75
  mutants, 58 caught, 17 unviable, none missed, no exclusion added.

Levers, each with its trigger:

| Lever | Trigger |
|---|---|
| Drop `Closed::{place, index}` and the `Place: Clone` bound: nothing in `app` reads them, only the model tests | The next change to `close`'s signature |
| Have `insert` return the index it used, so `reopen` does not look the new tab up with `active_index` | The next change to `insert` |

## As built (S6)

Plan: `docs/superpowers/plans/2026-09-30-tabs-s6.md`, with its binding red/blue
section. Built as planned, with these facts and deviations:

- **The model: `Tabs::move_tab(id, to)`**, Zed's remove-then-insert, clamped.
  Dragged right, a tab lands after the tab dropped on; dragged left, before
  it. It needs no `Place` and takes no `leaving`: no tab is left. An id that
  is no tab changes nothing. The MRU, the active id, every place and history
  and the closed stack stay (invariant 3).
- **The gesture is gpui's drag and drop, as Zed's tab bar.** Each tab has
  `on_drag` with a `DraggedTab { id, index, title, active }`, `drag_over` and
  `on_drop`. The drop reads `id`; `index` only picks the indicator's side. The
  preview is a plain `div` with no id and no listener, so it inserts no hitbox.
- **The app handler is a reorder.** `AppShell::move_tab` moves, keeps the
  active tab in the bar and notifies. No `dismiss_modal`: a picker's backdrop
  takes the press and dismisses it, so no drag starts under a picker.
- **The press rule: every press on a tab lands focus on the active tab's
  content.** The shell's root tracks focus, and gpui focuses a focusable
  element on any press that bubbles to it (`div.rs:2765-2780`). A drop, a
  click on the active tab and a close of an inactive tab show no place, so
  nothing would focus the content again. One handler, `press_tab`, on the
  left and middle buttons of each tab, calls `window.prevent_default()` and
  then focuses the active tab's content through `land_focus`: the editor, or
  the root on an empty tab. A `show_place` after the press may focus again,
  which is harmless. This also closes a gap older than S6: a middle click on
  an inactive tab left focus on the root.
- **Deviation from the red/blue review, item 2.** A drag begun on the × and
  released on it closes nothing. gpui clears every element's pending press
  while a drag is active (`div.rs:2234-2243`), so the ×'s click never fires,
  and the tab's drop onto itself moves nothing. The test is
  `a_drag_started_on_the_x_and_released_on_it_closes_nothing`, not
  `.._closes_the_tab_only`. No click fires anywhere during a drag, which is
  what keeps a dragged tab from activating.
- **Four deviations from Zed's tab drop:**
  - A drop does not activate the tab (Zed's `move_item` does). Invariant 3
    keeps the active id.
  - No drop target after the last tab: a drop on the last tab puts the tab
    last, and the bar's children stay the tabs, which `scroll_to_item` indexes.
  - No split zones, no cross-pane move, no ⌥-drag clone, no path or selection
    drops: one pane, one bar.
  - The indicator is Zed's: the target's background and a 2 px border on the
    side the tab lands, with colours and width from `theme.rs`
    (`tab_drop_background`, `tab_drop_border`, `DROP_EDGE`).
- **T4's fifth no-op case.** A drop leaves the active tab and its note, so a
  pending open still lands in it.
- **The look.** The `tabs-dragging` shot shows the target lit, its right edge
  drawn, and the preview under the pointer. The indicator's border takes
  layout space, but the shot shows no tab shifted beyond the 2 px edge. The
  preview covers part of the target's title, as in Zed. The `tabs` shot is
  unchanged. The scene presses at `AppShell::tab_center`, which reads
  `tab_scroll.bounds_for_item` under the `shot` feature.
- **Test adjustments.**
  - `DraggedTab` gained `index` and `active` with their readers in the second
    commit: a field nothing reads fails clippy.
  - `moving_an_unknown_id_changes_nothing` moves to index 1, so a fallback to
    index 0 fails it.
  - `a_drop_outside_the_tabs_moves_nothing` and
    `a_drag_under_a_picker_moves_nothing` passed with no drag listener: they
    are guards. The other drag tests failed first, which proves the `drag`
    helper.
  - The model's R1 proptest reuses `closed_entries` from the closed-stack
    tests.
- **Review fixes.** The press rule above replaced a bare `prevent_default`,
  which left focus in the tree after a click on the active tab or an inactive
  tab's ×. Four tests pin it, each starting with the tree focused. The
  per-tab builder moved to `render_tab`, and `tab_titles` returns
  `SharedString`s. `reopen_cancels_a_pending_open` asserts `activation` is
  dropped: the reopened tab has a fresh id, so no landing gate could show a
  kept task. `close_tab` keeps its unpark before `close`: by I1, `parked`
  holds no entry for an id that is no tab, so no view can be dropped.
- **Mutants.** `cargo mutants -p inkra-tabs --test-package inkra-tabs`: 76
  mutants, 59 caught, 17 unviable, none missed, no exclusion added.
- **Final review: no tab shows a note being trashed.** `NoteStore::buffer_for`
  applies `openable`, as `open_note` does, so `show_place` finds no buffer for
  a `Trashing` note, forgets it and shows the tab empty; the later
  `NoteClosed` finds nothing left to forget.

## As built (a failed trash keeps its tabs)

Plan: `docs/superpowers/plans/2026-09-30-failed-trash-keeps-tabs.md`. It
replaces the last S6 bullet: `show` no longer forgets a note being trashed.

- **The rule: a note leaves the tabs only on `NoteClosed`.** While its trash
  is pending, every tab, stack and closed entry keeps it. `NoteClosed`
  forgets it everywhere, as before. `TrashFailed` shows it again in the
  active tab if that tab is still on it, at the top of the note.
- **`Shown` replaces `Option<Bound>`.** `Empty`, `Bound(Bound)` or
  `Trashing(EntryId)`. Only `Bound` holds an editor (T2), so nothing can be
  typed into a note whose file is about to go. `set_shown` is the one
  writer; its recent, sidebar and backlinks effects read `showing()`.
- **`showing()` vs `active_note()`.** `showing()` is the note bound in the
  editor (`Bound` only): T3, backlinks, recent, the sidebar highlight, the
  gone banner, following a link and quick open read it. `active_note()` is
  the note the active tab is on (`Bound` or `Trashing`): `capture`, `park`,
  `forget_note`'s active check, `tab_titles` and the note `Tabs::back` and
  `forward` skip read it.
- **T4, as the user decided on 2026-09-30: a user action drops a pending
  open; a trash outcome updates the tab like a landing and keeps it.** `show`
  is the user path: it drops `activation`, then calls `display`. `display`
  binds, shows `Trashing` or shows `Empty`, and touches no `activation`. The
  `NoteClosed` and `TrashFailed` handlers (`note_closed`, `trash_failed`)
  call `display`. So an open started from a `Trashing` tab, here or in a new
  tab, lands whatever the trash's outcome.
- **`Parked` holds `(EntryId, Option<ParkedView>)`.** A tab left while
  `Trashing` has a note and no view, so I1 stays exact: an entry iff the tab
  is inactive and on a note. `unpark` flattens the two `Option`s. The loss is
  accepted: after a failed trash, such a tab opens its note at the top. The
  same loss hits a tab that arrives at the note while `Trashing`, even with a
  view parked before the trash: `display` drops the view it unparked. The
  view cannot be re-anchored without an editor. A failed trash is rare.
- **The focus on `TrashFailed`.** The shell focuses the note only if its root
  holds the focus, or if an open picker restores to the root (`land_focus`
  then retargets the dismissal, `Modal::restores`). The root holds it when
  the `Trashing` tab took it there through `show_place`. A tree the user
  moved to keeps the focus, with or without a picker over it.
- **Test adjustments.** The three "being trashed shows it empty" tests now
  see the note in the active tab during the trash, and no note after
  `NoteClosed`. `check` asserts that a `Trashing` note has no buffer.
  The new tests are in `shell/tests/failed_trash.rs`.
  `a_note_closed_on_the_active_tab_cancels_a_pending_new_tab_open` became
  `..._keeps_a_pending_new_tab_open`: the new tab lands after the emptied
  tab.

## As built (tab views one owner)

Spec: `docs/superpowers/specs/2026-09-30-tab-views-one-owner-design.md`. It
replaces A2's parked table and S2's `arrive`.

- **`Tabs<Place, Parked>`.** An inactive tab's slot holds its `Parked`: its
  note and its anchored view. Stacks and closed entries hold a plain `Place`.
  The table beside `Tabs` and `AppShell::arrive` are gone.
- **The closure types pick park or capture.** `insert`, `activate` and
  `reopen` take `Parked::park`; `commit`, `back` and `forward` take `capture`.
  `close` takes one `closing`: it captures the active tab and unparks an
  inactive one. `activate` and `Closed.shown` yield a `Parked`, which binds
  only through `unpark`.
- **I1 and I2 hold by type.** `check` no longer asserts them. `Tabs::close`
  panics on a slot that breaks H8, before `closing` runs.
- **`Tabs::forget` drops a forgotten note's `Parked`** without forgetting its
  anchors, as `drop_note` did: the buffer is gone.

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
| Reveal the note showing in the tree: expand its folders, scroll to it | A user loses a nested note in the tree |
| Anchors for back and forward entries | Back lands off its text after an edit elsewhere |
| Mouse buttons 4 and 5, and swipe, for back and forward | A user asks |
| Cap the history kept per closed tab: each of 1024 entries can hold 2 × 1024 places | Memory measured to grow with tabs closed |
| Scroll the bar while a drag nears its edge | A user cannot drag a tab past the visible part of a crowded bar |
