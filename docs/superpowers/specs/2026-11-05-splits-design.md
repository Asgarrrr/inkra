# Splits: a tree of tab groups in the centre and in each dock

Status: **approved 2026-11-05** in a brainstorm on HTML mocks
(`.superpowers/brainstorm/*/content/title-row.html`, gitignored). Sections 1 and 2's
title row were approved by the user; the rest are defaults taken by Claude under
delegation ("je te laisse enchaîner"), marked *(delegated)*.

## Why

The user wants two notes side by side, the trigger the tabs spec set for splits.
The user also wants panes inside the docks, as Obsidian has: a dock split in two
with Files above and an outline below. The docks spec deferred both (D6) and
parked the dock header merged into the title row; this spec takes both up.

## Decisions taken

- **D1. A tree of tab groups everywhere.** The centre and each dock hold a tree
  of groups, split horizontally or vertically, nested at will (Zed's
  `PaneGroup`, extended to the docks as Obsidian does).
- **D2. A tab holds a note or a view.** The centre accepts both; a dock accepts
  views only. A note never lands in a dock.
- **D3. A view follows the last active note.** `Backlinks` shows the note of the
  last focused centre group; focus in a dock leaves it unchanged. Pinning a view
  to one note is deferred.
- **D4. The top groups' tabs live in the title row** (mock option A). Every group
  whose top edge is the window's top draws its tab strip in the title row; a
  lower group draws its own header. Dock tabs are icons. This revisits the
  docks spec's D3 and its parked merged header.
- **D5. `SearchVault` and `NewNote` move into a header of the Files view.** They
  keep their keys and their palette entries.
- **D6. Five gestures**, landed in this order: the tab context menu (split right,
  split down), resizing a separator, keys, dragging a tab onto a group, and
  `⌘⌥`-click on a link.
- **D7. A note opens in the focused centre group.** With focus in a dock, it
  opens in the last focused centre group. A link followed from a note opens in
  that note's group.
- **D8. Splitting shows the same note in the new group**, with its own caret,
  as the tabs spec already allows for two tabs (rule 3).

## Deviations from Zed

- **Trees in the docks.** Zed's dock shows one panel; only the terminal panel
  embeds a `PaneGroup`. Inkra follows Obsidian here.
- **Normalised on every mutation.** Zed collapses a one-child axis on remove only
  and tolerates an axis nested in one of the same direction.
- **The neighbour is computed on the tree.** Zed probes a pixel 4 px past the
  pane's edge in the last frame's bounding boxes. Inkra computes unit rectangles
  from the flexes and probes at the caret's height, the same answer without a
  frame (see Layout, `neighbour`).
- **Pure data.** Zed's tree holds `Entity<Pane>` and `Arc<Mutex<…>>` flexes and
  boxes. `inkra-layout` has no gpui (`check.sh`); the app keeps the measured
  sizes the resize needs.
- **JSON in the session file**, not SQLite tables. Flexes are validated on read
  as Zed's `PaneAxis::load` does.
- **Dropped:** zoom, maximise, swap, pinned tabs, followers, `SplitMode::EmptyPane`.

## Layout (`inkra-layout`)

`Docks`, `Dock` and docks D6 go; `View` and `Side` stay.

```rust
pub enum Surface { Center, Dock(Side) }
pub enum Axis { Horizontal, Vertical }
pub enum Member { Group(GroupId), Split { axis: Axis, children: Vec<Member>, flexes: Vec<f32> } }
pub struct Layout {
    center: Member,
    left: Option<Member>,
    right: Option<Member>,
    shown: [bool; 2],
    last_center: GroupId,
}
```

`Layout` knows where each group sits and how much room it takes. It does not
know what a group's tabs hold; the app does.

Operations: `split(group, direction, room, min) -> Option<GroupId>`,
`fits_split(group, axis, room, min)`, `removable(group)`, `remove(group)`,
`resize(path, index, delta)`, `equalize(path)`, `activate(group)`,
`neighbour(group, direction, probe) -> Option<GroupId>`, `rects(surface)`,
`surface_of(group)`, `top_groups()`.

**Normalisation**, after every mutation:

- a split with one child is replaced by that child;
- a split whose parent has the same axis merges into it, its children's flexes
  scaled by its own flex;
- flexes sum to `children.len()`; a stored set of the wrong length or sum
  resets to all 1.0.

**`neighbour`** lays the surface out in the unit square from the flexes, takes
the active group's rectangle, and picks the group whose rectangle holds the
probe point moved past that edge. The probe is the caret's fraction of the
group's height (or width), given by the app, else the rectangle's centre.
Across a surface edge it continues into the adjacent surface's nearest group;
from a dock it reaches `last_center`.

