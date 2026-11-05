# Shell chrome: components, icons, panels, title bar

Status: S0 shipped (#57), S1 shipped (#58). S2a shipped (#59); S3 specified 2026-10-02 (S3, below), ahead of S2b, S2c and S2d; S2b, S2c, S2d, S4 pending.

## Why

The engine is done: index, search, watcher, tabs and restore all ship. The chrome around it is not. Rendered on 2026-10-01 (`tabs`,
`sidebar-vault`, `quick-open-results`):

- A folder row and a note row look the same: no chevron, no icon. Depth is the
  only cue.
- Nothing reacts to the pointer. The crate has no `.hover(`, no `.active(`, no
  `cursor_pointer`, no tooltip.
- The tab's close button is the text glyph `×`. There is no icon anywhere.
- The sidebar and backlinks are fixed at 220 and 260 px and cannot be hidden.
- The system title bar costs a row, and the backlinks header sits 7 px lower
  than the tab bar beside it (measured 2026-10-02).

The cause is structural. Every view builds its chrome from raw `div()`s:

- 3 row implementations: sidebar, picker, backlinks.
- 6 ellipsis cells.
- 9 restatements of `UI_TEXT_SIZE` with `UI_LINE_HEIGHT`.
- 12 muted-text divs.

A hover state or an icon added to one of them misses the others. The user
asked on 2026-10-01 for interface work, starting with a layer of reusable
components as Zed's `ui` crate has.

## Decisions taken

Taken by the user on 2026-10-01:

1. **Interface work starts now**, ahead of markdown blocks S2/S3.
2. **A component layer comes first** (S0), and every later slice is built on
   it.

Proposed here, to confirm at review:

3. **Components are a module, not a crate**: `crates/app/src/ui/`. `app` is
   their only consumer. A crate earns its place when a second consumer exists.
4. **A component exists only with two call sites in the slice that adds it.**
   This is the "abstract only at a real seam" rule, applied to components. It
   leaves out `Divider` (one drawn hairline), `Disclosure` (see D5) and
   `Modal` (one site) until a second site appears. **One exception: `Icon`**
   ships in S0 with one call site, the tree's chevron. It is the only consumer
   of `Assets`, which S0 must build and test anyway, and its second site,
   `IconButton`'s `x`, is S1.
5. **Icons are Lucide**, copied verbatim, with its licence committed beside
   them. Zed's icons are not copied (see D2).
6. **A component owns the look; the view owns the interaction.** See D4.

## What Zed does, and what we take

Read in `vendor/zed-reference/crates/{gpui,ui,icons,assets,project_panel,workspace,title_bar,platform_title_bar}`
at `526c95d`. The `ui`, `icons`, `assets` and `theme` crates are
GPL-3.0-or-later. We study their API shape and write our own code. gpui is
Apache-2.0.

- **The component idiom.** A `#[derive(IntoElement)]` struct holds owned,
  `'static` data, implements `RenderOnce`, and is configured by consuming
  builders (`gpui/src/element.rs:181`). The derive wraps the subtree in
  `window.with_id(type_name)` (`gpui/src/view.rs:343,448`), so ids inside one
  component never collide with ids outside it.
- **Forwarding interactivity.** `Tab` holds a `Stateful<Div>` and implements
  `InteractiveElement` and `StatefulInteractiveElement` by returning its
  `interactivity()` (`ui/src/components/tab.rs:33,88`). The pane then calls
  `.on_click`, `.on_drag` and `.on_drop` on the `Tab` itself. D4 adopts this.
- **Icons.** `svg().path(p)` asks the app's `AssetSource` for the bytes at
  paint time (`gpui/src/svg_renderer.rs:257`). resvg rasterises them to an
  **alpha mask** (`svg_renderer.rs:231-252`), which is tinted with the
  element's own `text_color` (`gpui/src/elements/svg.rs:150-182`). The file's
  colours are ignored, and an svg without `text_color` paints nothing.
- **The project panel** draws a tree row as a `ListItem` with the chevron
  passed as a child `Icon`, not as a `Disclosure`. A file row gets an
  invisible spacer of the same width, so names align
  (`project_panel/src/project_panel.rs:6535-6571`). D5 adopts this.
- **States.** Hover is `.hover(|s| ..)` and pressed is `.active(|s| ..)`, which
  needs an id. Selected is a plain `.when(selected, ..)`. A child shown on hover
  uses `.group(name)` on the row and `.group_hover(name, ..)` on the child
  (`ui/src/traits/visible_on_hover.rs`).

## D1. The module

```
crates/app/src/ui.rs            // pub(crate) re-exports, h_flex, v_flex, UiText, selectable (S1)
crates/app/src/ui/assets.rs     // Assets: AssetSource, IconName
crates/app/src/ui/icon.rs       // Icon
crates/app/src/ui/list_item.rs  // ListItem
crates/app/src/ui/tab.rs        // Tab (S1)
crates/app/src/ui/indicator.rs  // Indicator (S1)
crates/app/src/ui/tests.rs
crates/app/assets/icons/*.svg   // Lucide, verbatim
crates/app/assets/icons/LICENSE // Lucide ISC + Feather MIT, verbatim
```

Everything is `pub(crate)`. `ui` depends on `theme` and gpui only. It never
reads a view, an entity or a vault type, so the dependency points one way:
views → `ui` → `theme`.

`ui.rs` holds four helpers. Every component and every view migrated by a
slice uses the three below. The fourth, `selectable`, came with S1 (S1-D1).

```rust
pub(crate) fn h_flex() -> Div { div().flex().flex_row().items_center() }
pub(crate) fn v_flex() -> Div { div().flex().flex_col() }

pub(crate) trait UiText: Styled + Sized {
    fn ui_text(self) -> Self { self.text_size(theme::UI_TEXT_SIZE).line_height(theme::UI_LINE_HEIGHT) }
}
impl<E: Styled> UiText for E {}
```

The `UiText` pair is restated at 9 sites today. A slice migrates the sites in
the files it touches. It does not sweep the others: no drive-by edits.

## D2. Assets and icons

```rust
pub(crate) struct Assets;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IconName { ChevronDown, ChevronRight }

impl IconName {
    pub(crate) const ALL: &[IconName] = &[..];
    pub(crate) fn path(self) -> &'static str;   // "icons/chevron-right.svg"
    fn bytes(self) -> &'static [u8];           // include_bytes!, one arm per variant
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;  // Borrowed, no copy
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}
```

- **Embedding.** `include_bytes!` in a `match`, with no new dependency. A
  missing file is a build error. `rust-embed` (8.12.0, MIT) earns its place
  only past about a hundred files, or for hot reload.
- **Registration.** There are two sites:
  - `gpui_platform::application().with_assets(Assets)` in
    `bootstrap::run` (`bootstrap.rs:88`).
  - `VisualTestAppContext::with_asset_source(platform, Arc::new(Assets))` in
    `shot/stage.rs:32`. Without it, a shot paints no icon, silently
    (`gpui/src/app/visual_test_context.rs:43-46`).
- **Tests.** `#[gpui::test]` hard-codes the empty asset source
  (`gpui/src/app/test_context.rs:171-175`) and has no setter. An icon there
  paints nothing but keeps its layout box. So geometry is tested headless, and
  pixels are checked by shot.
- **Source.** The icons are Lucide 1.49.0 (2026-09-29). Lucide is licensed
  ISC; icons derived from Feather are also MIT. Both notices ship in
  `assets/icons/LICENSE`. A Lucide icon has a 24×24 viewBox and
  `stroke="currentColor" stroke-width="2"`. usvg resolves `currentColor` to
  black, which is opaque, and that is all the alpha mask needs.
- **Not Zed's icons.** They are Zed's redraws of Lucide, in a GPL crate, with
  no per-file licence. We treat them as GPL-3.0-or-later and do not copy them.
- **The set grows by call site.** S0 adds `chevron-right` and `chevron-down`.
  S1 adds `x`. No icon lands without a use.
- **Stroke weight.** At 14 px, Lucide's stroke is about 1.17 px. Zed's own
  icons draw 1.2 px at 16 px, so the visual weight is close.

## D3. Theme

`theme.rs` stays a set of free functions. There is no `Global`, no
`ActiveTheme` and no `Color` enum: Inkra has one theme. The functions return
`Rgba`. A component's builder takes `impl Into<Hsla>`, so a call site passes
`theme::icon_muted()` unchanged. The trigger for a `Theme` global is a second theme
(light mode) or a user setting for size. The change is mechanical then,
because every colour already goes through `theme.rs`.

S0 adds:

| Item | Value | Use |
|---|---|---|
| `row_hover_background()` | white at alpha `0x0c`: #232323 on the sidebar (#181818), #2f2f30 on a modal (#252526) | a list item under the pointer |
| `icon_muted()` | `tab_inactive_text`'s #8a8a8a | chevrons |
| `ICON_SIZE` | 14 px | `Icon` default; equals `UI_TEXT_SIZE` |
| `ROW_INSET` | 8 px (today `INDENT_BASE`, and `px_2` in picker and backlinks) | a list item's left and right padding |
| `INDENT_STEP` | 12 px (today `sidebar::INDENT_STEP`) | one tree level |

`ROW_INSET` and `INDENT_STEP` move from `sidebar/mod.rs` to `theme.rs`, because
`ListItem` reads them. A size that sets the look of the chrome lives in the
theme.

Row states are overlays, so one pair works on the sidebar and the modal.
`selected_row_background()` becomes white at alpha `0x17`: #2d2d2d on the
sidebar, as before, and #39393a on a modal. On each surface, hover sits
between the background and the selected row.

## D4. `ListItem`: the component owns the look, the view owns the interaction

```rust
#[derive(IntoElement)]
pub(crate) struct ListItem {
    base: Stateful<Div>,          // carries the view's handlers
    indent: usize,                // tree depth; 0 outside the tree
    start: Option<AnyElement>,    // chevron or spacer
    children: Vec<AnyElement>,
    end: Option<AnyElement>,      // added in S1 for the dirty dot; S0 had no user (rule 4)
    selected: bool,
}

impl ListItem {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self;
    pub(crate) fn indent(self, depth: usize) -> Self;
    pub(crate) fn start(self, e: impl IntoElement) -> Self;
    pub(crate) fn selected(self, selected: bool) -> Self;
}
impl ParentElement for ListItem { .. }
impl InteractiveElement for ListItem { fn interactivity(&mut self) -> &mut Interactivity { self.base.interactivity() } }
impl StatefulInteractiveElement for ListItem {}
```

Render:

1. `h_flex`, `w_full`, `ui_text`.
2. `pl(ROW_INSET + INDENT_STEP * indent)`, `pr(ROW_INSET)`.
3. Background: a selected row gets `selected_row_background` and **no hover
   style**. An unselected row gets `.hover(row_hover_background)`. gpui
   applies a hover refinement over the base whenever the hitbox is hovered
   (`gpui/src/elements/div.rs:3451-3467`). So hover set on a selected row would
   repaint it in the hover colour. One `match selected` sets one or the other;
   a row never carries both.
4. The start slot is `flex_none`. The content is `flex_1`, `min_w_0`,
   `overflow_hidden`, with the children inside.

**The row does not decide wrapping.** Whether text stays on one line belongs
to the content, not to the row. gpui's `white_space` cascades through the text
style stack (`gpui/src/window.rs:2363-2365,3936-3944`). A `nowrap` on the row
would therefore stop the backlink excerpts from wrapping, and they wrap today
in the 260 px panel. The single-line callers keep their own
`whitespace_nowrap`, as today (`sidebar/render.rs:78`, `picker/render.rs:34`).
The backlinks never set it.

`items_center` changes no height. The row has no height of its own, so the
content fills it. It also keeps the field row's `FieldElement` at its forced
one line (`field.rs:136`) instead of stretching it.

**No height, no vertical padding.** The row is its line box, as today. The
sidebar's `a_click_far_down_the_list_lands_on_the_row_it_names` depends on
this, and so does `uniform_list`, which measures item 0. A row's height comes
from its content: one line in the tree, two in quick open, N in a backlink
group.

**Why the view keeps the handlers.** The sidebar and backlinks act at
mouse-down, with the press's modifiers, and the sidebar takes focus first. A
picker row acts at release, through `on_click`. These are two deliberate
behaviours, each commented in place. A component-level `on_click` would force
one of them, or grow a `Press::{Down, Up}` switch. Forwarding the
`Interactivity`, as Zed's `Tab` does, keeps every handler where it is written
today:

```rust
ListItem::new(("tree-row", item))
    .indent(row.depth)
    .start(disclosure_icon(row, open))
    .selected(selected)
    .whitespace_nowrap()
    .on_mouse_down(MouseButton::Left, cx.listener(..))   // unchanged
    .child(div().debug_selector(|| format!("tree-name-{item}")).child(label))
```

The label is a bare `String` child today. It gets a wrapper `div` so a test can
read where the name starts. `render_rows` returns `Vec<AnyElement>` in place of
`Vec<Div>`, as `picker/render.rs:21` already does.

**Ids.** Today a sidebar row and a backlink row have no id. `ListItem` holds
a `Stateful<Div>`, so it needs one. Hover does not: it is read from the
hitbox each frame (`div.rs:3452-3467`). Only `.active` keeps id-keyed state,
and that state lasts one press. So every list keys by position, as the tabs
and the picker already do (`tab_bar.rs:110`, `picker/render.rs:26`):

- `("tree-row", item)`, the item index, which the name field's row uses too;
- `("picker-row", ix)`, which already exists;
- `("backlink", ix)`.

An `EntryId` key was considered and dropped. Its `u32` is private
(`vault/src/vault.rs:22`), and the orphan rule forbids a `From<EntryId> for
ElementId`. Exposing it would widen `inkra-vault` for state that outlives no
gesture.

**Users in S0**, which satisfies rule 4:

- the tree row;
- the tree's name-field row. One component gives "same box as any other" by
  construction. Today `render_field` copies the row's styles by hand
  (`sidebar/render.rs:28-40`).
- the picker row wrapper, shared by quick open, vault search and the command
  palette;
- the backlink row.

## D5. The tree's chevron is an icon, not a `Disclosure`

A folder row's start slot is `Icon::new(ChevronDown | ChevronRight)` in
`icon_muted()`. Which one depends on `expanded.contains(&row.id)`. A note row's
start slot is an empty `div` of `ICON_SIZE` × `ICON_SIZE`. Every name at one
depth therefore starts at the same x.

There is no separate click target. A press anywhere on a folder row already
toggles it (`sidebar/tree.rs:31`). A chevron with its own handler would be a
second path to the same effect. Zed's project panel makes the same choice. A
`Disclosure` component waits for a toggle that is *not* the row's own action:
a collapsible backlinks group, or a panel section.

The dirty marker stays the text ` •` in S0. It moves to a dot element when the
tab gets one too (S1), which makes two call sites.

## D6. `Icon`

```rust
#[derive(IntoElement)]
pub(crate) struct Icon { name: IconName, size: Pixels, color: Hsla }
// new(name): size ICON_SIZE, color icon_muted(); .size(px), .color(hsla)
// render: svg().path(name.path()).size(size).flex_none().text_color(color)
```

`text_color` goes on the `svg` itself. Whether it inherits through a parent is
unverified, and the svg reads its own style (`svg.rs:150`).

## Deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| `ui` crate | `app/src/ui/` module | one consumer |
| `Theme` global, `ActiveTheme`, `Color` enum | `theme.rs` free functions, `Hsla` arguments | one theme; trigger: a second one |
| `DynamicSpacing`, `UiDensity`, rems | fixed `px` constants | no density setting on the roadmap |
| `Component`/`RegisterComponent` preview registry | none; shots are the gallery | the registry serves Zed's component preview only |
| `ListItem` owns `on_click` | `ListItem` forwards `Interactivity` | two press semantics, each deliberate (D4) |
| `rust-embed` via `fs_embed!` | `include_bytes!` match | 3 icons; no dependency |
| `IconName` by strum | `macro_rules! icons` generates the enum, `ALL`, `path` and `bytes` from one list | no dependency added |
| Zed's redrawn icons | Lucide verbatim | licence |

## Tests (S0)

The success criteria are written before the code. Each one fails on the
current tree.

1. `ui/tests.rs`, plain `#[test]`:
   - `every_icon_loads`: `Assets.load(name.path())` is `Some` and starts with
     `<svg`, for every `IconName::ALL`.
   - `no_icon_file_is_dangling`: every `assets/icons/*.svg` file is some
     `IconName::path()`, read through `std::fs` from `CARGO_MANIFEST_DIR`.
   - `unknown_path_is_none`.
2. `sidebar/tests.rs`, `#[gpui::test]` against `MemFs`:
   - `a_note_and_a_folder_name_start_at_the_same_x`. The name element gets a
     `debug_selector`; its bounds' left edge is compared between a folder and
     a note at the same depth. It fails without the spacer.
   - `a_child_name_starts_one_step_right`: depth 1 against depth 0 differs by
     `INDENT_STEP`.
   - `the_field_row_has_the_tree_rows_height`: the bounds of the field's row
     and a plain row have equal heights.
   - `backlink_excerpts_still_wrap`: a backlink whose excerpt is wider than the
     panel has bounds taller than one `UI_LINE_HEIGHT` per line of source text.
     It fails if the row forces `nowrap`.
3. Existing tests stay green unchanged, the click tests above all.
   `./scripts/test-names.sh` shows no lost test.
4. Shots, looked at:
   - `sidebar-vault` gains chevrons, with names aligned.
   - New `sidebar-hover`: the pointer over an unselected row.
   - New `sidebar-hover-selected`: the pointer over the selected row, which
     keeps `selected_row_background`.
   - New `picker-hover`: the pointer over a row that is not the selected one.
   - `backlinks` is unchanged apart from the hover.

There is no `cargo mutants` run: `app` is outside its scope (CLAUDE.md). The
falsification is the geometry tests above. Delete the spacer, or swap the
indent formula, and one of them fails.

## Slices

Each slice is vertical: components, the views moved onto them, tests and
shots, then `check.sh` green.

| Slice | Content | New components | Opens on |
|---|---|---|---|
| **S0** | `ui` module, `Assets`, Lucide chevrons, `ListItem`, `Icon`; tree, field row, picker and backlinks on `ListItem`; hover everywhere a row is clickable | `Icon`, `ListItem` | — |
| S1 | Tabs: a `Tab` component shared by the bar and the drag preview; the close glyph becomes Lucide `x`, shown on the active tab and on hover; a hover background; the dirty dot as an `Indicator` on tab and tree row. Specified in S1. Tabs, below | `Tab`, `Indicator` | S0 |
| S2 | S2a (specified below): `IconButton`, toggles, `PanelHeader`. S2b: resize. S2c: persist. S2d: tooltips. Original row: Panels: `IconButton` (its second site after the tab's close button); hide and show the sidebar and backlinks, with a resize handle (Zed's `dock.rs:1275-1332` pattern: `deferred`, `on_drag` + `on_drag_move` on the shell); a panel header shared by sidebar (vault name) and backlinks; aligned header heights | `IconButton`, `PanelHeader` | S1 |
| S3 | Title bar: `appears_transparent`, traffic-light padding, the tab bar on the title row, window drag area | — | S1 (`Tab`), S2 (header heights) |
| S4 | Status bar | — | S2 (panel layout); free against S3 |
| S5 | Translucency: a blurred window background, panels drawn as cards on it | — | S3 |

### Carried to later slices

The S0 quality review's levers, each taken up by the slice named:

| Lever | Slice | Change |
|---|---|---|
| A | S1 | `ListItem`'s end slot holds the dirty `Indicator`; the ` •` no longer clips on a long name |
| B | S1 | the hover-or-selected rule becomes one `ui` helper when `Tab` lands, its second user |
| D | S1, S2a, done | S1: `Role::Tab`, `Role::TabList` and `aria_selected` on `Tab`. S2a: `Role::Tree` on the tree, `Role::TreeItem`, `aria_selected` and `aria_expanded` on its rows, set at the call site (As built S2a, 8) |
| E | S1 | an icons `macro_rules!` at the third icon |
| F | deferred to the `Label` sweep (S2a-D4) | `UiText` becomes `StyledExt` with a muted variant, or a `Label` |
| G | deferred (S2a-D4) | `ListItem` owns the start slot's width when a second fixed-slot user appears |

The S1 reviews' levers:

| Slice | Change |
|---|---|
| S2a, done | `IconButton` owns its debug selector, padding, hover, `Role::Button` and label; `stop_propagation` stays at the call site |
| deferred: the third slotted component | one shared slot layout and slot selectors for `ListItem` and `Tab` |
| deferred, with lever F | `selectable` becomes a chained builder |
| S3 | the drag preview: its wrapper `div` (As built S1, 3; quality review lever 4), and its `Role::Tab` outside the `TabList` (`/code-review`) |
| the next per-tab field | one per-tab struct in `render_tab_bar`, in place of zipped vectors |
| a second control shown on hover | a show-on-hover helper |

The S2a quality review's levers:

| Slice | Change |
|---|---|
| S2b | a `Panel` enum and `PanelState { shown, width }` keyed by it: `toggle(Panel)`, `render_panel_toggle(Panel)`, `render_column(Panel)`; the fields private once S2c writes them |
| a focus slice | one shell-wide `on_focus_lost` → `land_focus` (Zed `workspace.rs:1892`), in place of the special cases |
| S3 or S4 | decide the no-vault layout and drop the asymmetric column branch |
| the roadmap's blur item | the name field cancels on blur, so the shell stops calling `cancel_edit` |
| S2d | `IconButton` renders its tooltip from `label`; an optional `.action()` gives a key hint and `aria_keyshortcuts`; toggles dispatch their action |
| S3 | `TAB_BAR_HEIGHT` becomes `PANEL_HEADER_HEIGHT` |
| `PanelHeader`'s first control | an end slot, which makes it the third slotted component |

## S3. Title bar: the tab row is the window's title bar

Specified 2026-10-02. The user asked for it from a reference app, with the
tabs in the native title bar, not under it. S3 ships ahead of S2b, S2c and
S2d, which stay pending; none of them blocks it. Choices taken under the
delegation are marked *(delegated)*.

### What S3 changes

- The system title bar goes. The window draws its content under the traffic
  lights (`TitlebarOptions { appears_transparent: true, .. }`), and the tab
  row takes the title bar's place.
- **The tab row spans the window's width**, above all three columns
  *(delegated: the reference app's layout and Zed's)*. Today it sits at the
  top of the note column only, and each side panel's header sits level with
  it. After S3 the row is the top of the window. The sidebar, the note column
  and the backlinks sit under it, and the two panel headers sit level with
  each other, under the row.
- The row, left to right: the traffic lights' space, the sidebar toggle,
  `#tab-bar`, a drag region, the backlinks toggle.
- **The window moves by a drag on the row's empty space**, and a double-click
  there does what the user set in System Settings (zoom, minimise or
  nothing). A press on a tab, a toggle or a × does neither.
- **With no vault, the sidebar column is not drawn** *(delegated; the S2a
  lever "decide the no-vault layout")*. The empty state takes the width. The
  title row is still drawn, with no toggles, so the window can still move.

### S3-D1. The window

`bootstrap.rs` opens the window with:

```rust
titlebar: Some(TitlebarOptions {
    title: None,
    appears_transparent: true,
    traffic_light_position: Some(TRAFFIC_LIGHT_POSITION),
}),
app_owns_titlebar_drag: true,
```

- `app_owns_titlebar_drag: true` makes the whole view app-owned for AppKit
  (`gpui_macos/src/window.rs:3497-3515`). AppKit then never drags the window
  from the title area, so a tab drag cannot also move the window, and a click
  there is not delayed while AppKit tells a click from a drag. The app moves
  the window itself (S3-D3). Zed sets the same option for the same reason
  (`zed/src/zed.rs:1712`).
- gpui centres the buttons in a container `button height + 2 y` tall
  (`gpui_macos/src/window.rs:744-748`). Their frame is 16 px tall, so `y` is
  `(TITLE_BAR_HEIGHT - 16) / 2`. With Zed's 34 px bar this is Zed's own
  `(9, 9)` (`zed/src/zed.rs:396`).
- `title: None` is today's value (the default). S3 does not change what the
  Window menu or Mission Control shows.

### S3-D2. The title row

`render_tab_row` moves to a new `shell/title_bar.rs`, which holds the row's
layout, its drag regions and the traffic lights' space: the window chrome is
one concern. The row keeps its `tab-row` selector, its background and its
bottom rule. It now spans the window. It is `TITLE_BAR_HEIGHT` tall.

| Element | Width | Content |
|---|---|---|
| leading drag region, `title-drag-start` | `TRAFFIC_LIGHT_PADDING`, or `ROW_INSET` in full screen | nothing; the native buttons draw over it |
| `panel-toggle-sidebar` | its own | unchanged |
| `#tab-bar` | its tabs' width, shrinking to scroll when they overflow | unchanged: tabs only, `Role::TabList`, the scroll |
| trailing drag region, `title-drag-end` | the rest, at least `TITLE_DRAG_MIN_WIDTH` | nothing |
| `panel-toggle-backlinks` | its own | unchanged |

- **`#tab-bar` stops growing.** It was `flex_1`. It becomes `flex_initial`
  with `min_w_0`: as wide as its tabs, and narrower, scrolling, when the row
  is short. The empty space after the tabs is then the trailing drag region,
  not the bar. Its children stay the tabs only, so `scroll_to_item` is
  unchanged (S2a-D2 test 2b).
- **The tabs fill the row's height.** `#tab-bar` is `h_full` and stretches
  its tabs, so a tab's hover and active background run from the row's top to
  its rule, as in the reference app. `Tab` itself is unchanged.
- **The traffic lights' space reads the window.** `window.is_fullscreen()`
  hides the native buttons, so the leading region shrinks to `ROW_INSET`, as
  Zed's does (`platform_title_bar.rs:248-251`).
- **With no vault** the row holds the leading region, `#tab-bar` and the
  trailing region.

### S3-D3. Moving the window: drag regions, not a draggable row

A drag region is a leaf `div` with no child. A left press on it arms a move
and calls `window.prevent_default()`. The root div tracks `AppShell`'s focus
handle, and gpui runs the innermost bubble listener first
(`window.rs:5780-5786`), so the press keeps the editor's focus
(`div.rs:2765-2780`). A click on it with `click_count() == 2` calls
`window.titlebar_double_click()`.

The move and the release listen on the shell's root div, not on the region:

- a mouse move while armed disarms and calls `window.start_window_move()`;
- a left release disarms.

`on_mouse_move` fires only while its own hitbox is hovered (`div.rs:298-310`).
On the region itself, a fast drag that leaves it before the first move event
would start no move. The root covers the window, so it sees that event. Only
a region's press arms, so a press on a tab never reaches the root's move
listener armed.

This is Zed's gesture set (`platform_title_bar.rs:196-230`), with the move
moved to the root. The armed flag is one `bool` on `AppShell`,
`title_drag_armed`: only one press happens at a time.

**Why regions, not the row.** Zed puts the handlers on the whole bar, so a
press on any child bubbles up to them. Ours holds tabs that drag and click.
gpui bubbles a child's mouse-down to its parent unless the child stops it
(`window.rs:5778-5786`). On a draggable row, a tab drag would arm a move, and
the first mouse move would drag the window instead of the tab. A double-click
on a tab would also zoom the window. Every child would need a
`stop_propagation`, and the next child added would forget it. With the
handlers on two leaf regions, no child of the row can reach them.

The trailing region keeps `TITLE_DRAG_MIN_WIDTH` when the tabs overflow, so
the window can always move by its title row.

### S3-D4. Panel headers

`TAB_BAR_HEIGHT` becomes `PANEL_HEADER_HEIGHT` (the S2a lever): the headers
are no longer level with the bar, only with each other. Its value stays
23 px. `PanelHeader` reads the new name.

**`row_center` reads the painted list, not a sum.** Today it is
`TAB_BAR_HEIGHT + UI_LINE_HEIGHT * (ix + 0.5)`, true in a window that holds
the sidebar alone. In the shell the sidebar now starts under the title row,
so every shell test and shot scene that clicks a tree row would land 34 px
high. A second variant adding `TITLE_BAR_HEIGHT` would be a second sum to
keep in step. Instead `Sidebar::row_center(&self, ix)` reads the tree list's
painted bounds through its `UniformListScrollHandle` and returns `None`
before the first paint, as `Picker::row_center` does
(`picker/render.rs:49-58`). It is right in both windows. Its callers pass the
sidebar entity.

### S3 theme

| Item | Value | Use |
|---|---|---|
| `TITLE_BAR_HEIGHT` | 34 px, Zed's macOS value (`ui/src/utils/constants.rs:17-19`) | the title row |
| `TRAFFIC_LIGHT_PADDING` | 78 px, Zed's value for the macOS 26 SDK (`ui/src/utils/constants.rs:10`) | the leading drag region |
| `TRAFFIC_LIGHT_POSITION` | `(9, 9)`, from `TITLE_BAR_HEIGHT` (S3-D1) | `bootstrap.rs` |
| `TITLE_DRAG_MIN_WIDTH` | 32 px *(delegated)* | the trailing drag region's floor |
| `PANEL_HEADER_HEIGHT` | renamed from `TAB_BAR_HEIGHT`, 23 px | `PanelHeader`, `row_center` |

### S3 deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| handlers on the whole bar | two leaf drag regions | the bar holds draggable tabs (S3-D3) |
| `TRAFFIC_LIGHT_PADDING` is 71 or 78 by a build-time SDK cfg | 78 | one build machine, SDK 26.5; trigger: a build on an older SDK |
| a `PlatformTitleBar` entity | a module of `AppShell` | one window kind, macOS only |
| the move listener on the bar | on the shell's root | a region is small; a fast drag would leave it before the first move (S3-D3) |
| `window_control_area(Drag)` on the bar | none | inert on macOS (`gpui_macos/src/window.rs:2041`); trigger: a Windows or Linux build |
| disarms in `on_mouse_down_out` | disarms on any move; the move starts only with the left button held | the root's up listener misses a release outside the window (As built S3, 10) |

### S3 tests

In `shell/tests/title_bar.rs`.

1. `the_tab_row_spans_the_window_above_every_column`: `tab-row` starts at
   x 0, is the window's width and `TITLE_BAR_HEIGHT` tall. `sidebar-column`,
   the note column and `backlinks-column` start at its bottom.
2. `the_traffic_lights_have_their_space`: `title-drag-start` is
   `TRAFFIC_LIGHT_PADDING` wide, and the sidebar toggle starts at its right
   edge. After `toggle_fullscreen` it is `ROW_INSET` wide.
3. `the_space_after_the_tabs_is_a_drag_region`: with two tabs,
   `title-drag-end` starts at `#tab-bar`'s right edge and ends at the
   backlinks toggle.
4. `overflowing_tabs_keep_a_drag_region`: with 30 tabs, `title-drag-end` is
   at least `TITLE_DRAG_MIN_WIDTH` wide, and `cmd-9` still scrolls `tab-29`
   into `#tab-bar`'s bounds.
5. `a_press_on_a_drag_region_keeps_the_editors_focus`, and arms a move; a
   release disarms it.
6. `a_drag_on_a_drag_region_moves_the_window`: a press on `title-drag-end`
   then a move reaches `start_window_move`, which the test platform leaves
   `unimplemented!()` (`gpui/src/platform/test/window.rs:568`). The test is
   `#[should_panic(expected = "not implemented")]`: `#[gpui::test]` moves the
   attribute to its outer `#[test]` and re-raises the panic
   (`gpui/src/test.rs:110-137`). A second case moves out of the region on the
   first move and expects the same panic.
7. `a_tab_drag_never_arms_a_window_move`: a press on `tab-0` leaves
   `title_drag_armed` false, and a drag to `tab-2` reorders the tabs with no
   panic. The existing tab drag tests stay green unchanged.
8. `no_vault_draws_no_sidebar_column`: the empty state starts at x 0 under
   the row, and the row is drawn.
9. `the_panel_headers_line_up_under_the_title_row`: both headers' tops equal
   `tab-row`'s bottom, and both bottoms equal that plus
   `PANEL_HEADER_HEIGHT`. A wrong constant moves every tree click in the
   tests, so this guards `row_center`.
9b. `a_click_at_row_center_lands_on_its_row`, in the shell: a click at
   `row_center(1)` selects tree row 1. It fails with the old sum.
10. The S2a tests that measured the headers against `tab-row` move to test 9.
    `test-names.sh` shows every other test kept.
11. Shots, looked at: `tabs`, `panels-hidden`, `toggle-hover`, `tabs-hover`,
    `tabs-dragging`, `sidebar-vault`, `backlinks`; a new `tabs-overflow`
    with 30 tabs. The shot PNG holds the gpui scene only, so it shows no
    traffic lights and no native bar; its leading region is empty.
12. The real app, looked at: `cargo run -p inkra-app <vault>`, then a
    screenshot of the window. The traffic lights sit centred in the row, a
    drag on the empty space moves the window, a double-click zooms it, a tab
    drag reorders tabs and does not move the window. A test cannot see the
    native buttons or the AppKit drag.

### S3 red/blue, 2026-10-02

| Finding | Verdict | Change |
|---|---|---|
| a renamed `row_center` lands every shell tree click 34 px high | red, blocker; blue confirmed | `row_center` reads the painted list (S3-D4); test 9b |
| test 7 proved nothing: the tab drag tests never move over a region | red, major; blue confirmed | test 7 asserts the flag |
| `on_mouse_move` is hitbox-gated, so a fast drag leaves a small region unmoved | blue | the move listens on the root (S3-D3); test 6's second case |
| test 6's fallback is dead: gpui re-raises the panic | red, minor; blue confirmed | `should_panic` |
| `window_control_area(Drag)` omitted | minor, inert on macOS | deviation row |
| the shot window keeps a native bar, so shots show two bars | blue; refuted: a shot PNG holds the gpui scene only, and today's shots start at y 0 | none |

### Out of S3

- Translucency (`WindowBackgroundAppearance::Blurred`) and panels drawn as
  cards: the next slice, S5 *(delegated)*.
- The tab hover card from the reference app.
- The drag preview's wrapper `div` and its `Role::Tab` outside the
  `TabList` (S1 levers): S3 does not touch the preview. They move to the
  next slice that does.
- Tooltips (S2d), resize (S2b), persistence (S2c).

## S2a. Panels: toggles, headers, `IconButton`

Specified 2026-10-02 under the user's delegation ("je te laisse gérer").
Choices taken under it are marked *(delegated)*.

S2 splits in two. **S2a** hides and shows the panels and gives them headers.
**S2b** resizes them, with a drag handle (Zed's `dock.rs:1275-1332`).
Persisting widths and visibility across launches is **S2c**: it writes the
session file that restore-tabs owns, so it gets its own slice and its own
review of that file's format *(delegated)*.

### What S2a changes

- `cmd-\` hides or shows the sidebar, and ⌘⇧\ the backlinks
  *(delegated)*. Neither key is bound today, and `cmd-b` stays free for bold.
  ⌘⇧\ is bound as `cmd-|`. macOS reports a shifted non-letter as its shifted
  character with shift cleared (`gpui_macos/src/events.rs:461-479`), as
  `keymap.rs` already does for `cmd-}`. A `cmd-shift-\` binding would pass a
  `simulate_keystrokes` test, which parses the string directly
  (`gpui/src/app/test_app.rs:383`), and never fire on a keyboard.
  Both actions appear in the command palette, which lists every bound action
  not on its hide list.
- Two icon buttons sit at the ends of the tab bar row. `panel-left` at its
  start toggles the sidebar; `panel-right` at its end toggles the backlinks.
  They sit in a new outer `h_flex` row: toggle, then the existing `#tab-bar`
  scrolling `div`, unchanged, then toggle. The scrolling `div`'s children
  stay "the tabs only", because `scroll_to_item` indexes them by child
  position (`tab_bar.rs:70`, `div.rs:4291-4309`). The `tab-bar` selector
  stays on the scrolling `div`.
  The row stays on screen when either panel is hidden, so each toggle stays
  reachable *(delegated: Obsidian's placement; Zed's is a status bar, which is
  S4)*. With no vault there are no panels, so there are no toggles
  (`VaultPanes`).
- Each panel gets a `PanelHeader`: the vault's folder name over the tree, and
  "Backlinks" over the backlinks. A header is exactly as tall as the tab bar,
  so the three columns' first rows line up. Today the backlinks header sits
  7 px lower than the tabs.

### S2a-D1. `IconButton`

Its two S2a sites are the two panel toggles, and S1's close button moves onto
it as the third. Rule 4 is met.

```rust
#[derive(IntoElement)]
pub(crate) struct IconButton {
    base: Stateful<Div>,   // id → debug_selector, as ListItem and Tab
    icon: IconName,
    label: SharedString,   // aria_label; required, an icon has no text
    toggled: bool,         // a panel shown: the icon at full strength
}
// new(id, icon, label), toggled(bool); Interactivity forwarded, no Styled
```

Render: `p_0p5`, `rounded_sm`, `Role::Button`, `aria_label(label)`, the
`button_hover_background` hover, and the `Icon` inside. The icon colour is
`icon_muted()` by default and `icon_active()` when toggled. `icon_active()`
is a new theme token, with `tab_text`'s value, so a panel toggle reads no
tab colour. That is `Icon`'s
first `.color()` builder (As built S0, 1).

`stop_propagation` stays at the close button's call site. The tab bar is the
reason for it, not the button (S1 levers).

### S2a-D2. `PanelHeader`

```rust
#[derive(IntoElement)]
pub(crate) struct PanelHeader { title: SharedString }
```

Render: `h_flex`, `flex_none`, `h(TAB_BAR_HEIGHT)`, `px(ROW_INSET)`,
`ui_text`, `text_color(placeholder_text())`, a `truncate` title, and the
same bottom rule as the tab bar: `border_b(RULE_WIDTH)` in `tab_rule()`.

`TAB_BAR_HEIGHT` is new in `theme.rs`: `UI_LINE_HEIGHT + RULE_WIDTH`,
23 px, which red measured as today's bar. The tab bar does **not** take it as
its height. The bar keeps its natural height, so the alignment test compares
a constant against a measured layout, not a constant against itself. Taffy's
boxes are border-box (`taffy-0.13.0/src/style/mod.rs:109`), so the header's
rule sits inside its 23 px, as the bar's does.

The sidebar's title is the vault root's folder name, read through `Vault`.
The backlinks title is "Backlinks". The backlinks panel drops its own header
`div` (`backlinks/render.rs`, the "Backlinks" `px_2 py_1` row) for it.

### S2a-D3. Showing and hiding

Superseded by the docks spec: `ToggleLeftDock`/`ToggleRightDock` on the same keys; `Docks` replaces `PanelsShown`; `PanelHeader` takes a left-press listener.

The panel is the sidebar or the backlinks; "hide" covers the key, the toggle
and the palette entry.

`AppShell` gains `PanelsShown { sidebar: bool, backlinks: bool }`, both true
at launch. A hidden panel renders nothing: its column is gone, and the note
column takes the width.

- **Focus.** Hiding the sidebar while the tree has focus would leave focus on
  an element no longer drawn. The toggle then lands focus where a tab switch
  does (`land_focus`). The backlinks panel takes no focus, so hiding it
  needs no landing.
- **State.** A hidden sidebar keeps its entity, its expanded folders and its
  selection. Showing it again shows them unchanged: the panel is not rebuilt.
- **An open name field is cancelled on hide** *(delegated)*. Hiding moves
  focus off the field, the way a click elsewhere does. That case is already a
  known defect: "a click elsewhere during a rename leaves the name field open
  without focus" (`docs/roadmap.md`). Hiding would also make the stranded
  field invisible. The hide therefore cancels the field through the path that
  `escape` takes. Cancel, not commit: a half-typed name must not create or
  rename a file. The roadmap item about the click stays open.
- **The modal.** The modal centres on the window, not on the note column. A
  hidden panel therefore moves no modal.

### S2a-D4. Levers taken

- **F stays deferred.** Renaming `UiText` to `StyledExt` would edit an import
  in seven files that S2a does not otherwise touch. A `muted()` helper would
  have one real site, `PanelHeader`, out of about twelve muted texts, and
  would leave two idioms for one look. F goes to the slice that sweeps the
  muted texts (Deferred, `Label`). `selectable` stays a free function.
- **D, second half.** `ListItem::role(Role)` emits `aria_selected(selected)`.
  The tree uses `Role::TreeItem`, and folder rows add `aria_expanded`, read
  from the same `expanded` test as the chevron.
- **G** and the shared slot layout stay deferred. `PanelHeader` has no start
  slot, so it is not the third slotted component.

### S2a theme

| Item | Value | Use |
|---|---|---|
| `TAB_BAR_HEIGHT` | `UI_LINE_HEIGHT + RULE_WIDTH`, 23 px | every `PanelHeader`; the bar keeps its natural height |
| `icon_active()` | #d0d0d0 | a toggled `IconButton`'s icon |
| icons `panel-left`, `panel-right` | Lucide 1.49.0, verbatim | the toggles |

### S2a tests

1. Keys, one `simulate_keystrokes` test each (CLAUDE.md):
   - `cmd_backslash_hides_and_shows_the_sidebar`: `backlinks-column` stays,
     the sidebar column goes, then comes back.
   - `cmd_pipe_hides_and_shows_the_backlinks`, simulating `cmd-|`, the
     keystroke macOS delivers for ⌘⇧\.
2. `hiding_the_focused_sidebar_lands_focus_on_the_note`.
2b. `the_tab_bar_still_scrolls_to_the_active_tab_with_toggles`: an existing
   scroll-into-view test is re-run with the toggles present. It fails if a
   toggle becomes a child of the scrolling `div`.
3. `a_shown_sidebar_keeps_its_open_folders_and_selection`.
4. `the_headers_line_up_with_the_tab_bar`: the bottom edges of
   `panel-header-sidebar` and `panel-header-backlinks` equal the measured
   bottom of `tab-bar`. The bar's height is not set from the constant, so a
   wrong `TAB_BAR_HEIGHT`, or a bar that grows, fails the test. The rule's
   colour is only checked in the shots.
4b. `hiding_the_sidebar_cancels_an_open_name_field`: start a new note, then
   hide the sidebar. No note is created, and showing the sidebar shows no
   field.
5. `a_click_on_a_toggle_hides_its_panel`, for each toggle.
6. `no_vault_shows_no_toggle`.
7. `ui/tests.rs`: the asset tests cover the two new icons.
8. The existing tests stay green unchanged. `test-names.sh` shows no lost
   test.
9. Shots, looked at:
   - `tabs`, `sidebar-vault` and `backlinks`, with headers;
   - new `panels-hidden`, both panels hidden;
   - new `toggle-hover`, the pointer over a toggle.

### Out of S2a

- Resizing (S2b) and persisting (S2c).
- Tooltips on the toggles. The first icon-only buttons have arrived, which is
  Deferred's trigger. But a tooltip needs a delay, a position and an
  `AnyView`, so it gets its own slice, S2d, before S3.

## S1. Tabs

Specified 2026-10-02. The user delegated the open choices ("je te laisse
gérer"). Each choice taken under that delegation is marked *(delegated)*, so
the user can revisit it.

### What S1 changes

- The tab and its drag preview become one `Tab` component. Today the preview
  restates the tab's look by hand (`tab_bar.rs:30-47` against `:96-163`).
- An inactive tab gets the row hover. The active tab keeps its opaque
  `tab_active_background`, which opens it into the editor below.
- The close glyph `×` becomes the Lucide `x` icon. It shows on the active tab
  and on any tab under the pointer *(delegated: Zed's and Obsidian's choice)*.
- A dirty note shows a dot. On a tab, the dot sits before the title, as in Zed
  *(delegated)*. On a tree row, it moves out of the name string into
  `ListItem`'s new end slot. A long name then clips before the dot, not the
  dot itself (lever A).

### S1-D1. The hover-or-selected rule, once (lever B)

`ListItem::render` holds the rule "a row carries the selected background or a
hover style, never both". `Tab` needs the same rule with a different selected
colour. It becomes one function in `ui.rs`:

```rust
/// One background or a hover, never both: a hover refinement repaints the base (`div.rs:3451`).
pub(crate) fn selectable(base: Stateful<Div>, selected: Option<Rgba>) -> Stateful<Div>
```

`ListItem` passes `selected.then(theme::selected_row_background)`. `Tab`
passes `active.then(theme::tab_active_background)`. The hover colour is
`row_hover_background` for both: an overlay, so it reads on the tab bar too.

### S1-D2. `Tab`

```rust
#[derive(IntoElement)]
pub(crate) struct Tab {
    base: Stateful<Div>,
    active: bool,
    start: Option<AnyElement>,   // the dirty dot, or nothing
    end: Option<AnyElement>,     // the close button, or nothing (the drag preview)
    children: Vec<AnyElement>,   // the title
}
// new(id), active(bool), start(e), end(e); ParentElement; Interactivity forwarded, no Styled
```

Render: `h_flex`, `flex_none`, `gap_2`, `max_w(TAB_MAX_WIDTH)`,
`px(ROW_INSET)`, `ui_text`, `selectable(.., active)`, and the text colour by
`active`. The title sits in a `min_w_0` `truncate` cell. The start and end
slots are `flex_none`.

`Tab` also sets `.group(TAB_GROUP)`. The end slot reads that group to show on
hover (S1-D3). gpui resolves a group name to the nearest enclosing element
with that name, so one constant serves every tab.

The two sites:

- **The bar.** `render_tab` builds `Tab::new(("tab", ix))` and attaches every
  handler it has today, unchanged: both mouse-downs, click, aux-click, drag,
  drag-over, drop. `debug_selector` comes from the id, as in `ListItem`, so
  `tab-{ix}` is unchanged.
- **The preview.** `DraggedTab::render` builds `Tab::new("tab-preview")` with
  the same title and active flag, and no end slot. Today's comment "drawn at
  the window root, so it inherits nothing" stays true: `Tab` states its own
  text size and colour.

**Accessibility (lever D).** The tab gets `Role::Tab` and
`aria_selected(active)`, and the bar gets `Role::TabList`. Both methods exist
on `StatefulInteractiveElement` (`div.rs:1304,1388`). `Tab` sets the role and
the selection itself, so the two cannot drift. `docs/gpui-accessibility.md`
records that gpui's bridge is widget-level only. These are widget properties,
so they reach it. `ListItem::role` is left to S2, when the tree's
`aria_expanded` can come from the same `expanded` test as the chevron.

### S1-D3. The close button: shown on hover, never hidden from the pointer

The close button is a `div` with an id, holding `Icon::new(IconName::X)`, with
today's `on_click` and `stop_propagation`. On an inactive tab it has
`opacity(0.)` and `.group_hover(TAB_GROUP, |s| s.opacity(1.))`.

**Why opacity and not `invisible()`.** A div with `Visibility::Hidden` returns
before it registers its mouse listeners (`div.rs:2529`). An invisible ×
would therefore be unclickable until a frame had drawn the hover. The tests
that click `tab-close-0` and `tab-close-1` would depend on that frame
(`shell/tests/tab_bar.rs:127,156,355`, `dragging.rs:188`). Opacity keeps the
button laid out, painted and clickable. The tab's width also stays the same
whether or not the × shows.

The button gets the button hover: `button_hover_background()`, a white
overlay stronger than the row's, so the × reads as a target within a hovered
tab. It also gets `rounded_sm` and a 2 px padding around the 14 px icon. The
padded box is 18 px, which fits the 22 px line box, so the tab's height does
not change.

This is not `IconButton` yet. It has one site in S1, the tab, so rule 4
places the component in S2 with the panel toggles. S2 moves this code into
`IconButton` unchanged.

### S1-D4. `Indicator`: the dirty dot

```rust
#[derive(IntoElement)]
pub(crate) struct Indicator;   // a dot: DOT_SIZE square, rounded_full, bg(theme::dirty_dot())
```

There are two sites:

- **The tab's start slot**, when its note is dirty. `tab_titles` keeps its
  signature: three tests compare its result to plain titles
  (`shell/tests/tab_bar.rs:20`, `tabs.rs:461`, `failed_trash.rs:423`). The
  note each tab shows moves into `tab_notes()`. `tab_titles` and
  `render_tab_bar` both read it, and the bar asks `is_dirty` per note.
- **`ListItem`'s new `end` slot** (`flex_none`, after the content), on a dirty
  tree row.

`row_label` loses its ` •`. The sidebar test that reads the dot
(`sidebar/tests.rs:424-439`, through `row_labels`) moves to the state it
names. A `row_is_dirty(sidebar, row)` helper reads `is_dirty` exactly where
the render reads it, so the test keeps its name and its three asserts.

The dot reads `NoteStore::is_dirty(id, cx)` (`vault/store.rs:129`) inside the
draw, as the label does today. The comment "reading inside the draw is what
repaints the dot" moves with it.

**Why a save with no keystroke clears the dot.** Inkra caches no view: there
is no `.cached(` in the crate. So every `render` runs on every window draw,
and the gated reuse in `gpui/src/view.rs:481-488` never applies. A save
notifies `NoteStore`. The draw read that entity, so the window redraws
(`gpui/src/app.rs:1210,2790`). Blue confirmed this with a throwaway test
(Red/blue, S1). A view that adopts `.cached()` must notify on `NoteSaved`
itself.

### S1-D5. Icons (lever E)

`x` is the third icon. `assets.rs` becomes one `macro_rules! icons`, which
generates the enum, `ALL`, `path` and `bytes` from one list of
`Variant => "file-stem"` pairs. A variant can then no longer be missing from
`ALL`. The three asset tests stay as they are.

### S1-D6. Theme

| Item | Value | Use |
|---|---|---|
| `button_hover_background()` | white overlay `0x1f` | an icon button under the pointer |
| `dirty_dot()` | `tab_inactive_text`'s #8a8a8a | the dirty `Indicator` |
| `DOT_SIZE` | 6 px | `Indicator` |
| `TAB_GROUP` | `"tab"`; a name, not a colour, but it binds `Tab` to its end slot, so it lives beside `Tab` in `ui/tab.rs`, not in `theme.rs` | — |

### S1 tests

The success criteria are written first. Each one fails on the current tree.

1. `ui/tests.rs`: the three asset tests now cover `x`. A new
   `every_icon_has_a_distinct_path` replaces what the hand-written `ALL` used
   to guard.
2. `shell/tests/tab_bar.rs`:
   - `an_inactive_tabs_close_button_closes_it_without_a_hover`: a click on
     `tab-close-1` with no prior mouse move closes tab 1. It fails if the ×
     is `invisible()`.
   - `a_dirty_note_shows_a_dot_on_its_tab`: edit the note in tab 0, and
     `tab-dot-0` is drawn. Save it, and the dot is gone.
   - `the_tab_keeps_its_width_when_hovered`: the bounds of `tab-1` are equal
     before and after the pointer moves onto it.
   - `the_preview_matches_the_tab`: the bounds of the dragged preview and of
     its tab have equal height. No test here reads bounds mid-drag yet. The
     drag start calls `window.refresh()` (`div.rs:2999`), so the preview
     should be in the next frame. If it is not reachable, render
     `DraggedTab` alone in a `VisualTestContext` and compare.
3. `sidebar/render_tests.rs`:
   - `a_dirty_rows_dot_survives_a_long_name`: at the 220 px column, a note
     with a 200-character name and an edit has a drawn `tree-dot-{item}`
     whose right edge is inside the row.
   - `a_saved_notes_dot_leaves_the_frame`: the dot is drawn after an edit, and
     gone after `save_current` and `run_until_parked`, with no further input.

   **Measure edges, not widths.** A child of `ListItem`'s content cell is a
   block, so it fills the row whatever its text. A width taken there reads
   the row, not the text.
4. The existing tab tests stay green unchanged: click, middle-click, close,
   drag, reorder, restore. So does `tab_titles` and its three callers.
   `test-names.sh` shows no lost test.
5. Shots, looked at:
   - `tabs`: an × on the active tab only, dots where notes are dirty;
   - new `tabs-hover`: the pointer over an inactive tab, so its hover
     background and its × show;
   - `tabs-dragging`: the preview is unchanged;
   - `sidebar-dirty`: the dot sits at the row's right edge.

### Out of S1

- `IconButton`, which S2 adds at its second site.
- The tab's ⌘-hover tooltip, and tooltips in general (Deferred).
- Swapping the dot for the × on hover, as Obsidian does. It would need two
  states in one slot, while the dot-before-title layout needs none.

## Open questions for the user

1. **Note icons.** Should a note row show a file icon, as in Zed, or a chevron
   for folders only, as in Obsidian? This spec assumes chevrons only, with a
   spacer on notes.
2. **Panel shortcuts (S2).** Answered in S2a: `cmd-\` for the sidebar and
   `cmd-|` (⌘⇧\) for the backlinks; `cmd-b` stays free for bold.
3. **Panel widths and visibility (S2).** Answered: they persist, in S2c,
   which writes the session file that restore-tabs owns.
4. **Status bar (S4).** What should it show: save state, indexing progress,
   word count, caret line?
5. **Light theme.** Is it on the roadmap? It is the trigger for D3's theme
   global.

## As built (S0)

S0 shipped on `feat/shell-chrome-s0`. Each item below deviates from the text
above or fills a gap in it.

1. **`Icon` has no `.size()` or `.color()` builder in S0.** No S0 call site
   sets either, and an unread builder fails `-D warnings`. `Icon` holds only
   its name and renders at `ICON_SIZE` in `icon_muted()`. The builders land
   with their first caller.
2. **`ListItem::new` sets `debug_selector` from its id**, as Zed's `Tab::new`
   does: `("tree-row", 3)` reads `tree-row-3`. `the_field_row_has_the_tree_rows_height`
   measures the row, and the field row has no name element to measure.
3. **`ListItem` does not implement `Styled`; wrapping and colour go on the
   content.** A caller cannot set a row's height, padding or direction. The
   tree's name `div` carries `whitespace_nowrap` and the text colour; the
   picker wraps its delegate's row in a `whitespace_nowrap` `div`.
4. **The field row's start slot is the note spacer.** The text typed then
   starts where the names at its depth start, so a rename field sits on the
   name it replaces.
5. **`selected` picks the style with an `if`, not a `match`.** Clippy's
   `match_bool` rejects a `match` on a `bool`. A row still carries one style
   or the other, never both.
6. **The tests sit in their own files.** The sidebar geometry tests are in
   `sidebar/render_tests.rs`, because `sidebar/tests.rs` is past 400 lines.
   `backlink_excerpts_still_wrap` is in `shell/tests/backlinks.rs`, because it
   needs the shell's 260 px column.

## As built (S1)

S1 was built on `feat/shell-chrome-s1`. Each item below deviates from the S1
text above or fills a gap in it.

1. **The new tab tests are in `shell/tests/tab_chrome.rs`, not `tab_bar.rs`.**
   `tab_bar.rs` holds 373 lines, and five more tests would carry it past 400.
   `tab_bar.rs`'s `click` helper became `pub(super)` for the new file.
2. **A tab's title cell has a minimum width, `theme::TAB_MIN_WIDTH` (18 px).**
   Without it, a one-letter tab's centre lies on the ×'s 18 px box: `a` is
   8 px of padding, about 8 px of text and an 8 px gap, so the box spans
   about 24 px to 42 px and the centre sits near 25 px. A centre click then
   closed the tab, the active one too. `clicking_a_tab_activates_it` failed,
   and `clicking_the_active_tab_focuses_its_editor` passed while it closed
   the only tab. A tab is `2 × ROW_INSET + title + gap + ×`, and its centre
   lies left of the × when the title is wider than the × less the gap. A
   title cell as wide as the × keeps the centre a gap's width clear. Both
   tests click the centre, as before S1. The second now also asserts the
   tab list, and `a_click_on_a_short_tabs_centre_never_closes_it` fails with
   `TAB_MIN_WIDTH` at 0.
3. **The drag preview paints `tab_active_background` behind its `Tab`.** An
   inactive `Tab` has no background, only a hover, and gpui applies no hover
   while a drag is active (`div.rs:3431`). A preview built from `Tab` alone
   would be bare text at the window root when an inactive tab is dragged. The
   preview before S1 always painted that background, so a wrapper `div` keeps
   it. The `tabs-dragging` shot shows the preview unchanged.
4. **The `tabs` scene types into the active `index`, not into an inactive
   tab.** A note left saves at once (`NoteStore::set_current`), so an inactive
   tab shows the dot only while its save is in flight or has failed. The
   scene shows the dot, the title and the × on one tab. `tabs-hover` and
   `tabs-dragging` open the clean bar.
5. **The dot's debug selectors sit on a wrapper `div` at each call site:**
   `tab-dot-{ix}` and `tree-dot-{item}`. `Indicator` stays a unit struct with
   no id and no interactivity.
6. **`row_label` is gone.** `render_row` calls `one_line(&row.name)` for the
   label, and the `Sidebar` method `row_is_dirty` for the dot. The test helper
   `dirty_rows` lists the names of the rows that `row_is_dirty` marks, so the
   three asserts read none, `b`, none.
7. **The ×'s 2 px padding is `p_0p5()`**, 0.125 rem at gpui's 16 px rem. The
   bar's other spacings are rem helpers too (`px_2`, `gap_2`).
8. **The bar's `text_size` and `line_height` pair became `ui_text()`**, as D1
   asks of the files a slice touches.
9. **`tab_notes()` is `pub(super)` and returns an iterator.** `tab_titles`,
   `render_tab_bar` and the shell tests' `tab_notes` helper read it.
10. **`a_saved_notes_dot_leaves_the_frame` was seen red only on the tree
    before S1**, where no `tree-dot` exists. Its named bug, a cached view,
    cannot be injected in its window: the window's root is the `Sidebar`
    itself. Every other S1 test was also seen red with its named bug injected.
11. **Deferred drops its `ListItem` end slot row**, because S1 built the slot
    for the dirty dot.
12. **`Tab` hides its own end slot on an inactive tab**, with the opacity and
    the `group_hover` of S1-D3, on the slot's `flex_none` wrapper. The bar's
    close button carries no visibility rule, and `TAB_GROUP` is private to
    `ui/tab.rs`.
13. **The close button has `Role::Button` and the label "Close tab"**, found
    by `/code-review`.

## As built (S3)

S3 was built on `feat/shell-chrome-s3`. Each item below deviates from the
S3 text above or fills a gap in it.

1. **`tab-row` has no left padding.** The leading region is the row's inset,
   so a press at x 0 moves the window. The row keeps S2a's 4 px (`pr_1`) at
   its right end only (As built S2a, 15).
2. **The body under the row has a definite width (`w_full`).** Without it
   taffy measures the row's content, and a keystroke shaped the note twice:
   `a_keystroke_in_a_narrow_pane_shapes_the_note_once` read 2 against 1.
3. **The drag preview is as tall as a tab in the bar.** Its wrapper `div` is
   `flex` and `TITLE_BAR_HEIGHT - RULE_WIDTH` tall, so its `Tab` stretches as
   the bar's do. With the bar's tabs stretched and the preview unchanged,
   `the_preview_matches_the_tab` read 22 against 33. This touches the preview
   that "Out of S3" leaves alone; the two S1 levers on it (the wrapper `div`,
   `Role::Tab` outside the `TabList`) stay with the next slice.
4. **`many_tabs_never_widen_the_note_column` drops its bar assertion.** The
   bar now ends past the backlinks column's left edge by design. Its note
   column and backlinks column assertions stay. Test 4 asserts that the
   backlinks toggle stays inside the window.
5. **`Sidebar::row_center` returns the list's centre x**, as
   `Picker::row_center` does, and callers no longer pass x 20.
   `a_click_reports_the_row_it_landed_on` still guards `w_full`: an `auto` row
   ends at 17.6 px, far left of the centre. The pitch stays `UI_LINE_HEIGHT`,
   not the list's `last_item_size`, so
   `a_click_far_down_the_list_lands_on_the_row_it_names` still catches a drift.
   Shell tests reach it through `row_point`, which reads the window's root;
   scenes through `Stage::tree_row_center`. It stays
   `#[cfg(any(test, feature = "shot"))]`: nothing else reads it.
6. **Test 6's second case is its own test**,
   `a_fast_drag_off_a_drag_region_moves_the_window`: a `should_panic` test
   holds one panic. It failed with the move listener on the region. Test 7
   failed with the press armed on the whole row, test 5 without
   `prevent_default`.
7. **Test 3 also asserts that `#tab-bar` ends at its last tab**, so it names
   the cause when the bar grows. With the bar still `flex_1`, it read 997
   against 257.
8. **The note column has the selector `note-column`**, for tests 1 and 8.
   Test 8 measures it, not the empty state, which fills it.
9. **Zed sets `app_owns_titlebar_drag` at `zed/src/zed.rs:408`**, next to
   its `(9, 9)`. S3-D1's `zed.rs:1712` is the About window, at `(12, 12)`.
10. **A release outside the window disarmed nothing.** The root's up and move
    listeners need its hitbox hovered (`div.rs:217-219, 309`). So a press on a
    region and a release outside the window left the flag set, and the next
    plain move over the window started a move with no button held.
    `title_drag_move` now disarms on every move and starts the move only when
    `pressed_button` is the left button.
    `a_release_outside_the_window_leaves_no_move_armed` failed first, with the
    test platform's `not implemented` panic.
11. **`row_center`'s scroll offset is tested.**
    `a_click_on_a_scrolled_tree_lands_on_the_row_it_names` scrolls 60 notes to
    row 50 in a 300 px window and clicks there. With the offset term deleted,
    nothing was selected. The sidebar's and the picker's `row_center` share one
    helper, `ui::list_row_center`.

## As built (S2a)

S2a was built on `feat/shell-chrome-s2a`. Each item below deviates from the
S2a text above or fills a gap in it.

1. **The background and the rule move from `#tab-bar` to a new outer row,
   `tab-row`.** A rule on the scrolling `div` alone stops short of both
   toggles. `#tab-bar` keeps its id, its `Role::TabList`, its scroll and its
   tabs-only children, and becomes `flex_1` inside the row. So
   `the_headers_line_up_with_the_tab_bar` measures the bottom of `tab-row`,
   where the rule is. A wrong `TAB_BAR_HEIGHT` fails it (22 px read 22 against
   23), and so does a row grown by a larger toggle (`p_1p5` read 23 against 27).
2. **Each header is drawn by its panel's own view**, `Sidebar` and
   `BacklinksPanel`, not by the shell. The sidebar reads its title through
   its own `Vault`. The backlinks' scroll moves to an inner `div` under the
   header, so the header does not scroll away.
3. **`row_center` adds `TAB_BAR_HEIGHT`.** The header moves every tree row
   down by 23 px. The tests and scenes that click a row call `row_center`, so
   their bodies stay unchanged.
4. **`PanelHeader::new(selector, title)` is a plain `Div` with a debug
   selector and no id**: `panel-header-sidebar`, `panel-header-backlinks`. It
   has no interactivity, so it needs no id. `ListItem`, `Tab` and `IconButton`
   build their root with one `ui` helper, `identified(base, id)`, which sets
   the debug selector from the id.
5. **`PanelsShown` lives in `VaultPanes`**, as `panes.shown`. With no vault
   there is no panel, so there is no state to hold.
   Superseded by the docks spec: `ToggleLeftDock`/`ToggleRightDock` on the same keys; `Docks` replaces `PanelsShown`; `PanelHeader` takes a left-press listener.
6. **`TAB_BAR_HEIGHT` is the literal `px(23.)`.** `px` has no const add. Its
   doc names the sum, and test 4 catches a drift.
7. **`IconButton` calls `prevent_default` on a left mouse-down**, as a tab's
   press does. Without it gpui's root takes focus on the press
   (`div.rs:2765-2780`), and a click on a toggle unfocuses the editor.
   `a_click_on_a_toggle_hides_its_panel` asserts that the editor keeps focus.
   The close button gets it too; the tab's own press already prevents it.
8. **`ListItem` has no `role` of its own and emits no `aria_selected`.** The
   tree's call site sets `.role(Role::TreeItem).aria_selected(selected)`
   through the trait, and `aria_expanded` on folders. The field row is a
   `Role::TreeItem` too. The picker row is unchanged from S1: `Role::ListItem`,
   no `aria_selected`. An inherent `ListItem::role` was tried and dropped: it
   shadowed the trait method and changed the picker's output silently.
9. **`Role::Tree` sits on the sidebar's root**, which gains the id `sidebar`
   and an `aria_label`, the vault's title.
   gpui's `UniformList` is not a `StatefulInteractiveElement`, so it takes no
   role. Zed puts `Role::Tree` on the list's parent too
   (`settings_ui.rs:3211`). The header is a child of that node, but it has no
   role, so it is no node of its own.
10. **With no vault, the sidebar column stays drawn empty**, as before S2a.
    Only a vault's sidebar hides.
11. **`IconButton` is `flex_none`**, so a toggle keeps its 18 px in the row.
12. **The sidebar's title is the root's folder name**, or the whole path for a
    root with none, such as `/`. `Sidebar::new` computes it once; the root
    never changes.
13. **Test 2b asserts the toggles lie outside `#tab-bar`'s bounds**, then that
    `tab-29` lies inside them after `cmd-9`. With both toggles moved into the
    scrolling `div`, the existing `a_tab_activated_past_the_bar_edge_scrolls_into_view`
    stayed green, so 2b is the only guard.
14. **The tests are in `shell/tests/panels.rs`.** `toggle-hover` hovers the
    point just left of the bar's drawn bounds, centred on the bar
    (`AppShell::sidebar_toggle_point`). The toggle ends where the bar begins.
    The shot runner cannot read `debug_bounds`, because gpui keeps the
    rendered frame `pub(crate)`.
15. **`tab-row` has a 4 px (`px_1`) inset at each end**, so a toggle does not
    touch the column's edge. It is the row's padding; no element special-cases
    it.
16. **`cmd-n` and `cmd-shift-n` show a hidden sidebar first**, as Obsidian
    reveals its tree *(delegated)*. Without it, `create` opened and focused a
    name field that was not drawn, so every keystroke went out of sight.
    `creating_with_the_sidebar_hidden_shows_it` failed before the fix: the
    sidebar column was not drawn after `cmd-n`.
17. **`IconButton::toggled` is an `Option<bool>`.** `Some` emits `aria_toggled`;
    the icon is at full strength only for `Some(true)`. The close button is
    `None`, a plain button.
18. **`cancel_edit` notifies only when it closed a field.** `escape` always
    has one; a hide often has none.
19. **A pre-existing defect, fixed: the palette drew `cmd-|`, `cmd-}` and
    `cmd-{` as ⌘|, ⌘} and ⌘{.** macOS reports a shifted symbol with shift
    cleared, so the binding holds no ⇧. `key_label` now maps each such symbol
    to ⇧ and the key pressed, on a US layout: ⇧⌘\, ⇧⌘] and ⇧⌘[.
    `a_shifted_symbol_draws_as_shift_and_its_key` failed first (`⌘|`).

## Deferred

| Item | Trigger |
|---|---|
| `Disclosure` | a toggle that is not the row's own action |
| `Divider` | a second drawn hairline (one today: vault search's group rule) |
| `Label` with size, colour and truncation (lever F) | a slice that migrates the ellipsis cells and muted texts; 6 and 12 sites today, left out of S0 to keep it reviewable |
| `Theme` global | a second theme or a size setting |
| Tooltips | S2d: the toggles are the first icon-only buttons with no visible label |
| `ui` as its own crate | a second consumer |
| The Lucide licence shown in the app (About, Acknowledgements) | the first distributed build; none exists, so the licence file in the source tree is enough today |

## Red/blue review, 2026-10-01

Red (Sonnet) attacked the draft. Blue (Sonnet) verified each claim from source
and reviewed the fixes. Findings folded in:

| Finding | Verdict | Change |
|---|---|---|
| `ListItem`'s `nowrap` cascades into the backlink excerpts, which wrap today | confirmed, blocker | the row does not decide wrapping (D4); test `backlink_excerpts_still_wrap` |
| `("tree-row", EntryId)` does not compile: private `u32`, orphan rule | confirmed, blocker | positional ids, as tabs and picker (D4) |
| `Icon` has one call site, against rule 4 | confirmed | named as the one exception (decision 4) |
| Theme functions return `Rgba`, not `Hsla` | confirmed | builders take `impl Into<Hsla>` (D3) |
| Hover on a selected row repaints it in the hover colour | found by blue | selected rows carry no hover style; shot `sidebar-hover-selected` |
| The tree's label is a bare `String`; no element to measure | found by blue | a wrapper `div` with `tree-name-{item}` (D4) |
| `render_rows` returns `Vec<Div>` | found by blue | `Vec<AnyElement>` (D4) |
| S3 needs S1's `Tab`; S4's order against S3 unstated | confirmed, minor | Slices table |
| Licence in a shipped `.app` | moot today: no bundling | Deferred |
| S1: the dirty dot stays after a save that lands with no keystroke | refuted by blue with a running test: no view is cached, so every render reruns | S1-D4 records why; the test is kept as `a_saved_notes_dot_leaves_the_frame` |
| S1: lever D's row overstated S1's scope | confirmed | the levers table |
| S1: `tab_titles` returning tuples breaks three tests | found by blue | `tab_notes()`; `tab_titles` unchanged (S1-D4) |
| S1: the icons macro is premature at three icons | blue: keep; it prevents a variant missing from `ALL`, which no test catches | kept (S1-D5) |
| S1: a mid-drag bounds read has no precedent | found by blue | fallback named in S1 tests |
| S2a: `cmd-shift-\` never fires on macOS; the test would pass anyway | confirmed, blocker (precedent `keymap.rs:87`) | bound as `cmd-|` |
| S2a: toggles inside `#tab-bar` break `scroll_to_item`'s indexing | confirmed, blocker | an outer row; test 2b |
| S2a: the alignment test compared a constant with itself | confirmed | the bar keeps its natural height |
| S2a: lever F was a drive-by across seven files, and `muted` had one real site | confirmed | F deferred |
| S2a: hiding strands an open name field out of sight | confirmed | the hide cancels it; test 4b |
| S2a: `tab_text` read by a panel toggle | minor | `icon_active()` |
| S2a as built: `cmd-n` with the sidebar hidden opens and focuses an undrawn name field | confirmed by `/code-review`, the quality review and the spec review; blocker | `create` shows the sidebar first (As built S2a, 16); test `creating_with_the_sidebar_hidden_shows_it` |
| S2a as built: an inherent `ListItem::role` shadows the trait's, and changes the picker's output | red, major | dropped; the tree sets its role and selection at the call site (As built S2a, 8) |
| S2a as built: the palette draws ⌘⇧\ as ⌘\| | red, minor; pre-existing for `cmd-}` and `cmd-{` | `key_label` maps shifted symbols (As built S2a, 19) |
| S1 as built: a one-letter tab's centre lies on its ×, so a centre click closes it, the active tab too; `clicking_the_active_tab_focuses_its_editor` passed while it closed the only tab | confirmed by red, with the built tests reverted; blocker | `TAB_MIN_WIDTH` on the title cell (As built S1, 2); the test asserts the tab list; new `a_click_on_a_short_tabs_centre_never_closes_it` |
| `ListItem` implemented `Styled`: a caller could set its height, padding or direction | confirmed, quality review and red | `Styled` dropped; wrapping and colour on the content (As built 3) |
| Picker hover invisible on #252526: an opaque #232323 is darker than the modal | found in the `picker-hover` shot | overlays: hover and selected are white at alpha `0x0c` and `0x17` (D3) |

Claims both teams failed to break include:

- interactivity forwarding through a `Stateful<Div>` (`div.rs:768,1300`);
- `.role`, `.aria_label` and `.debug_selector` reaching the picker row through
  `ListItem`;
- `with_assets` and `with_asset_source`;
- an icon keeping its layout box with no asset source;
- `debug_bounds` in tests;
- `items_center` changing no row height.
