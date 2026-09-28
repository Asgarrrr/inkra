# One module builds highlighted row text

Date: 2026-09-29. Status: **implemented**.

A refactor from the architecture review. Behaviour stays identical, except for
the one change to backlinks listed under B1.

## Context

Three views draw one line of text with lit byte ranges: quick open (`⌘P`),
vault search (`⌘⇧F`) and the backlinks panel. Each builds its line its own way.

- `quick_open/render.rs:74-102`, `RowText::name`, splits the NFC path at its
  last `/` into stem and folder. It clips each `name_highlights` range to its
  half, so a grapheme across the `/` lights the stem only. `set_excerpt`
  (`:51-72`) runs `one_line` on the excerpt and keeps its ranges.
- `vault_search/rows.rs:92-116`, `Header::new`, splits a second way, through
  `RelPath::display_name` and `Path::parent`, each through `one_line`.
- `vault_search/rows.rs:126-145`, `HitRow::new`, trims the context's indent,
  but never past the hit, runs `one_line`, and shifts the lit range.
- `backlinks/mod.rs:118-124` copies `SourceLink::context` and `context_link`
  as they are: no `one_line`, no indent trim.

Each view builds its own `HighlightStyle`: `match_highlight()` in quick open,
a local `lit` in `vault_search/render.rs:57`, `link_highlight` in
`backlinks/render.rs:12`. Quick open stores the style in every range.

The name row, a stem that ellipsizes at its end and a dimmed folder that
ellipsizes at its start, is copied with the same comment in
`quick_open/render.rs:110-140` and `vault_search/render.rs:10-33`.

Nothing checks that a range lands on a char boundary inside its text. gpui
panics on a mid-char highlight, in release too (Zed's `HighlightedLabel`
comment, `ui/src/components/label/highlighted_label.rs:169-179`). Today only
the index's contracts keep the ranges valid.

## What Zed does

- `file_finder.rs:1415` `labels_for_path_match` splits one path into file
  name and directory. A position at or past the name's start moves into the
  name, shifted; a position before the directory's end stays in it. Our split
  is the same rule, on ranges instead of char positions.
- `HighlightedLabel` (`ui/.../highlighted_label.rs`) is one component for
  "text plus lit positions". The data holds no style; `render` builds the
  `HighlightStyle` from the theme (`:219-230`). `single_line` replaces
  control chars and remaps the offsets. `new` drops every index when one is
  off a char boundary, under `debug_panic!`.
- `picker/highlighted_match_with_paths.rs` `HighlightedMatch` is text plus
  positions plus a colour, rendered through `HighlightedLabel`.

Deviations, each simpler for a note app:

- **Byte ranges, not char positions.** The index gives ranges (`NoteMatch`,
  `Excerpt`, `Hit`, `SourceLink`). Converting to positions and back costs a
  walk per row for nothing.
- **Control chars become one space, byte for byte** (`one_line`), so no
  offset moves. Zed's stand-ins are wider, so it must remap.
- **Invalid ranges are clamped and snapped, not dropped.** A snapped range
  still lights the grapheme the index meant. Dropping them all would hide a
  whole row's match for one bad range.
- **No `ui` crate component.** One free function renders the text; one
  renders the name row. A component type would have one implementer.

## The module: `row_text`

`crates/app/src/row_text.rs`, with `row_text/render.rs` and
`row_text/tests.rs`.

It hides: the one-line rule, the path split, the clip of a range to a half,
the indent trim, and the range invariant. A view hands it the index's text
and ranges and gets back strings that are safe to draw.

It lives in `app` because its outputs are `SharedString`s and only views call
it. `one_line` and the scroll geometry set the precedent: pure logic, plain
`#[test]` and `proptest`, no `Window`. No lower crate owns display text. The
index gives the raw context; making it one-line would push a view rule into
`inkra-index`.

### Types

```rust
/// One line of text and the ranges in it to light.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LitText { text: SharedString, ranges: Vec<Range<usize>> }

impl LitText {
    /// `one_line(text)`, with `ranges` made valid for it.
    pub(crate) fn line(text: &str, ranges: impl IntoIterator<Item = Range<usize>>) -> Self;
    /// As `line`, after the leading whitespace, cut no later than the first range's start.
    pub(crate) fn context(text: &str, ranges: impl IntoIterator<Item = Range<usize>>) -> Self;
    pub(crate) fn text(&self) -> &SharedString;
    pub(crate) fn ranges(&self) -> &[Range<usize>];
}

/// A note path as its name and its folder, each one line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PathText { pub(crate) name: LitText, pub(crate) folder: LitText }

impl PathText {
    /// `path` is `/`-separated with no extension; `ranges` index it.
    pub(crate) fn new(path: &str, ranges: &[Range<usize>]) -> Self;
    /// `folder/name`, or `name` at the root: the path one-lined.
    pub(crate) fn label(&self) -> SharedString;
}
```

`render.rs`:

