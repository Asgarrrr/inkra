# Obsidian chrome: surfaces, split title row, browser tabs

Status: **approved 2026-11-04** in a brainstorm on HTML mocks. Not built.

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
  hairline on top and sides, 8 px top corners, and joins the note with no rule
  under it. An inactive tab has muted text and no background.
- **D4. A navigation row under the tabs.** 34 px, fixed at the top of the note
  column, holding `←` `→` bound to `GoBack` and `GoForward`. An arrow with no
  place to reach is dimmed and inert.
- **D5. The sidebar loses its `vault` header.** Rows grow to 27 px, rounded.
- **D6. Backlinks keeps a plain title header**: `Backlinks` and a count, not an
  icon bar. A later dock spec shows a title for a dock with one view and an icon
  bar for several; this header is drawn to fit that rule.

Out of scope: movable views (Zed's `Dock` and `Panel`, the next spec), the note
body's styling (headings, code blocks: markdown blocks S2), a tab list menu.

## D1. Surfaces

Sampled from the reference capture, converted to sRGB. The capture's colour
space makes them approximate: render the `tabs` scene and compare it beside
the reference before freezing them.

| Token | Value | Replaces |
|---|---|---|
| note, active tab | `#131212` | `editor_background` `#1e1e1e`, `tab_active_background` |
| chrome | `#2a2929` | `sidebar_background`, `tab_bar_background` `#181818` |
| hairline | `#353332` | `tab_rule`, the panel header rule |
| hover, selected row | `#383737` | the white overlays, on chrome only |
| text | `#d7d6ce` | `sidebar_text`, `tab_text` |
| muted text, icons | `#8f8d88` | `tab_inactive_text`, `icon_muted` |

Every value lives in `theme.rs`. Modals, banners and markdown colours keep their
values unless the comparison shows a clash; record any change here.

## D2. Title row

- Height `TITLE_BAR_HEIGHT` 34 → 39 px; re-derive `TRAFFIC_LIGHT_POSITION` from
  it (its doc comment gives the formula).
- The sidebar's segment is the sidebar's width; the tab segment fills the rest.
  One hairline under the whole row, broken by the active tab.
- Sidebar hidden: the traffic lights' padding and the sidebar toggle lead the
  tab segment, as in S3's `panels-hidden`.
- The drag regions of S3-D3 still hold: empty space in either segment moves the
  window, a button never does.

## D3. Tabs

- Height 34 px inside the 39 px row, bottom-aligned. `TAB_MAX_WIDTH` and the
  drop indicator are unchanged.
- The × shows on the active tab and on the hovered one, as today. The dirty and
  failed dots are unchanged.
- `+` after the last tab dispatches `NewTab`.

## D4. Navigation row

- Belongs to the note column, above the editor; the editor's scroll does not
  move it. Its background is the note's.
- Enabled state comes from the active tab's history in `inkra-tabs`; the view
  reads it, it does not compute it.

## D5. Sidebar

- No header. Rows 27 px, 6 px radius, 8 px inset from the sidebar's edges, text
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
  size. The skills' 40 × 40 px desktop minimum does not fit a 39 px row; this
  is a recorded deviation, not an oversight.
- Web-only rules (CSS transitions, `will-change`, font smoothing, `@media
  (hover)`) have no gpui counterpart and do not apply.

## Tests

- Update the shot scenes `tabs`, `panels-hidden`, `toggle-hover`, `tabs-hover`,
  `tabs-dragging`, `tabs-overflow`, `backlinks`; add `nav-row` with one arrow
  dimmed. Render each and look at it beside the reference.
- `#[gpui::test]`: clicking `←` and `→` reaches the previous and next place; a
  click on a dimmed arrow changes nothing; `+` opens a new tab; the sidebar's
  search and new-note buttons dispatch their actions.
- Existing title-bar, drag-region and backlinks tests stay green; adapt their
  expected metrics to the new heights, never delete one.

## Slices

Suggested, for the plan to refine; each one touches data, view and test.

1. **Surfaces and sidebar** (D1, D5): theme tokens, the headerless sidebar,
   rows. Shot comparison against the reference freezes the values.
2. **Split title row and browser tabs** (D2, D3, D6): the row, the tabs, `+`,
   the backlinks header.
3. **Navigation row** (D4).

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
