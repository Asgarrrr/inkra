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
    active: GroupId,
    last_center: GroupId,
}
```

`Layout` knows where each group sits and how much room it takes. It does not
know what a group's tabs hold; the app does.

Operations: `split(group, direction) -> GroupId`, `remove(group)`,
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

- `TabId`s come from one counter shared by every group, so an id is unique in
  the window;
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
  24 px icons; a strip that overflows ends in a `⋯` button listing the hidden
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

A 30 px row at the top of the Files view: `NewNote`, `NewFolder`, `SearchVault`,
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
| Tab drag | Over a group: within `min(w, h) × 0.33` of an edge, the nearest edge wins and the drop splits; else the drop moves the tab into the group. A half-group overlay shows the result. Over a tab strip: reorder or insert. A note tab over a dock shows nothing and drops nothing (L2). |
| `⌘⌥`-click on a link, `⌘⌥↵` | Open the link in a split to the right of its group (`Open::SplitRight`). |

Zed's keys, checked at zed@526c95d4 via DeepWiki:

- `cmd-k up/down/left/right` binds `pane::SplitUp/Down/Left/Right` in the
  `Pane` context (`assets/keymaps/default-macos.json:770-773`).
- `cmd-k cmd-up/down/left/right` binds `workspace::ActivatePaneUp/Down/Left/Right`
  in the `Workspace` context (`default-macos.json:876-881`).

Zed's drop target size is 0.2 (`assets/settings/default.json:205`), against
this spec's 0.33; P4 decides.

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
The P rows below are the phases; this run lands them as S1–S6.

| Slice | Content | User sees |
|---|---|---|
| S1 | Groups: one tab group per surface (P0). | Nothing changes (shots compared) |
| S2 | Two editors on one buffer: a pinned selection and a shared parse. | Nothing yet |
| S3 | Split Right: a row of centre groups, the last-active group feeding note consumers. | Two notes side by side |
| S4 | Closing a group (L6) and the tab context menu. | The menu's Split Right and Close |
| S5 | Split Down: a normalised tree of groups in the centre. | Notes above and below |
| S6 | Session v2: the tree, every group's tabs and the dock groups, with v1 still read. | Splits survive a relaunch |

A follow-up run covers the rest:

- the Files header (D5);
- dock tab strips in the title row: view icon tabs, overflow;
- dock trees (`Option<Member>`);
- P2 separators: resize, equalise, minimum sizes, the split refusal;
- P3 `⌘K` keys with `neighbour`, focus-driven activation and the `active` field;
- P4 tab drag: `take`/`put`, a shared `TabId` counter, views into the centre, the edge zone;
- P5 `⌘⌥`-click on a link and `Open::SplitRight`.

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

## Deferred

| Item | Trigger |
|---|---|
| Pin a view to one note (Obsidian's "link with tab") | A user wants a note's backlinks while editing another |
| Resizing a dock | Chrome spec S2b's trigger |
| A note in a dock | The user asks (rejected 2026-11-05) |
| Bottom dock | Docks spec's trigger |
| Splits in a second window | Multiple windows land |
| Zoom or maximise one group | A user asks |