```rust
pub(super) fn highlights(lit: &LitText, color: Hsla)
    -> impl Iterator<Item = (Range<usize>, HighlightStyle)> + '_;
pub(crate) fn styled(lit: &LitText, color: Hsla) -> StyledText;
/// The name, then the folder dimmed; a caller appends children (the hit count).
pub(crate) fn name_row(path: &PathText, color: Hsla) -> Div;
```

### Invariants

The fields of `LitText` are private, so every value holds these:

- **I1.** `text` holds no ASCII control char.
- **I2.** Every range is non-empty, `end <= text.len()`, and both ends are
  char boundaries of `text`.
- **I3.** Ranges ascend and are disjoint; ranges that overlap or touch merge.
- **I4.** `line(t, r).text().len() == t.len()`. On valid input (I2 and I3
  against `t`), `line` returns `r` unchanged.

The input rule, in order: clamp both `start` and `end` to `text.len()`,
floor `start` and ceil `end` to a char boundary, drop empty ranges, sort by
start, merge. A range's `start` can exceed its text: `PathText::new` hands
the folder whole-path ranges. The floor and ceil never index past
`text.len()`; they probe with `str::is_char_boundary`, which returns false
rather than panicking past the end.

`context(t, r)`: `cut = min(leading-whitespace bytes of t, min start of r)`,
then `line(&t[cut..], r shifted down by cut, saturating)`. With no range, the
whole indent goes.

`PathText::new(p, r)`: `stem_start` follows the last `/`, or is 0.
`folder_end = stem_start.saturating_sub(1)`. `folder` is
`line(&p[..folder_end], r)`, whose clamp does today's clip.
`name` is `line(&p[stem_start..], r shifted down by stem_start, saturating)`.
So a range across the last `/` lights both halves, a range ending on the `/`
lights the folder only, and the `/` itself is never drawn.

## What moves where

- **Q1. Quick open.** `RowText` keeps its role (built on landing, counted by
  `BUILDS`). It becomes `{ path: PathText, excerpt: LitText, label }`.
  `RowText::name` and `match_highlight` go. `set_excerpt` calls
  `LitText::line(&excerpt.text, excerpt.highlights)`. The label is
  `path.label()`, then `: {excerpt}` when the excerpt is not empty.
  `render_row` calls `name_row` and `styled(excerpt, theme::match_text())`.
- **V1. Vault search headers.** `Header { path: PathText, count, label }`.
  `Header::new` calls `PathText::new` on
  `path.as_path().with_extension("").to_string_lossy()` with no ranges. That
  equals `display_name` and `parent` for every `.md` note, including a root
  note and a dotted stem (`a/b.c.md`). `render::header` becomes
  `name_row(&header.path, theme::match_text().into())`, then `.relative()`, the count and the rule.
- **V2. Vault search hits.** `HitRow { context: LitText, .. }` replaces
  `context` and `lit`. `HitRow::new` calls
  `LitText::context(&context, [context_match])`. `render::hit` calls
  `styled(&hit.context, theme::match_text())`.
- **B1. Backlinks.** `Occurrence { context: LitText }` replaces `context` and
  `link`, built with `LitText::context`. `render_row` calls
  `styled(&occurrence.context, theme::link_text())`; `link_highlight` goes.
  **Behaviour change**, the only one: a backlink context that starts at its
  line's start loses its indent, and a tab in it draws as a space. Vault
  search shows the same text this way, so both panes now agree.
- The debug selector `quick-open-stem` becomes `row-name`, set in `name_row`
  on the inner, ellipsis-clipped name child, where it is today. Never on
  `name_row`'s outer `w_full()` row: then
  `a_stem_wider_than_the_row_is_cut_inside_it` would stop testing the cut.

## Tests

The new interface is the test surface. Each line is one executable criterion.

`row_text/tests.rs`, plain `#[test]` and `proptest`:

- P1 (proptest). For any string drawn from ASCII, control chars, `/`, `é`,
  `e\u{301}` and `🎉`, and any ranges with ends in `0..len + 8`: `line`,
  `context` and both halves of `PathText::new` hold I1 to I3.
- P2 (proptest). For valid input, `line` keeps the text length and the
  ranges (I4), and `context`'s lit substrings equal the input's lit
  substrings, one-lined.
- P3 (proptest). For valid input to `PathText::new`, a byte of `p` other
  than the last `/` is lit in its half exactly when an input range holds it.
- `a_range_ending_on_the_last_slash_lights_the_folder_only` (moved from
  `quick_open/render/tests.rs`, now on `PathText::new` directly).
- `a_range_across_the_last_slash_lights_both_halves`.
- `a_root_path_has_no_folder`; `label_joins_folder_and_name`.
- `a_mid_char_range_widens_to_the_whole_char`;
  `overlapping_ranges_merge`; `a_range_past_the_end_is_clamped`.
