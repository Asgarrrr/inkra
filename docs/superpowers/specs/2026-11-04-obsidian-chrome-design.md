# Obsidian chrome: surfaces, split title row, browser tabs

Status: **built 2026-11-04**; approved 2026-11-04 in a brainstorm on HTML mocks.

## Why

The S3 chrome reads flat: the sidebar, the tab bar and the backlinks panel
share one shade with the note, and the headers look like body text. The user
works daily in Obsidian and likes its look. Two earlier directions were
rejected: islands on dark glass, and a `#262626` frame with the note recessed
(`feat/shell-chrome-s5`, dead). This spec takes the user's Obsidian as the
reference instead of inventing a look.

References, both gitignored:

- `target/design-lab/obsidian-reference.png` — the user's Obsidian window.
- `target/design-lab/obsidian-chrome-mock.html` — the approved mock, panels
  shown and hidden.

## Decisions taken

- **D1. The note is the darkest surface.** The chrome (sidebar, title row,
  backlinks) is lighter and warm. This inverts S3, where the note is lighter.
- **D2. The title row splits in two.** Over the sidebar: the traffic lights,
  `SearchVault` and `NewNote` buttons, then the sidebar toggle at the
  sidebar's right edge. Over the note column: the tabs, a `+` (`NewTab`), the
  drag space, then the backlinks toggle at the right end.
- **D3. Tabs look like browser tabs.** The active tab has the note's colour, a
  hairline on top and sides, 10 px top corners, and joins the note with no rule
  under it. An inactive tab has muted text and no background.
- **D4. The history arrows start each tab strip.** `←` `→`, bound to `GoBack`
  and `GoForward`, sit before a centre group's first tab, outside the strip's
  scroll. An arrow with no place to reach is dimmed. A right-click or a hold on
  either arrow opens the tab's history menu. Revised 2026-11-06: the 34 px nav
  row under the tabs is gone, and the note gains its height.
- **D5. The sidebar loses its `vault` header.** Rows grow to 27 px, rounded.
- **D6. Backlinks keeps a plain title header**: `Backlinks` and a count, not an
  icon bar. A later dock spec shows a title for a dock with one view and an icon
  bar for several; this header is drawn to fit that rule.

