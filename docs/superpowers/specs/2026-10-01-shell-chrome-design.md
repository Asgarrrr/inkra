# Shell chrome: components, icons, panels, title bar

Status: S0 built 2026-10-01 on `feat/shell-chrome-s0`; S1–S4 pending.

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
crates/app/src/ui.rs            // pub(crate) re-exports, h_flex, v_flex, UiText
crates/app/src/ui/assets.rs     // Assets: AssetSource, IconName
crates/app/src/ui/icon.rs       // Icon
crates/app/src/ui/list_item.rs  // ListItem
crates/app/src/ui/tests.rs
crates/app/assets/icons/*.svg   // Lucide, verbatim
crates/app/assets/icons/LICENSE // Lucide ISC + Feather MIT, verbatim
```

Everything is `pub(crate)`. `ui` depends on `theme` and gpui only. It never
reads a view, an entity or a vault type, so the dependency points one way:
views → `ui` → `theme`.

`ui.rs` holds three helpers that every component and view migrated by a
slice uses:

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
    end: Option<AnyElement>,      // count, key hint; S0 has no user, so not built (rule 4)
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
| `IconName` by strum | a hand-written `match` and `ALL` | no dependency for a handful of variants; tests keep it in sync |
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
| S1 | Tabs: a `Tab` component shared by the bar and the drag preview (2 sites, `tab_bar.rs:30-47,96-163`); `IconButton` with Lucide `x`, shown on the active tab and on hover; a hover background; the dirty dot as an `Indicator` on tab and tree row | `Tab`, `IconButton`, `Indicator` | S0 |
| S2 | Panels: hide and show the sidebar and backlinks, with a resize handle (Zed's `dock.rs:1275-1332` pattern: `deferred`, `on_drag` + `on_drag_move` on the shell); a panel header shared by sidebar (vault name) and backlinks; aligned header heights | `PanelHeader` | S1 (`IconButton`) |
| S3 | Title bar: `appears_transparent`, traffic-light padding, the tab bar on the title row, window drag area | — | S1 (`Tab`), S2 (header heights) |
| S4 | Status bar | — | S2 (panel layout); free against S3 |

### Carried to later slices

The S0 quality review's levers, each taken up by the slice named:

| Lever | Slice | Change |
|---|---|---|
| A | S1 | `ListItem`'s end slot holds the dirty `Indicator`; the ` •` no longer clips on a long name |
| B | S1 | the hover-or-selected rule becomes one `ui` helper when `Tab` lands, its second user |
| D | S1 | `aria_selected` and `aria_expanded` through `ListItem::role`, checked against `docs/gpui-accessibility.md` |
| E | S1 | an icons `macro_rules!` at the third icon |
| F | S2 | `UiText` becomes `StyledExt` with a muted variant, or a `Label` |
| G | S2 | `ListItem` owns the start slot's width when a second fixed-slot user appears |

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

## Deferred

| Item | Trigger |
|---|---|
| `Disclosure` | a toggle that is not the row's own action |
| `Divider` | a second drawn hairline (one today: vault search's group rule) |
| `Label` with size, colour and truncation | a slice that migrates the ellipsis cells and muted texts; 6 and 12 sites today, left out of S0 to keep it reviewable |
| `ListItem` end slot | its first user: vault search's count or the palette's key hint, when their slice moves them |
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