- `the_indent_is_cut_but_never_past_the_hit` (the rule `HitRow` held).
- `highlights_colour_every_range` for `render::highlights`: replaces the
  colour asserts in `quick_open/render/tests.rs` and
  `backlinks/tests.rs:334`.

Kept, adapted to the accessors:

- `quick_open/render/tests.rs`: the split, grapheme, decomposed-char,
  newline and excerpt tests stay. They prove the index-to-row path.
- `quick_open/tests/rows.rs:213` reads `path.name.ranges()`; `:292` reads
  `row-name`.
- `vault_search/tests/rows.rs::the_hit_is_lit_in_its_context` and
  `backlinks/tests.rs::rows_of` index `context.text()` by `ranges()[0]`.

Added in the views:

- `vault_search/tests/rows.rs::a_header_splits_its_path_like_quick_open`:
  `work/a\nb.md` gives name `a b` and folder `work`.
- `backlinks/tests.rs::an_indented_link_drops_its_indent`: `\t  see [[b]]`
  gives the context `see [[b]]` with `[[b]]` lit.

Run `./scripts/test-names.sh` before step 1 and at the end of each of steps
1, 2 and 3, and diff it against the previous run. A dropped `mod` line then
surfaces at the step that caused it. The only names that vanish are the ones
moved or replaced above.

## Look

Render these scenes before step 1 and after the last step, and compare:
`quick-open-results`, `quick-open-groups`, `quick-open-text-only`,
`quick-open-graphemes`, `vault-search-results`, `vault-search-limit`,
`vault-search-opened`, `backlinks`. They must match pixel for pixel, except
`backlinks` where an indented context would move left. The `backlinks` scene
has no indented link, so it must match too.

## Risks

- **A header's name used a plain child, now a `StyledText` with no ranges.**
  Both shape one run with the inherited style; the shots confirm no change.
- **The per-view colour is no longer asserted by a test.** Each view names it
  once, at its `styled` call; review and the shots check it.
- **Snapping hides an index bug** where a debug panic would expose it. Keep
  the index's own range tests as the guard; `row_text` only keeps gpui from
  panicking.
- **`row-name` is not unique in a frame.** gpui keeps debug bounds in a flat
  map, last write wins (`window.rs:1028-1030`). A `debug_bounds("row-name")`
  assertion is valid on a one-row scene only. A multi-header vault-search
  assertion needs another mechanism, such as an index-suffixed selector.
- **`with_extension("")` on a non-`.md` path** would cut a real extension.
  Vault search lists notes only, all `.md`.

## Plan

Each step builds, passes `./scripts/check.sh`, diffs `test-names.sh`
against the previous step, and is one commit.

1. **`row_text` core.** Add `LitText`, `PathText`, `render.rs` and the tests
   P1 to P3 plus the unit tests. Move quick open onto them (Q1), with its
   tests adapted. Shots of the quick-open scenes match.
2. **Vault search.** V1 and V2, the header test, the hit test adapted. Shots
   of the vault-search scenes match.
3. **Backlinks.** B1, the indent test, `rows_of` adapted. The `backlinks`
   shot matches.
4. **Close.** Diff `test-names.sh` against step 0 for the whole list. `cargo mutants` is not
   required: no `text`, `editor`, `markdown` or `index` change. Add this spec
   to the CLAUDE.md docs list.

## As built

- **Snapping uses the standard library.** `str::floor_char_boundary` and
  `ceil_char_boundary` clamp an index past the end to `len` before they
  probe, so `line` needs no clamp of its own.
- **`context` lands in step 2, not step 1.** Only vault search and backlinks
  call it; in step 1 it failed `clippy -D warnings` as dead code. Its unit
  test and its lines in P1 and P2 moved with it.
- **`RowText::set_excerpt` takes no path.** The label now comes from
  `name.label()`, so the path argument went.
- **The backlinks fixture is `  see\t[[b]]`, not `\t  see [[b]]`.** A
  leading tab opens an indented code block, which holds no link, so the
  panel listed no row. Two spaces stay a paragraph. The tab moved inside the
  line, so the test also proves B1's tab-to-space. It is named
  `an_indented_link_drops_its_indent_and_draws_a_tab_as_a_space`.
- **Test names.** P1 is `any_input_gives_valid_ranges`, P2
  `valid_input_keeps_its_ranges`, P3 `a_path_lights_each_byte_in_its_half`.
  Two names vanish: `a_highlight_ending_at_the_stem_lights_the_folder_only`
  moved to `row_text` as
  `a_range_ending_on_the_last_slash_lights_the_folder_only`, and
  `a_backlink_row_highlights_its_link_in_the_link_colour` is replaced by
  `highlights_colour_every_range` and the backlinks indent test.
- **Shots.** All eight scenes render byte for byte as before, compared with
  `cmp` on the PNGs.
- **Mutants.** No lower crate changed, but `row_text` ran under
  `cargo mutants -p inkra-app --test-package inkra-app -f <file>`: 39
  mutants, 16 caught, 23 unviable, none missed.
