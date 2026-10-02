# Shell chrome: components, icons, panels, title bar

Status: S0 shipped 2026-10-02 (#57). S1 built 2026-10-02 on `feat/shell-chrome-s1`. S2–S4 pending.

## Why

The engine is done: index, search, watcher, tabs and restore all ship. The chrome around it is not. Rendered on 2026-10-01 (`tabs`,
`sidebar-vault`, `quick-open-results`):

- A folder row and a note row look the same: no chevron, no icon. Depth is the
  only cue.
- Nothing reacts to the pointer. The crate has no `.hover(`, no `.active(`, no
  `cursor_pointer`, no tooltip.
- The tab's close button is the text glyph `×`. There is no icon anywhere.
- The sidebar and backlinks are fixed at 220 and 260 px and cannot be hidden.
- The system title bar costs a row, and the backlinks header sits 8 px lower
  than the tab bar beside it.

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
| S2 | Panels: `IconButton` (its second site after the tab's close button); hide and show the sidebar and backlinks, with a resize handle (Zed's `dock.rs:1275-1332` pattern: `deferred`, `on_drag` + `on_drag_move` on the shell); a panel header shared by sidebar (vault name) and backlinks; aligned header heights | `IconButton`, `PanelHeader` | S1 |
| S3 | Title bar: `appears_transparent`, traffic-light padding, the tab bar on the title row, window drag area | — | S1 (`Tab`), S2 (header heights) |
| S4 | Status bar | — | S2 (panel layout); free against S3 |

### Carried to later slices

The S0 quality review's levers, each taken up by the slice named:

| Lever | Slice | Change |
|---|---|---|
| A | S1 | `ListItem`'s end slot holds the dirty `Indicator`; the ` •` no longer clips on a long name |
| B | S1 | the hover-or-selected rule becomes one `ui` helper when `Tab` lands, its second user |
| D | S1, S2 | S1: `Role::Tab`, `Role::TabList` and `aria_selected` on `Tab`. S2: `ListItem::role` and the tree's `aria_expanded` |
| E | S1 | an icons `macro_rules!` at the third icon |
| F | S2 | `UiText` becomes `StyledExt` with a muted variant, or a `Label` |
| G | S2 | `ListItem` owns the start slot's width when a second fixed-slot user appears |

The S1 reviews' levers:

| Slice | Change |
|---|---|
| S2 | `IconButton` owns its debug selector, padding, hover, `Role::Button` and label; `stop_propagation` stays at the call site |
| S2, or the third slotted component | one shared slot layout and slot selectors for `ListItem` and `Tab` |
| S2, with lever F | `selectable` becomes a chained builder |
| S3 | the drag preview: its wrapper `div` (As built S1, 3; quality review lever 4), and its `Role::Tab` outside the `TabList` (`/code-review`) |
| the next per-tab field | one per-tab struct in `render_tab_bar`, in place of zipped vectors |
| a second control shown on hover | a show-on-hover helper |

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
2. **Panel shortcuts (S2).** `cmd-b` is Zed's sidebar toggle, but it is bold in
   every markdown editor. Proposal: `cmd-\` for the sidebar and
   `cmd-shift-\` for backlinks. Neither is bound today.
3. **Panel widths and visibility (S2).** Should they persist across launches,
   in the session file that restore-tabs writes?
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

## Deferred

| Item | Trigger |
|---|---|
| `Disclosure` | a toggle that is not the row's own action |
| `Divider` | a second drawn hairline (one today: vault search's group rule) |
| `Label` with size, colour and truncation | a slice that migrates the ellipsis cells and muted texts; 6 and 12 sites today, left out of S0 to keep it reviewable |
| `Theme` global | a second theme or a size setting |
| Tooltips | the first icon-only button with no visible label (S2's toggles) |
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
