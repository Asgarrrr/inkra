# Docks: movable views

Status: **approved 2026-11-05** in a brainstorm on HTML mocks
(`.superpowers/brainstorm/62211-1793878872/`, gitignored). Not built.

## Why

The sidebar and the backlinks are hard-wired: `render_columns` draws the tree
on the left and the backlinks on the right, and `PanelsShown` holds two
booleans. The user wants to move views at will, modelled on Zed's `Dock` and
`Panel` (`vendor/zed-reference/crates/workspace/src/dock.rs`). The header rule
was agreed in the Obsidian chrome spec (D6): a dock with one view shows its
title, a dock with several shows an icon bar.

Two notes side by side are **splits**, a separate subsystem (Zed's
`PaneGroup`). The tabs spec deferred them with the trigger "a user wants two
notes side by side"; the user raised it in this brainstorm. Splits get their
own spec after this one. The note column stays one pane here.

## Decisions taken

- **D1. Two docks, left and right.** No bottom dock. `Side` is an enum, so a
  bottom dock can be added later without reshaping the model.
- **D2. A view moves by menu, by palette, then by drag.** The menu and the
  palette land first: they settle the model, and the drag only calls it.
- **D3. A dock's header sits under the title row**, level with the backlinks
  header. The title row does not change.
- **D4. The layout is session state.** It goes in the session file, beside the
  tabs, not in `<vault>/.inkra/settings.json`.
- **D5. A closed set of views and a pure model.** `View` is an enum; the
  layout rules live in a new gpui-free crate, `inkra-layout`.
- **D6. One view shows at a time in a dock.** No vertical stacking inside a
  dock: that is a split inside a dock, deferred to the splits spec.

## Deviations from Zed

| Zed | Here | Why |
|---|---|---|
| `trait Panel` + `Arc<dyn PanelHandle>` in a `Dock` entity | `enum View`, a pure `Docks` value, a `match` in `app` | Zed's panels come from a dozen crates and extensions. Ours are two views in one crate; a trait object would be a plugin system ahead of need. A pure model also tests without a `Window`. Revisit when a view comes from outside `inkra-app`. |
| A panel moves by its button's context menu only | Menu, palette, and drag | The user asked for views movable at will; with a mouse that means drag. |
| Bottom dock | None | Both views are columns (D1). |
| `right_click_menu`: a `deferred` menu owned by the clicked element, focused two frames later with `on_next_frame` | The shell's modal layer, focused at once with `window.focus` | The modal layer already paints last and already closes on every tab action and the palette, so no `deferred` is needed. A focus in the same frame keeps headless tests working; Zed waits for the deferred draw to link the handle. |
| `ContextMenu` items carry handlers, toggles, submenus, and asides | `(label, action)` entries | Two entries, both actions; a choice dispatches after the focus returns. |
| Size per panel, resizable | Width per side, fixed | Resize is the chrome spec's S2b, still deferred. |
| Dock state in the workspace database | The session file | Same intent, our existing store (D4). |

## The model: `inkra-layout`

A new workspace crate. It depends on no workspace crate and has no gpui
dependency; `check.sh` enforces the latter. `app` depends on it. The splits
spec will add its pane tree here.

### Data

- `View { Files, Backlinks }`, `Side { Left, Right }`.
- `Docks { left: Dock, right: Dock }`. A `Dock` holds its views in order, its
  active view, and whether it is shown.
- **Invariant: each `View` appears exactly once, in one dock.** Only
  `move_view(view, side, index)` relocates a view, so the invariant holds by
  construction.

### Rules

1. **A moved view becomes active in its new dock, and that dock shows.** The
   user always sees where it went.
2. **When the active view leaves, its right neighbour becomes active, else its
   left one.** This is the tabs' rule.
3. **An empty dock draws nothing**: no column, no title-row segment, no
   toggle. It keeps its shown flag for when a view comes back.
4. **A click on an icon activates its view.** A click on the active icon does
   nothing.
5. **Width belongs to the side**: 220 px left, 260 px right, as today.
6. **Default**: `Files` left, `Backlinks` right, both shown. It looks like
   today plus the `Files` header.