### Invariants

| # | Rule |
|---|---|
| L1 | Each view is in at most one tab of the window. Closing `Files` removes it; its toggle or the palette reopens it on its default side. |
| L2 | A dock holds view tabs only. The centre holds notes and views. |
| L3 | The centre always has a group. Its last tab closed leaves an empty tab, as today. |
| L4 | A dock may be empty; it draws nothing and keeps its `shown` flag (docks rule 3). |
| L5 | One group is active. `last_center` is always a centre group. |
| L6 | Closing a group's last tab removes the group, except the centre's last. Its neighbour takes its room. |
| L7 | Every tree is normalised. Normalising never moves a group's rectangle. |

L1, L2 and L6 span the layout and the groups' tabs: the app's one writer
(below) checks them after every change in debug builds.

## Tabs (`inkra-tabs`)

A `Tabs` stays one group. Two changes:

- a tab moved into a group takes a fresh id from that group's counter, so an
  id is unique in its group *(delegated, S11)*;
- `take(id) -> Detached` and `put(detached, at)` move a tab between groups with
  its back and forward stacks. `take` on the active tab runs the leave's `park`
  as `activate` does; the tabs spec's H8 holds on both sides.

The closed stack stays per group. `⌘⇧T` reopens into the focused group
*(delegated)*. `forget(note)` runs on every group.

## App

A `Group` is `Tabs<Place, Parked>`, its `Shown`, its `EditorView` and its
`activation` task. `Place` and `Shown` gain a `View(View)` variant. Each group
keeps the tabs spec's rule of one rebound `EditorView`, now per group: N visible
notes, N editors.

- **One writer of the workspace**, as `change_docks` is today: every split,
  move, close and resize goes through it, normalises, checks the invariants,
  notifies and calls `save_session_later`.
- **The active group** feeds everything that read the single `shown`: `⌘S`
  (`NoteStore::current`), quick open's `showing`, back and forward, the nav row,
  `sidebar.follow`. `Backlinks` reads `last_center` (D3).
- **`open(note, range, open)` takes a target group** (D7). `Open::NewTab` opens
  in a new tab of that group; the new `Open::SplitRight` splits it first.
- **T4 per group.** An open commits only into the `(group, tab)` active when it
  began. Closing a group drops its pending activation.
- **Focus.** Clicking a group's text, tab or header activates it. The root
  `focus_handle` stays tracked; `unfocus_editor` runs before a group's editor
  drops.

## Chrome

### Title row (D4)

- The title row is built from `top_groups()`: each top group draws its tab
  strip in a segment as wide as the group. Segments sit left to right as the
  groups do; a hairline separates surfaces.
- The left-most segment reserves 88 px for the traffic lights, whichever surface
  it belongs to.
- **Note tabs** keep the browser look (Obsidian chrome D3). **View tabs** are
  28 px icons (S7); a strip that overflows ends in a `⋯` button listing the hidden
  tabs *(delegated)*.
- A group below the top draws the same strip in a 34 px header inside its rect.
- Each centre group has its own nav row (`←` `→`) under its strip, on its own
  history.
- Empty space in any title-row strip is a window drag region, as today.
- A dock's toggle sits at its segment's outer end. A hidden dock's toggle moves
  to the outer end of the title row, as today.
- The active group's active tab is marked as today. Other groups' active tabs
  are drawn as active, with muted text *(delegated)*.
- Dock widths stay the docks spec's constants; resizing a dock stays deferred.

### Files header (D5)

A 34 px row at the top of the Files view (S7): `NewNote`, `NewFolder`, `SearchVault`,
as Obsidian's explorer row. It belongs to the view, so it moves with Files.

### Separators

A 4 px hitbox between siblings, a hairline drawn in its middle. Dragging it
resizes the two neighbours. Minimum sizes are 80 px wide and 100 px high, as
Zed's. A double-click equalises the split.

## Gestures