Out of scope: movable views (Zed's `Dock` and `Panel`, the next spec), the note
body's styling (headings, code blocks: markdown blocks S2), a tab list menu.

## D1. Surfaces

Read from the reference's theme variables (`obsidian eval`, Flexoki dark).
The reference's chrome is translucent: `#100f0f` at 70 % over the window's
vibrancy, so its colour on screen depends on the desktop behind it. Inkra does
the same on the user's request (2026-11-04): the window is
`WindowBackgroundAppearance::Blurred` and the chrome `#181717` at 85 %. It was
95 % first, a hint of the desktop without discerning it; on 2026-11-08 the user
found the translucency barely visible, and 65 % was chosen from captures at 85,
75 and 60 %; later that day 65 % read too transparent, then 75 % and 80 %, and
85 % replaced them. The light theme stays at 95 %. The base was matched by the user's
eye beside the reference: `#201f1f`, tuned to the reference's measured
`#2a2929`, read too light. The note and the active tab stay opaque. Compare
on a full-screen capture: a one-window capture (`screencapture -l`) draws the
blur over nothing. An off-screen shot has no blur either, so the stage paints
`shot_backdrop` under the root; the chrome then shows opaque `#181717`.

| Token | Value | Replaces |
|---|---|---|
| note, active tab | `#100f0f` | `editor_background` `#1e1e1e`, `tab_active_background` |
| chrome | `#181717` at 85 % over the blur | `sidebar_background`, `tab_bar_background` `#181818` |
| hairline | `#282726` | `tab_rule`, the panel header rule |
| hover, selected row | `rgba(254,252,240,.05)` over the chrome | the white overlays, on chrome only |
| text | `#cecdc3` | `sidebar_text`, `tab_text` |
| muted text, icons | `#878580` | `tab_inactive_text`, `icon_muted` |

Every value lives in `theme.rs`. Modals, banners and markdown colours keep their
values unless the comparison shows a clash; record any change here.

## D2. Title row

- Height `TITLE_BAR_HEIGHT` 34 → 40 px, the reference's `--header-height`;
  `TRAFFIC_LIGHT_POSITION` is (14, 12), which puts the red light's centre at the
  reference's (20.75, 18.75) pt, measured on two captures. The sidebar's first
  button starts 88 px in, as the reference's container padding.
- The sidebar's segment is the sidebar's width; with the backlinks shown, a
  segment as wide as them ends the row and holds their toggle at its right end;
  the tab segment fills the rest. One hairline under the whole row, broken by
  the active tab: it carries the tabs' floor over both docks, so the row reads
  as one band (user request, 2026-11-08; until then the side panels ran
  unbroken to the window's top, as the reference's).
- Backlinks hidden: their toggle ends the tab segment.
- Sidebar hidden: the traffic lights' padding and the sidebar toggle lead the
  tab segment, as in S3's `panels-hidden`.
- The drag regions of S3-D3 still hold: empty space in either segment moves the
  window, a button never does.

## D3. Tabs

- Height 34 px inside the 40 px row, bottom-aligned, 10 px top corners (`--tab-radius-active`). `TAB_MAX_WIDTH` and the
  drop indicator are unchanged.
- The active tab spreads into the note with two concave feet outside its bottom
  corners, 10 px (`--tab-curve`; user request, 2026-11-04). Its side hairlines
  curve out along them. Each foot is a ring in the note's colour clipped to the
  corner's square; the chrome shows through the ring's hole.
- A hovered inactive tab shows a pill, not a full tab: inset 1 px from the top
  and 4 px from the sides and bottom, 8 px corners (`--tab-radius`), the hover
  token; its text stays muted (user request, 2026-11-04). The title starts
  10 px in, the pill's inset plus the reference's 6 px inner padding. A tab's
  content and `+` centre on the pill, not on the taller tab, as the reference's.
- The × shows on the active tab and on the hovered one, as today. The dirty and
  failed dots are unchanged.
- `+` after the last tab dispatches `NewTab`.

## D4. History arrows and the history menu

- `←` `→` start every centre group's strip, before the first tab and outside
  the tab bar's scroll. This holds in the title row's segments and in a lower
  group's header. Each pair acts on its own group's active tab's history.
- An arrow with no stop is dimmed at `DISABLED_OPACITY`.
- Enabled state comes from the active tab's history in `inkra-tabs`; the view
  reads it, it does not compute it.
- A right-click or a 500 ms hold on either arrow, lit or dimmed, opens the
  history menu at the pointer. The hold time is
  `GestureTuning::default().long_press_duration`.
- Row order: forward stops far to near, each with `→`. Then the current row,
  with an accent dot. Then back stops near to far, each with `←`. The menu
  shows at most 7 each way.
- Picking a row walks there in one step. A row whose stop moved walks nothing.
  The release that ends a hold walks nothing.
- `inkra-tabs` lists the stops (`Tabs::stops`) and walks to one
  (`Tabs::walk_to`); `app` names each row and draws the menu.
- Dock headers get no arrows: a dock's tabs hold views, and history skips views
  (tabs spec, H2).

Deviations:

- Zed's arrows have no menu
  (`vendor/zed-reference/crates/workspace/src/pane.rs:3474-3518`, `3570-3585`).
  Inkra reaches a note several steps back in one gesture, as a browser's back
  menu does.
- From the mock (`/tmp/inkra-nav-poc/index.html`): no "ago" times, no footer
  hint, 500 ms rather than 450 (gpui's default), and the menu anchors at the
  pointer like every `ContextMenu`.

## D5. Sidebar

Partly reversed by the docks spec: the tree has a `Files` header.

- No header. Rows 27 px, 8 px radius (`--nav-item-radius`), 8 px inset from the sidebar's edges, text
  at `UI_TEXT_SIZE`. Chevrons and indentation unchanged.
- Selected and hovered rows use the hover token, not a full-width band.

## D6. Backlinks header

`PanelHeader` with the title and a count; same surface as the chrome, a hairline
on its left edge against the note.

## Polish rules (from `make-interfaces-feel-better` and `better-ui`, read as gpui)

- **Concentric radii**: a rounded element inside a rounded one takes outer =
  inner + inset. Rows (6 px) inside the sidebar need no outer radius; a button's
  hover background inside a tab follows the rule.
- **Optical alignment**: centre icons by eye in their buttons; the `+` and the
  arrows sit on the tabs' text baseline, not the row's geometric middle.
- **One icon set, one stroke**: Lucide only (`Assets`), the stroke matching
  14 px regular text. States come from colour, never from a second asset.
- **No motion on high-frequency interactions**: hover, tab switch and row
  selection change instantly. No animation is added by this spec.
- **Borders for structure only**: hairlines separate surfaces and mark the
  active tab; nothing gets a border to fake depth.
- **Hit areas**: icon buttons keep at least a 28 × 26 px target, the reference's
  size. The skills' 40 × 40 px desktop minimum does not fit a 40 px row; this
  is a recorded deviation, not an oversight.
- Web-only rules (CSS transitions, `will-change`, font smoothing, `@media
  (hover)`) have no gpui counterpart and do not apply.

## Tests

- Update the shot scenes `tabs`, `panels-hidden`, `toggle-hover`, `tabs-hover`,
  `tabs-dragging`, `tabs-overflow`, `backlinks`; the arrows show in the strip,
  one dimmed. Add a scene with the history menu open. Render each and look at
  it beside the reference.
- `#[gpui::test]`: clicking `←` and `→` reaches the previous and next place; a
  click on a dimmed arrow changes nothing; `+` opens a new tab; the sidebar's
  search and new-note buttons dispatch their actions.
- `#[gpui::test]`: a right-click or a hold on an arrow opens the history menu;
  a row walks there in one step; with no history, nothing opens; the release
  that ends a hold walks nothing.
- Existing title-bar, drag-region and backlinks tests stay green; adapt their
  expected metrics to the new heights, never delete one.

## Slices

Suggested, for the plan to refine; each one touches data, view and test.

1. **Surfaces and sidebar** (D1, D5): theme tokens, the headerless sidebar,
   rows. Shot comparison against the reference freezes the values.
2. **Split title row and browser tabs** (D2, D3, D6): the row, the tabs, `+`,
   the backlinks header.
3. **Navigation row** (D4), later replaced by the strip's arrows and the
   history menu.

## Deviations

S1, surfaces and the headerless sidebar:

- `icon_active` joins the text row (#d7d6ce).
- `selected_row_text` #f0efe8 comes from the mock (`.row.on`).
- `TREE_TOP_GAP` is 4 px, from the reference.
- The sidebar column has a hairline on its right edge (the reference's full-height rule), and the backlinks column on its left (D6's edge), both drawn in S1.
- Backlinks and picker rows keep the white overlays: they are non-inset. On chrome, the backlinks overlay renders about #343333.
- `SIDEBAR_WIDTH` stays 220 (the reference is about 267, the mock 260).
- The rejected-name message starts at the row's padding edge (16 px), not at the name text (30 px, after the chevron slot).
- The shot comparison froze every D1 value unchanged: the `tabs` and `sidebar-*` scenes match the mock's variables.

S2, the split title row:

- Search and New note sit 30 px apart (`gap_0p5`); the reference is about 31.
- An on toggle takes no background: the reference shows none, so the mock's `.ib.on` is dropped.
- `TAB_MIN_WIDTH` follows the ×'s 28 px box, so a short tab's centre still misses its ×.
- The 26 px dismiss button grows the notice line from 22 to 26 px: in `save-failed` the text and the × stay centred on it.

S3, browser tabs:

- The bar pads `TAB_CURVE` (10 px) before the first tab and after `+`, so its scroll never clips a foot; this matches the reference's ~10 pt inset. `TAB_GAP` (6 px) parts the tabs and `+`; the mock uses 4.
- The tab's × is the reference's 20 px box, below the 28 × 26 hit-area rule: a full-size button filled the 29 px hover pill. Its radius, 3.5 px, is concentric with the pill.
- The drag preview always shows the active look: an inactive tab has no background to float on.
- The active tab's hairline is a paint-only overlay, so a layout border does not shift the title or take the drop indicator's colour.
- The title row's bottom rule is a paint-only layer listed first in the tab segment: the active tab, painted later, breaks it.

S4, the backlinks header:

- The header keeps the `ROW_INSET` padding, so the title lines up with the backlink rows. The mock pads 16.
- The count is 11.5 px muted text on the hover token.
- No count with no backlinks: "No backlinks" under the header already says it.
- The panel's left hairline was drawn in S1.

S5, the navigation row (replaced 2026-11-06 by D4's strip arrows):

- Enabled arrows use `icon_muted`, one colour for every button. The mock's #616161 is dropped.
- Dimmed means 45% opacity.
- The vendored gpui has no aria-disabled, so a dimmed arrow is inert but announced as a plain button.
- The nav row's left padding is 12 px, from the reference.