An invalid call (an index out of range) panics: a caller bug, per the project
rule. The model has no fallible operation.

### Drop targets

Per-icon drop targets: an icon inserts at its index (`Tabs::move_tab`'s rule,
counted after the dragged view left); the column appends (`Docks::append`).
The pointer-x function this section first named is dropped: `inkra-tabs` has
none to mirror. A tab is placed by the tab it is dropped on, one `on_drop` per
tab, so a dock icon is too.

## In `app`

- A `match` binds each `View` to its entity (`Sidebar`, `BacklinksPanel`), its
  icon, its title, and an optional count (the backlinks').
- `PanelsShown` goes. `VaultPanes` holds `Docks`.
- **Focus.**
  - Hiding a dock that holds focus lands it through the existing `land_focus`.
    This keeps today's rule: a hidden tree keeps neither focus nor an open
    name field.
  - A focused view keeps focus when it changes dock.
- **Actions.**
  - `ToggleLeftDock` on `cmd-\` and `ToggleRightDock` on `cmd-|` replace
    `ToggleSidebar` and `ToggleBacklinks`, same keys.
  - One action per view, `MoveFilesToOtherDock` and `MoveBacklinksToOtherDock`,
    listed in `⌘⇧P` as "Move files to other dock" and "Move backlinks to other
    dock". The palette humanizes every action name the same way, so the names
    carry no "Docks:" prefix and no title case.
- **Title row.** Unchanged. `SearchVault` and `NewNote` stay over the left
  dock: they act on the vault, not on the tree, so they do not follow `Files`.
  A dock's toggle sits where today's panel toggle sits, and goes with rule 3.

### Headers

- One view: `PanelHeader` with the view's title, and its count if any.
- Several views: an icon bar. The active icon is highlighted, and each icon has
  a tooltip with the view's name.
- No per-view tools in the header for now.

The tree gains a `Files` header. This partly reverses the Obsidian chrome
spec's D5, which removed the tree's `vault` header. It follows from that same
spec's D6 rule, which the user approved again here.

### Context menu

No menu component exists. `ui::ContextMenu` is new, modelled on Zed's, and
holds `(label, action)` entries and nothing more. A right-click on a one-view
header or on an icon opens it with "Move to Left Dock" or "Move to Right Dock",
and "Hide Dock". The bar's gaps open nothing. D3a chooses by mouse; D3b adds ↑,
↓, ⏎ and Esc.

- The menu lives in the shell's modal layer, anchored at the press: one modal
  at a time, a picker or a menu. Every tab action and the palette already
  close it through `dismiss_modal`.
- It is focused with `window.focus` when it opens, as a picker is.
- The selection follows Zed's: none at open, ↓ from none selects the first
  entry, ↑ from none the last, both wrap, and ⏎ with none selected does
  nothing. The keys bind in a `ContextMenu` key context to `picker::SelectPrev`,
  `picker::SelectNext`, `Confirm` and `Cancel`: the actions are reused, so the
  palette lists nothing new.
- The right press calls `prevent_default`, or the root's focus-on-press takes
  focus from the menu.
- A choice closes the menu, focus returns, and then the action is dispatched,
  as the palette does: it reaches the view the menu left.
- A press outside, left or right, closes it, as on macOS.
- Any `change_docks` closes an open menu: it describes the layout it opened on.
- The menu's entity exists only after a right-click: launch and frames do no
  extra work.

### Drag

- An icon, or the title of a one-view dock, drags a `DraggedView`
  (`on_drag`, `drag_over`, `on_drop`, as the tabs do).
- Drop targets:
  - An icon: insert at its index. A drop in the same dock reorders.
  - The rest of a dock's column (the bar's gaps, a one-view header, the body):
    the column appends.
  - An empty side: during a drag only, a 40 px drop strip shows at that window
    edge. Without it an emptied dock could not be refilled by drag. The strip
    is drawn only during a drag, only for a side with no views, and tinted only
    for a dragged view: a tab drag sees nothing (`group_drag_over` on the
    root's group).
- A drop outside any target changes nothing.
- The drop edge an icon draws during a drag compares the drag's start index with the
  icon's index. This holds only because icons show on a dock that holds every view, so
  the drag comes from that same dock. With a third view the drop still lands right, but
  the edge can point the wrong way, and `DraggedView` must then carry its side again.

## Persistence

- `Session` gains an optional `docks` field. `VERSION` stays 1: an old file
  reads without it and gets the default layout.
- Views are stored as strings (`"files"`, `"backlinks"`). A serde enum would
  fail the whole file on an unknown name, and lose the tabs' restore with it.
- `Docks::restore` drops unknown names and duplicates. It appends each missing
  view to its default side. Each dock activates its last kept entry at or
  before the stored active index, else its first view. Any file yields a valid
  layout.
- The existing writer and its throttle save on every layout change: a move, an
  activation, a toggle.
- **The layout applies before the first frame.** `bootstrap::root` reads the
  session file once, with `block_on`, as it reads the theme. `AppShell::new`
  builds `VaultPanes` with the saved docks, so the first frame draws them and
  no layout jumps. The tabs' restore reuses the same `Session`; the notes
  still load behind the window. The read costs a median 0.30 ms (10 runs,
  `docs/startup-floor.md`, 2026-11-05).
- Applying the layout when the async read landed was measured first. The read
  landed 7.7 to 27.8 ms after the first frame (medians of two batches), so the
  default layout drew first. Waiting for it would have delayed the first frame
  instead. Both were rejected for the read before the first frame.

## Testing

- `inkra-layout`: a `#[test]` per rule; a `proptest` over random moves,
  activations and toggles that checks the invariant after each step; the
  `.proptest-regressions` file committed. `cargo mutants` on the crate, zero
  missed. Add the crate to the CLAUDE.md mutants list.
- `Docks::restore`: an unknown name, a duplicate, a missing view, an
  out-of-range active index, and an absent field each yield a valid layout.
- `app`, headless `#[gpui::test]` on `MemFs`:
  - `cmd-\` and `cmd-|` toggle the docks (`simulate_keystrokes`);
  - both move actions are listed in `⌘⇧P` and move their view;
  - focus lands on the note when its dock hides; a focused view keeps focus
    when it moves;
  - the menu works with ↑, ↓, ⏎ and Esc;
  - the layout round-trips through the session file;
  - the saved layout is drawn by the first frame;
  - a simulated drag drops a view at the expected index.
- Shots, looked at after each slice: `docks-default`, `docks-both-left`,
  `docks-menu`, `docks-drag-empty-side`.

## Slices

Each slice is vertical and lands as its own PR. D1 is split in two to keep each PR near 150 lines.

| Slice | Content | User sees |
|---|---|---|
| D1a | `inkra-layout` with `View`, `Side`, `Dock`, `Docks`, `toggle` and `reveal`; the shell draws its docks from `Docks`; the `Files` header; `ToggleLeftDock`, `ToggleRightDock`; the chrome spec's S2a toggle text updated; CLAUDE.md lines for the crate | The `Files` header |
| D1b | `move_view`, the empty-dock rule, the icon bar and its tooltips, both move actions in `⌘⇧P` | Moving by palette |
| D2 | The session's `docks` field, `Docks::restore`, the write, the read before the first frame and its measurement | The layout survives a restart |
| D3a | `ui::ContextMenu` in the modal layer, the right-click on a header or an icon, a choice by mouse | The menu |
| D3b | The menu by keys (↑, ↓, ⏎, Esc) and the `docks-menu` shot | The menu by keyboard |
| D4a | The drag: `DraggedView`, per-icon drop targets, the column's append (`Docks::append`), the `docks-drag` shot | Drag and drop between docks |
| D4b | The empty-side strips and the `docks-drag-empty-side` shot | Refilling an emptied dock by drag |

## Out of scope

- A bottom dock (D1).
- The header merged into the title row. The user found it more elegant; it
  was parked because the traffic lights take ~88 px on the left and the bar
  would overflow with 4–5 views. Revisit with a fix for both.
- Vertical stacking inside a dock (D6), with the splits spec.
- Splits in the note column: the next spec.
- Resizing a dock (chrome spec, S2b).
- Per-view tools in a header.
- `dyn Panel`: revisit when a view comes from outside `inkra-app`.