| Gesture | Behaviour |
|---|---|
| Tab context menu | `Split Right`, `Split Down` (the tab's note, or the view moved), `Close`. Uses `ui::ContextMenu`. |
| Separator drag | Resize, clamped to the minimum sizes; double-click equalises. |
| `⌘K` then an arrow | Split the active group in that direction *(delegated; Zed's convention, check its keymap via DeepWiki before binding)*. |
| `⌘K` then `⌘` + an arrow | Focus the neighbour in that direction *(delegated, same check)*. |
| Tab drag | Over a group: within `min(w, h) × 0.2` of an edge, the nearest edge wins and the drop splits; else the drop moves the tab into the group. A half-group overlay shows the result. Over a tab strip: reorder or insert. A note tab over a dock shows nothing and drops nothing (L2). |
| `⌘⌥`-click on a link, `⌘⌥↵` | Open the link in a split to the right of its group (`Open::SplitRight`). |

Zed's keys, checked at zed@526c95d4 via DeepWiki:

- `cmd-k up/down/left/right` binds `pane::SplitUp/Down/Left/Right` in the
  `Pane` context (`assets/keymaps/default-macos.json:770-773`).
- `cmd-k cmd-up/down/left/right` binds `workspace::ActivatePaneUp/Down/Left/Right`
  in the `Workspace` context (`default-macos.json:876-881`).

Zed's drop target size is 0.2 (`assets/settings/default.json:205`), against
the 0.33 first written here; S12 takes 0.2.

A split that would leave either side under its minimum size does nothing.
Dragging a view tab replaces the dock icon drag; the empty-dock drop strips
stay (dock_drag.rs:30's reason holds).

## Session

`VERSION` becomes 2. The file holds one tree per surface:

```json
{ "version": 2, "root": "…", "active": 3,
  "center": { "split": "horizontal", "flexes": [1.0, 1.0],
              "children": [ { "tabs": [ … ], "active": 0 }, { "tabs": [ … ], "active": 0 } ] },
  "left":  { "tabs": [ { "view": "files" } ], "active": 0 },
  "right": { "tabs": [ { "view": "backlinks" } ], "active": 0 },
  "shown": [true, true] }
```

- A note tab is the version-1 `SavedTab`; a view tab is `{ "view": name }`.
- `active` is the depth-first index of the active group.
- A version-1 file still restores: its tabs become one centre group and each
  dock one group, so no user loses a session on upgrade.
- Each surface decodes leniently: a malformed one falls back to its default
  alone, as `docks` does today.
- The tree and its view tabs restore synchronously at bootstrap, as the docks
  do. Note tabs still wait for the vault walk and land into their groups.
- A group left with no tab after the restore's resolve is removed (L6, L3).
- An unknown view name drops its tab alone; a view missing from every tree goes
  back to its default dock.

## Failure handling

- A note trashed or gone empties its tabs in every group (`forget` on each).
- Two groups on one note share its `Buffer`; each keeps its own caret, anchors
  and scroll.
- A drop or a split refused by L2 or the minimum sizes is a no-op, never an error.
- A session tree that breaks L1 or L2 (a view twice, a note in a dock) keeps the
  first occurrence and drops the rest, as `Docks::restore` does.

## Testing

- `inkra-layout` proptests over random operation sequences:
  - every invariant holds after each operation;
  - normalising never moves a group's rectangle;
  - `neighbour` agrees with a brute-force search over the rectangles;
  - the session round-trip is the identity on normalised trees.
- `inkra-tabs`: `take`/`put` keep H1–H8 and the stacks; ids stay unique.
- `app`, headless against `MemFs`:
  - one test per gesture, `simulate_keystrokes` for every key;
  - opening from the tree with two groups;
  - `Backlinks` following `last_center`;
  - a version-1 session restoring;
  - a gone note in two groups;
  - `⌘S` saving the focused group's note.
- Shots, looked at:
  - two notes side by side;
  - a dock split in two;
  - the title row with the left dock hidden;
  - a drop-zone overlay;
  - a view in the centre.
- `cargo mutants` on `layout` and `tabs`.

## Slices

Each slice is vertical and lands as its own PR, near 150 lines when possible.
The P rows below are the phases; this run lands them as S1–S6, and the follow-up run continues from S7.

| Slice | Content | User sees |
|---|---|---|
| S1 | Groups: one tab group per surface (P0). | Nothing changes (shots compared) |
| S2 | Two editors on one buffer: a pinned selection and a shared parse. | Nothing yet |
| S3 | Split Right: a row of centre groups, the last-active group feeding note consumers. | Two notes side by side |
| S4 | Closing a group (L6) and the tab context menu. | The menu's Split Right and Close |
| S5 | Split Down: a normalised tree of groups in the centre. | Notes above and below |
| S6 | Session v2: the tree, every group's tabs and the dock groups, with v1 still read. | Splits survive a relaunch |
| S7 | Dock view strips in the title row, and the Files header (D5). | Dock icons beside the toggle, new note, new folder and search under it |
| S8 | Dock trees: `Option<Member>` per dock, drawn, split from the dock menu, saved and restored. | Files above Backlinks in one dock |
| S9 | P2 separators: drag to resize, double-click to equalise, minimum sizes, the split refused for room. | Resizable splits |
| S10 | P3 `⌘K` keys: split in four directions, focus the neighbour. | Keyboard splits |
| S11 | P4 tab moves: `take`/`put` between groups, views into the centre, notes refused on docks. | Drag a tab to another group, Files into the centre |
| S12 | P4 edge drops that split, P5 `⌘⌥`-click and `⌘⌥↵` into a split. | Drag to split, links to the side |
| S13 | Review fixes: minimums by flex, a dock body's drop, a centre view's split, a lone view's edge, drags that outlive their target. | Split Right moves a centre view; a view lands in the lower dock group it is dropped on |

The follow-up run covered the rest, all landed:

- the `⋯` overflow of a dock's view strip, deferred to `docs/roadmap.md`;
- P2 separators: resize, equalise, minimum sizes, the split refusal (S9);
- P3 `⌘K` keys with `neighbour` and focus-driven activation (S10);
- P4 tab drag: `take`/`put`, per-group `TabId`s, views into the centre (S11), the edge zone (S12);
- P5 `⌘⌥`-click on a link and `Open::SplitRight` (S12).

The phases:

| Phase | Content | User sees |
|---|---|---|
| P0 | `Layout` replaces `Docks`: one group per surface, the docks' views as icon tabs of a dock group; the app's `Group` holds what `AppShell` held once. Session v2 with v1 read. | Nothing changes (shots compared) |
| P1 | Split Right/Down from the tab menu; axis rendering; the title row from `top_groups()`; the Files header; the active group feeding `⌘S`, quick open, history, `Backlinks`. | Two notes side by side |
| P2 | Separator resize and equalise. | Resizable splits |
| P3 | `⌘K` keys: split and focus a neighbour. | Keyboard splits |
| P4 | Tab drag between groups, edge drops, views into the centre, note refused on docks. | Drag to split |
| P5 | `⌘⌥`-click and `⌘⌥↵` open a link in a split. | Links to the side |

P0 cannot split into "layout crate, then app": deleting `Docks` breaks the app
in the same commit, so S1 lands both.

S1's choices:

- `TabId`s stay per group until P4's `take`/`put` *(delegated)*.
- The dock view logic lives in the app's one writer, `change_layout`;
  `check_layout` replaces the dock proptest *(delegated)*.
- The session stays version 1 until S6, a deviation from the P0 row *(delegated)*.
- Zed's flex epsilon, 0.001, is adopted *(delegated)*.

S2's choices:

- A pin holds the selection only *(delegated)*. An inactive editor keeps its
  selection as a `ParkedSelection` in its buffer, re-read on every buffer
  version; `Reloaded` leaves it to the anchors. The top line keeps its pixel
  offset: a wheel scroll on an inactive group would leave a pinned top anchor
  stale. Revisit when a user notices the jump.
- One parse per buffer version, shared by every view of the buffer through the
  `Parses` global *(delegated)*. N views of one note parse once per edit.
- Shaping stays per view, since each view has its own width *(delegated)*.

S3's choices:

- One focus field, `last_center`, until P3 adds `active` *(delegated)*. Every
  reader of the old single centre reads it: the tree's follow, backlinks, quick
  open, `cmd-w`, the session.
- A mouse press activates a group: `capture_any_mouse_down` on its column and
  on its title strip *(delegated)*. A press that leaves focus in the editor of
  the group left moves it to the group entered, since that editor is now pinned.
  A press on the sidebar toggle in the first strip (left dock hidden) also
  activates the first group.
- A split halves the split group's flex, then rescales the row to sum to its
  length *(delegated)*. Zed resets every flex to 1; halving keeps the other
  groups' widths.
- A tab dropped on another group's strip is refused until P4 *(delegated)*. Each
  group numbers its own tabs, so the drop checks `DraggedTab.group`, not the id.
- An inactive group's active tab draws its label muted *(delegated)*. The tab
  keeps the note colour and hairline, so the strip still shows what the group holds.
- The notice draws in `last_center`'s column only *(delegated)*.
- No minimum group size until P2 *(delegated)*.
- A debug selector in centre group `k > 0` takes a `-{k}` suffix; group 0 keeps
  the bare name, so every single-group test and shot reads as before *(delegated)*.
- An open lands in the group it began in (T4 per group): `Origin` carries the
  group and the tree cursor. Closing a group drops its pending open with its `Group`.
- An open's focus moves only if its group is still `last_center` when it lands
  *(delegated)*. A background open in a group the user left never takes the focus.
- The columns sit in the main row itself, not in a wrapper row *(delegated)*: a
  nested row has taffy measure the note's content and shape it twice per
  keystroke. A strip has basis 0 and no padding, so its width is its column's;
  the trailing inset is a spacer in the last strip.
- The hairline between columns is an absolute 1 px child of the right column, not
  its border *(delegated)*. With basis 0, a border gives the column a 1 px base
  size its strip lacks, and the strip drifts off the column by up to 1 px.
- gpui drops a test's or a shot's entities with the app, unreleased. The shell's
  `on_release` unpins its editors only when its window closes, so the split
  tests and `Stage`'s drop close every window first *(delegated)*. `Stage`'s drop
  (`shot/stage.rs`) is outside the slice's files; it applies to every scene, and
  a scene with one group renders the same.

S4's choices:

- The heir takes the closed group's closed stack *(delegated)*, the tab just closed
  on top, so `⌘⇧T` still brings it back. Each group keeps its own stack (Tabs), so
  dropping the stack with its group would lose every tab closed there.
  `ClosedTabs` moves it whole; the heir keeps the newest `HISTORY` entries.
- The heir is the left neighbour, else the right one, and takes the closed
  group's width; the other widths stay. Deviation: Zed's `PaneAxis::remove` resets
  every flex to 1, which would undo widths the user dragged; this matches S3's split.
  The heir becomes `last_center` only if the closed group was; then it is unpinned,
  feeds the note consumers and takes the focus, where Zed focuses the last pane.
- The tab menu lists `Split Right`, then `Close` *(delegated)*. `Split Down`
  joins it with S5. A right press activates the tab's group and the tab first,
  so both entries act on `last_center`. Deviation: Zed's menu binds each entry to
  the tab under the pointer and leaves the active tab alone; activating first lets
  the entries dispatch the existing actions.

S5's choices:

- A lower group's header holds its strip only: the tabs and `+`, with no drag
  region and no toggle *(delegated)*. The window moves from the title row, and
  the toggles stay at its ends.
- In a tree, the closed group's flex goes to its previous sibling, else its next
  one *(delegated)*. The heir is the previous sibling's last group, depth-first,
  else the next sibling's first: the group that touches the closed one. With
  every sibling a group, this is S4's left-else-right rule.
- A split replaces the group with a two-child split on its axis, flexes 1 and 1
  *(delegated)*. Within a same-axis parent, normalising splices it in, so the two
  halve the group's flex, as S3's row did. A removal rescales the parent's flexes
  to sum to their count before normalising, so the validity check never resets them.
- Normalising validates a split's incoming flexes against Zed's epsilon, resets
  an invalid set to 1.0, splices same-axis children with their flexes scaled,
  collapses a one-child split, then rescales the flexes to sum to their count
  *(delegated)*.
- A root horizontal split's children sit in the main row itself, as S3's columns
  did; any other split draws a flex wrapper with basis 0 and `min_*_0`
  *(delegated)*. Split Right therefore renders as before, and a keystroke in a
  lower group still shapes its note once.
- The rule before a member is an absolute 1 px child along its parent's axis:
  a left rule in a row, a top rule in a column, for S3's reason *(delegated)*.
- `k`, the debug selector's index, is the group's depth-first index *(delegated)*.
- The old row's slice readers in `shell.rs`, `session.rs`, `tabs.rs` and
  `docks.rs` read `center_groups()` instead *(delegated)*. This mechanical edit
  sits outside the slice's files.

S6's choices:

- A view tab saved in the centre is dropped until P4 lets the centre hold views
  *(delegated)*. A note tab saved in a dock is dropped, since a dock holds views only.
- The active group maps to its own group if it survives, else to the nearest
  earlier survivor, else to the first group *(delegated)*. This is the tab rule
  (`rposition`) one level up.
- Each dock saves and restores as one group until dock trees land *(delegated)*.
- A saved centre that holds a split with no children decodes as missing: the
  centre falls back to one empty group, and the docks still restore.
- A planned group the user closed before the restore landed is skipped; its
  notes do not reopen elsewhere.

S7's choices:

- The Files header is 34 px (`PanelHeader`), not 30, so every panel has one
  header height *(delegated)*.
- View tabs are 28 px `IconButton`s, not 24 *(delegated)*.
- The `⋯` overflow is deferred to `docs/roadmap.md` *(delegated)*. Two views and
  fixed dock widths never overflow a strip. The trigger is a third view or
  dock resizing.
- A dock's title-row segment holds, left to right for the left dock: the
  lights' space, the toggle, the view icons, drag space. The right dock's holds
  the icons, drag space, the toggle. The left toggle's bounds are the same
  whether its dock shows or hides.
- Each view draws its own header in the column: Files its three buttons,
  Backlinks its title and count. A header carries no drag and no menu; the icon
  does, so a one-view dock no longer draws its title.

S8's choices:

- `VERSION` stays 2 *(delegated)*. A dock saved by S6 or S7 is one group, which
  is also a valid tree, so every existing file decodes as is.
- `Layout::restored` honours the saved `active` *(delegated)*. An index out of
  the centre's range falls back to the first centre group.
- The dock menu offers Split Right and Split Down only when the group holds two
  views or more *(delegated)*. A lone view has nothing to split off, and
  splitting it would leave an empty group.
- A lower dock group starts with a 34 px header that holds its view strip, as a
  lower centre group's holds its tab strip. The title row holds one segment per
  top dock group.
- A dock emptied by a move or a restore becomes `None`; its `shown` flag stays,
  so showing it again draws nothing until a view lands.

S9's choices:

- `Layout::resize` converts pixels to flex as `px × len / size` *(delegated)*.
  Zed divides by the container alone because its flexes sum to 1; ours sum to
  the child count.
- A drag moves only the separator's two neighbours *(delegated)*. Zed cascades
  past a neighbour at its minimum into the next one; here the drag stops there.
- A side already under its minimum may grow and is never forced bigger. The
  clamp's floor is the lesser of the minimum and the side's current size.
- A split member's minimum along its parent's axis is the sum of its children's
  when it runs along that axis, and its largest child's when it runs across.
- A group's minimum width includes the title-row chrome it draws *(delegated)*:
  a centre group's curves, one tab, the new-tab button and the drag space; a
  dock group's view icons and drag space; the lights and the left toggle on the
  window's left-most top group, the right toggle and the inset on its right-most.
  80 px stays the floor.
- Split Right and Split Down are refused when half of the group would fall
  under that group's own minimum *(delegated)*. The check reads the unit
  rectangle and the window size, not the last frame's bounds.
- A press on a separator keeps the editor's focus and activates no group: the
  hitbox occludes the columns under it.
- The separator replaces the hairline each member after the first drew, at the
  same pixels; its hitbox overhangs the member before by 1.5 px.

S10's choices:

- No `active` field: the active group is read from focus *(delegated)*. It is the
  Files dock group while the tree holds focus, else `last_center`. Focus already
  says which group takes keys; a stored field would follow every focus change.
- `⌘K` and an arrow on the Files dock group splits Files off as the dock menu
  does: two views or more, and room for both. On a centre group it is S9's split,
  refused for room.
- `⌘K ⌘` and an arrow towards Backlinks does nothing *(delegated)*. The panel has
  no focus handle and takes no keys.
- The probe is the caret's centre as a fraction of the group's rectangle
  *(delegated)*. `TextInput::caret_point` reads the last frame's bounds and
  layout. The rectangle is the unit rectangle times the surface's room, offset by
  the title row and the drawn left dock, as S9's refusal reads it. With no caret
  drawn, the probe is 0.5.
- A rectangle is half-open, so the point is clamped 1e-4 short of the edge's end.
  A caret scrolled below its group gives a probe of 1 and still finds the group
  past the edge.
- From the centre, Left and Right continue into a drawn dock at the same height:
  the docks and the centre share the unit square's height under the title row.
- A right press on a tab focuses the tab's content before its menu opens, as a
  left press does *(delegated)*. The menu returns focus before
  its entry runs, and focus now picks the group a Split entry splits: with the
  tree focused, Split Right would split the Files dock. The contract does not
  settle this and `tab_bar.rs` is outside the slice's files, so it sits in its
  own commit.
- The eight bindings sit in the `AppShell` context. Deviation: Zed binds the
  splits in `Pane` and the focus moves in `Workspace`; here one context holds
  every shell key.
- `Layout::split` takes a `SplitDirection`, so the layout tests in `resize.rs`
  and `surface.rs` and the palette test in `splits.rs` change with it. The
  palette label of Split right now carries its key. These edits sit outside the
  slice's files.

S11's choices:

- `TabId`s stay per group: `put` gives the moved tab a fresh id from the
  receiving group's counter *(delegated)*. This replaces the spec's shared
  counter. Every app use is keyed by `(group, id)`, and T4 lets an open pending
  on the moved tab land nowhere.
- Two payloads: a note tab drags a `DraggedTab`, a view tab a `DraggedView`
  *(delegated)*. gpui exposes no reader for the active drag, so a dock refuses a
  note (L2) by listening to `DraggedView` only. Centre strips, tabs, headers and
  bodies accept both; a body takes the drop at the end of its group.
- A same-group `DraggedTab` over a body is ignored; over a tab it reorders, as before.
- A group of one tab leaves with its tab (L6). The centre's only group keeps an
  empty tab instead (L3), so a lone view moved out of it leaves `Place::Empty`.
- The × of a view tab in the centre returns the view to its default dock, not
  out of the window *(delegated)*. A view closes from its dock.
- `Open::Here` whose target tab shows a view opens a new tab in that group, and
  the view stays *(delegated)*. A commit would push the view into the tab's
  back stack.
- A centre view tab is still dropped at restore, and the view returns to its
  default dock *(delegated)*. The centre lands after the vault walk, and Files
  must show during it. The session format does not change.
- A centre view's side is its default side, so the dock menu's move to the
  other dock sends it to the default side's other dock *(delegated)*. Revealing a
  centre view activates its tab and its group.
- `land_focus` on a centre group showing Files is the tree's focus; showing
  Backlinks, it is the root.
- `docks.rs`'s `dock_tabs` became `tab_state` over every group, tab counts
  included, so `change_layout` notices a note tab moved between groups.
- These edits sit outside the slice's files: `group.rs` gains `remove_group`,
  which `close_group` and the move share, and exposes `Group::with` and
  `tree_focus`; `title_bar.rs` and `render.rs` take view drops on the strips;
  `dock_chrome.rs` shares the view body; the shot scene sits in
  `scenes/docks.rs` with a `drag_drop` helper in `stage.rs`. S3's refusal test in
  `splits.rs` is removed, and `dock_drag.rs`'s miss now drops on the title row's
  drag space, since a centre body takes a view.

S12's choices:

- The edge zone is Zed's 0.2 of `min(w, h)` (`DROP_FRACTION`), not this spec's
  0.33 *(delegated)*. Zed's value is tried; 0.33 left little room to move a tab
  into a group.
- `Open::from_click` is unchanged: only an editor link reads `⌥` and splits
  *(delegated)*. A sidebar, picker or backlinks row keeps `⌘`-click and `⇧`-click.
- A link opened in a split goes to a fresh empty group to the right of the
  last-active centre group, then opens there *(delegated)*. Where the split is
  refused for room, the link opens in a new tab instead.
- `open` takes no window, so `Open::SplitRight` defers to the next effect cycle
  and runs with the shell's window there *(delegated)*. The deferral lands
  before the next frame, so nothing shows late. Threading a `Window` through
  `open`'s callers (`follow`, the pickers, the sidebar and backlinks rows)
  would touch five modules for this one variant.
- A drop that splits splits first, then detaches the tab, as the contract
  orders. The fresh group is held empty before the detach: `Layout::remove`
  names it the heir of a source group the detach empties, and it keeps that
  group's closed tabs.
- With the left dock empty, a view held over the left drop strip also lies in
  the centre body's left band, so the body's left half lights beside the strip
  *(delegated)*. The strip sits on top and takes the drop; the stale
  `drop_split` is recomputed on the next drag's first move. The
  `docks-drag-empty-side` shots show both tints.
- `dragging.rs`'s R7 test drops in the body's middle, outside the slice's
  files: its old drop point lies in the top band, which now splits.

S13's choices:

- A split's minimum along its own axis is the largest child minimum scaled by
  the flexes' sum over that child's flex; across its axis, its largest child's.
  This replaces S9's sum, which let a child at a small flex fall under its own
  minimum when its parent shrank.
- A view dropped on a dock group's body lands as that group's last tab. The
  body no longer calls `append`, which keeps the cases with no known group.
- Split Right and Split Down on a centre view tab move the view into the fresh
  group (L1). `split_in` runs `split_view` for a view on any surface, and
  `split_view` now calls S12's `split_off`, so a centre split activates and
  focuses the fresh group as a drop's does.
- A group's only view tab splits nothing *(delegated)*: the keys and the tab
  menu do nothing there. The dock menu offers no split for a lone view, and a
  lone tab's own-edge drop shows no zone; the split would leave nothing behind.
- A view held over its own group's edge follows the tab rule: no zone where it
  is that group's only tab. `track_drop` takes the dragged tab's group for both
  payloads.
- A drag whose tab or separator a tree change removed does nothing. A
  separator drag ends at its next move; a tab drop finds no tab and is ignored.
- `DROP_FRACTION` is crate-private: only `drop_zone` reads it.

## Architecture review's choices

Slice 1, the layout owns split refusal and removability:

- `Layout::fits_split` holds the rule `AppShell::fits_split` held: the group's unit share
  along the axis, times `room`, halved, is at least its minimum. The app passes `room`, its
  surface's px length along the axis, and the minimum as a closure, as `resize` takes it.
- `Layout::split` refuses where `fits_split` does: it returns `None` and leaves the layout,
  `next` included, as it was. A group not in the layout is refused, no longer a panic.
- `removable` is false for a group not in the layout, as `fits_split` is *(delegated)*:
  `remove` panics on one.
- `split_off` asks `fits_split` before `change_layout`, as `split_view`'s pre-check did, so a
  refused split leaves an open menu and the focus alone *(delegated)*. The split inside it
  then cannot refuse, and panics if it does.
- The drop overlay keeps its filter through `Layout::fits_split` *(delegated)*: without it,
  a zone would show where the drop then splits nothing.
- A refused `split_group` moves no focus. Its test holds the root's focus: from the tree,
  `⌘K →` splits the Files group through `split_view` and never reaches `split_group`
  *(delegated)*.

Slice 2, one tab move for notes and views:

- `move_tab_to` in `tab_moves.rs` is the one move of a `(group, tab)` to `(group, index)`.
  `move_view` resolves `tab_of` and calls it; `append`, `drop_view_at_end` and `split_view`
  sit beside it.
- Within one group a move is a reorder, notes and views alike: the active tab, the active
  group and the focus stay, as the tabs spec's T4 fifth no-op case *(delegated)*. A view
  dropped on its own group's icon, strip or body no longer becomes active. Docks rule 1
  reads as a move into another group.
- Across groups the moved tab becomes the destination's active tab, notes and views alike.
  A centre destination becomes the active group and takes the focus; a dock destination
  shows *(delegated)*.
- The dock's show step runs inside the move's one `change_layout`, keyed on the destination's
  surface, not in `move_view` *(delegated)*: a second `change_layout` would see Files in a
  hidden dock between the two and drop the tree's focus. L2 keeps notes out of docks, so the
  step is a view's in practice.
- The past-the-end assertion covers every move, notes included *(delegated)*: no note caller
  passes such an index.
- A dock icon compares its index with the dragged view's only when that view is in its
  group; a view from another group shows the insertion marker a foreign tab shows.

Slice 3, one group column for the centre and the docks:

- `group_column` in `column.rs` builds a group's column on both surfaces: the caller's base
  element, the lower header, its rows, its body and the separator. `group_body` builds the
  body's view drop target and the drop overlay. A pure refactor: ids, selectors and every
  shot are unchanged.
- The tab drops are the centre's to add after `group_body` returns, not a parameter of it
  *(delegated)*: the dock body takes a view only, the centre also a tab, and an enum of the
  two would cost more than the one place that differs.
- `render_title_dock` builds a dock's title segments for a `Side`: the left leads with the
  lights and the toggle, the right ends with the toggle and an inset. The rule on the
  centre-facing edge stays a `match` in the title builder and in `render_dock`; a shared
  helper would not shorten either.

Slice 4, the shell's modules by concern; a move, not a rewrite:

- `workspace.rs` holds `change_layout`, `check_layout`, `tab_of` and `tab_state`: the one
  writer and what it reads and checks. `docks.rs` keeps the toggles, `reveal`, the dock menu
  and each view's title and icon.
- `geometry.rs` holds `dock_ends`, `min_sizes`, `room_along`, `surface_size`, `rect_of` and
  `bounds_of`. `separator.rs` keeps the separator element and its drag.
- `split.rs` holds the `⌘K` actions, `split_in`, `split_group`, `activate_pane` and `probe`.
  `group.rs` keeps `Group` and the group's lifecycle. `panes.rs` holds `VaultPanes`, so the keys take `split.rs`
  *(delegated)*.
- `column.rs` holds the empty state, the gone banner, the nav row, the note column and the
  group body that takes drops. `members.rs` holds `Slot`, `render_member` and `render_split`.
  `render.rs` keeps `Render` and `render_columns`.
- `probe` reads the caret's place from `bounds_of`, the group's pixel rect on its own surface,
  no longer from the centre's room. Only a centre group has a caret today, so no shot changes.

## Deferred

| Item | Trigger |
|---|---|
| Pin a view to one note (Obsidian's "link with tab") | A user wants a note's backlinks while editing another |
| Resizing a dock | Chrome spec S2b's trigger |
| A note in a dock | The user asks (rejected 2026-11-05) |
| Bottom dock | Docks spec's trigger |
| Splits in a second window | Multiple windows land |
| Zoom or maximise one group | A user asks |
