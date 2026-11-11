# Landing on the heading a link names, and on the link a backlink row shows

Status: **draft 2026-11-11**, for review before code. Planned at main `8ef653b5`.
Builds on `2026-09-24-links-and-backlinks-design.md` (D2, D3, D5, D7, D8),
`2026-11-08-backlink-context-on-demand-design.md` (B2, B5 to B7) and
`2026-09-28-vault-search-integration-design.md` ("Opening a hit", "As built (slice 4)").

## Context

Two navigations stop one step short.

- `[[Note#Heading]]` opens `Note` at its top. `Target::parse` keeps the fragment
  (`crates/index/src/target.rs:45-48`, `:14`). `Resolution` drops it
  (`crates/index/src/notes.rs:12-19`). `follow` passes no position
  (`crates/app/src/shell/activation.rs:255-259`).
- `[[#Heading]]` does nothing: `Resolution::SelfLink` is a no-op (`activation.rs:258`).
- A backlink row opens its source at the top (`crates/app/src/backlinks/render.rs:22-31`,
  `crates/app/src/shell/panes.rs:45-47`), although the row shows the line that holds the link.

The links spec deferred this, with the trigger "following a link to a long note lands at its
top and the user scrolls" (links spec, Deferred, line 626). Its scope table lists "Scrolling to
`#heading` or `#^block`" as out (line 28).

The machinery to open a note at a position exists. A vault search hit travels this path:

1. `HitActivated { note, range, open }` (`crates/app/src/vault_search/mod.rs:56-60`) reaches
   `shell.open(event.note, Some(event.range.clone()), event.open, cx)` (`crates/app/src/shell.rs:348`).
2. `open` (`activation.rs:82-108`) routes `at` to `show_file`, `open_in_new_tab` or `open_in_split`.
3. The note showing: `show_file` drops the pending activation and calls `select_range` at once,
   with no history entry (`activation.rs:138-148`).
4. Any other note: `open_file` (`activation.rs:31-58`) or `open_in_new_tab`
   (`crates/app/src/shell/tabs.rs:92-121`) spawns the load. When the buffer lands, `land`
   binds it and calls `select_range` on the editor (`activation.rs:61-79`). A superseded
   activation is a dropped `Task`, so its `at` is never applied (vault-search spec, line 250).
5. `select_range` snaps the range to graphemes and asks for `Reveal::Center`
   (`crates/app/src/editor/motion.rs:20-32`). The reveal waits for geometry and resolves in
   prepaint (vault-search spec, lines 286-298).

So `at` is a byte range known before the load, applied after it. This spec adds a target that is
resolved on landing, and gives two more openers a way to it.

## What note apps do

**Obsidian**, the compatibility target (links spec, "What note apps do"):

- `[[Note#Heading]]` links a heading; `[[#Heading]]` links one in the same note; several `#`
  name subheadings, as in `[[Help and support#Questions and advice#Report bugs and request
  features]]`; `#^id` links a block (https://obsidian.md/help/links).
- Matching is not documented. Third-party ports that read the app code report this rule for
  `resolveSubpath`, checked in Obsidian 1.5.3 and 1.15.0
  (https://github.com/KingOfKalk/obsidian-plugin-vault-mcp-server/pull/525): both sides go
  through `stripHeading`, which replaces a punctuation set and CR/LF with a space, collapses
  whitespace and trims; then they compare lower-cased. The set as reported is
  ``!"#$%&()*+,.:;<=>?@^`{|}~/[]\``; `_` and `-` are kept. The same PR says duplicates take the
  first heading. This is reverse-engineered, so it probably holds, and the build pins each case
  with a test.
- The heading side is the raw markdown source, not the rendered text: `## **Bold** title`
  matches `#Bold title` because `*` becomes a space (same source). Not verified against the app.
- The rule for a nested `A#B` is reported loosely ("a deeper heading B after it, not
  necessarily A's child"). Not verified.

**Zed** has no wikilinks, but its markdown preview follows `#fragment` links:

- A link carries `PreviewLinkTarget::Heading(slug)` or `Position { row, column }`
  (`vendor/zed-reference/crates/markdown_preview/src/markdown_preview_view.rs:131-134`). The
  target is resolved after the load: `scroll_to_heading_when_parsed` keeps a pending slug until
  the parse lands (`vendor/zed-reference/crates/markdown/src/markdown.rs:979-995`).
- Slugs come from the rendered heading text, GitHub-style: alphanumerics, `-` and `_` lowered,
  spaces to `-`, the rest dropped (`vendor/zed-reference/crates/util/src/markdown.rs:4-17`).
  Duplicates get `-1`, `-2` (`vendor/zed-reference/crates/markdown/src/parser.rs:104-140`).
- The preview scrolls a heading just into view, with a three-line margin
  (`markdown.rs:2603-2623`). The outline jumps with `Autoscroll::center()`
  (`vendor/zed-reference/crates/outline/src/outline.rs:415-420`).
- Back within one file: `push_to_nav_history` records a jump of 10 rows or more
  (`vendor/zed-reference/crates/editor/src/navigation.rs:1555-1576`,
  `MIN_NAVIGATION_HISTORY_ROW_DELTA`, `editor/src/editor.rs:307`).

What we take: Zed's shape (a target resolved on landing, after the parse) and Obsidian's match
rule (a vault's links were most likely written by Obsidian).

## Decisions

### L1. A fragment matches a heading by Obsidian's fold

`inkra-markdown` gains one pure function:

```rust
/// The heading `fragment` names in `text`, by Obsidian's fold; `None` if none matches.
pub fn find_heading(text: &str, headings: &[Heading], fragment: &str) -> Option<Range<ByteOffset>>;
```

The rule:

1. **The heading's text** is its source between the marker and the closing sequence. `Heading`
   gives the range and the marker (`crates/markdown/src/parse.rs:32-37`). The range is the first
   line for ATX and the text lines for setext (`heading_text`, `parse.rs:198-203`). A trailing
   run of `#` after a space is the closing sequence and is cut.
2. **The fold**, on both sides: each character of Obsidian's set, `\n` and `\r` become a space;
   whitespace runs collapse to one space; the ends are trimmed; the text is lower-cased with
   `str::to_lowercase`.
3. **Equal folds match.** Case and surrounding spaces never matter. Inline markup does not exist
   as such: its punctuation folds to spaces, so `**Bold**` matches `Bold` and `_Tables_` does not
   match `Tables`, as in Obsidian.
4. **Duplicates**: the first heading in document order wins.
5. **A nested fragment** `A#B#C` splits at every `#`. `A` matches as above. `B` matches the first
   heading inside `A`'s section, at any depth: after `A`, before the next heading of `A`'s level
   or shallower. `C` matches inside `B`'s section the same way.
6. **A partial match lands on the deepest part matched**: `A#Missing` lands on `A`.
7. **No match**, an empty fragment, or a fragment starting with `^` (a block id): `None`.

The returned range is the heading's text, without the marker (L6 places the caret).

Rejected:

- **Zed's GitHub slug** (`util/src/markdown.rs:4-17`). Links written by Obsidian keep spaces and
  case; `#My Heading` would never match a slug `my-heading`.
- **Matching the rendered text.** It needs the inline events of the heading, which `Markdown`
  does not keep, and it disagrees with Obsidian on `## [[a|B]]`.
- **Unicode NFC in the fold.** `inkra-markdown` has no `unicode-normalization` dependency
  (`crates/markdown/Cargo.toml`), and both sides are usually typed on one keyboard. An NFD
  heading against an NFC link would miss. Accepted, with a trigger (Deferred).
- **A nested part matched anywhere after the previous one**, which Obsidian may do (not
  verified). `Daily#Notes` would then reach a `Notes` heading in a later section. The section
  rule is what the user wrote.
- **Top of the note on a partial match.** The deepest part matched is strictly closer.

Why `inkra-markdown` and not `inkra-index`: the function reads one note's text and its parse,
both markdown's data. The editor already depends on `inkra-markdown`
(`crates/app/src/editor/parses.rs:7`). In `index` it would use no index state.

### L2. The app finds the heading on landing, in the parse the draw shares

The heading is found when the buffer lands, against the buffer's text, through
`parses::parsed(buffer, cx)` (`crates/app/src/editor/parses.rs:22-41`). That cache keeps one
parse per buffer version, and the first draw of the landed note needs the same parse. So a
landing costs no extra parse: the draw reuses it. A cold one costs about 89 µs for 50 KB
(live-preview spec, X1, line 83).

The fragment is matched against the live buffer, not the saved text. A heading typed a second
ago is found.

Rejected: **headings in the index.** Each note's record would grow by its headings, and the
cache's `FORMAT_VERSION` (`crates/index/src/persist/header.rs:10`) would go up. It fights the
memory gate of the backlink-context spec (≤ 200 MiB, "Goals and gates"). The ranges would be
saved-text ranges, stale against an open, edited buffer (links spec, D5). Its one gain is
heading completion for notes not loaded, which L7 defers. When that completion lands, headings
enter the index for it.

### L3. One type travels to `open`: a range or a heading, resolved on landing

`crates/app/src/open.rs` gains:

```rust
/// Where an opener wants the caret; resolved against the buffer when it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Spot {
    /// Saved-text offsets: a search hit, a backlink's link.
    Range(Range<ByteOffset>),
    /// A link's fragment, as written: `A#B` for a nested heading.
    Heading(Arc<str>),
}
```

- `at: Option<Range<ByteOffset>>` becomes `at: Option<Spot>` on `open`, `show_file`,
  `open_file`, `open_in_split`, `open_in_new_tab` and `land`. Vault search passes
  `Spot::Range` (`shell.rs:348`).
- `EditorView::select_range` gains a sibling, `land_on(spot, window, cx)`, which resolves the
  spot and places the caret per L6. It runs where `select_range` runs today:
  `activation.rs:71-73` and `activation.rs:140-144`.
- A `Spot::Heading` that matches nothing leaves the caret where the open put it: offset 0 for a
  note just opened (`EditorView::open_buffer`, `crates/app/src/editor/mod.rs:130-133`), and
  unmoved for the note showing.
- `follow` reads the fragment from `Target::Note` and passes `Spot::Heading` with
  `Resolution::File` (`activation.rs:256`). `Resolution` itself does not change: the fragment is
  the link's, not the resolution's.
- `Target::parse` percent-decodes an inline link's fragment as it decodes the path
  (`target.rs:54-57`). Obsidian writes `[x](Note.md#My%20Heading)` when wikilinks are off.

Rejected:

- **A range resolved before `open`.** The note may not be loaded, so its text is not there to
  search.
- **A fragment field on `Resolution::File`.** Quick open, the tree and backlinks resolve
  nothing, and `backlinks` compares resolutions (`crates/index/src/index.rs:295`,
  `resolution == Resolution::File(file)`). A fragment in it would split equal resolutions.
- **Two parameters** (`at` and `heading`). Both set is a state no opener means.

### L4. `[[#Heading]]` moves the caret in place, with no history entry yet

`Resolution::SelfLink` calls `open` on the note showing with `Spot::Heading`. `show_file` sees
the note showing and lands the spot in place, as a search hit in that note does
(`activation.rs:138-148`). `[[Note#Heading]]` to the note showing takes the same path.

**Open question: does back return to the link?** Not in this design. Tab history is
note-granular: a push onto a top on the same note replaces it
(`crates/tabs/src/history.rs:58-67`), and back pops past every place on the note showing
(`history.rs:82-85`, tabs spec A1 invariant 4). A back that returns to the link needs history
within one note. The tabs spec defers exactly that: "Back and forward within one note (a link or
search hit in the note showing)", trigger "a user reports back skipping a jump, as Obsidian
users did" (`2026-09-29-tabs-design.md:827`). It changes invariant 4 for search hits too, and it
needs a rule for which caret moves count (Zed's 10-row delta, `editor.rs:307`).

Recommendation: ship L4 without a history entry. Write in-note history as its own spec when its
trigger fires. A self-link makes that trigger likelier, so the roadmap entry names it.

Rejected for this feature: **lifting H1 for jumps only.** A stack that holds two places on one
note breaks invariant 4's proptest and the history menu's one-row-per-note listing
(`crates/app/src/shell/nav.rs:130-142`). That is a tabs change, not a link change.

### L5. A backlink occurrence lands on its link, through a second event

**What the row carries.** Each occurrence carries its link's range in the source. On main it
comes from `Backlink::occurrences()`, which yields `&SourceLink` with `range`
(`crates/index/src/index.rs:56-58`, `crates/index/src/source.rs:29`). On `backlinks-context-2b`
the row keeps the whole `Backlink` (`crates/app/src/backlinks/mod.rs` on that branch, `Row`),
so the range is there too. `Occurrence` gains `link: Range<ByteOffset>`; it joins the
`PartialEq` that decides a redraw, and an equal link range changes nothing.

**The event.** A new event, as vault search has its own (`vault_search/mod.rs:56-62`):

```rust
pub(crate) struct LinkActivated {
    pub(crate) source: EntryId,
    pub(crate) link: Range<ByteOffset>,
    pub(crate) open: Open,
}
```

The panel emits it from an occurrence line. The occurrence's mouse-down calls
`cx.stop_propagation()`, as the tab bar does (`crates/app/src/shell/tab_bar.rs:217`), so the
group's `FileActivated` does not fire too. The source's name line keeps `FileActivated`, and
opens the note as today. The shell subscribes beside the panel's `FileActivated`
(`panes.rs:45-47`) and calls `open(source, Some(Spot::Range(link)), open, cx)`.

Rejected: **an `at` field on `FileActivated`.** It is `Copy` (`open.rs:24`) and a `Range` is
not. Two of its three emitters (`crates/app/src/sidebar/tree.rs:34`,
`crates/app/src/quick_open/mod.rs:226`) would pass `None` forever.

**A source buffer that differs from the saved text.** The range is in the text the index read,
which is the saved text (links spec, D5; B7 proves a context against it). If the source is open
with unsaved edits, the range may be off by what was typed since the last save, about two
seconds at most (autosave, links spec D5). `select_range` already snaps such a range to
graphemes inside the buffer (`motion.rs:19-29`). We accept that drift, as vault search does
(vault-search spec, lines 255-257).

Rejected: **finding the link again by its ordinal** among the buffer's links. The index keeps
note links only (backlink-context spec, B2), while `inkra_markdown::links` returns every link, so
the ordinals differ. A link added above it also shifts the ordinal. That is a second rule to fix
a two-second window.

### L6. What landing looks like

| Opener | Caret | Scroll |
|---|---|---|
| Heading (L1) | Empty selection at the heading text's start, after the marker | Heading line at the viewport's top |
| Backlink occurrence (L5) | Empty selection at the link's start | Centred, as a search hit |
| Search hit (unchanged) | The hit selected | Centred |

- `Reveal` (`crates/app/src/editor/viewport.rs:12-21`) gains `Top`. The scroll clamps at the
  document's end, as the other strategies do.
- The caret on the heading line reveals its marker; the live-preview spec hangs it in the margin,
  so no text moves (`docs/roadmap.md:151-152`).

Rejected:

- **The heading line selected.** A keystroke would replace the heading.
- **A heading centred** (Zed's outline) or **just in view** (Zed's preview). A heading link names
  a section, read downward from its title. Centring spends half the screen on the section above.
  Obsidian probably scrolls to the top as well; not verified.
- **The backlink's link selected.** A keystroke would delete the link. The caret before it shows
  the place, and the line around it is the line the row showed.

### L7. Heading completion is deferred

`[[Note#` completes nothing in this feature. `[[` completion itself is deferred (links spec,
Deferred, line 633). Heading completion joins it, with the same trigger. When it is built,
headings of unloaded notes come from the index (L2).

Rejected: building heading completion now. It needs a completion popup the editor does not have.

### L8. Deviations from Obsidian

| Obsidian | Inkra | Why |
|---|---|---|
| A self-link jump enters history (probably; not verified) | No history entry (L4) | Tab history is note-granular; in-note history is a deferred tabs change |
| A nested part may match anywhere after the previous one (reported loosely) | Inside the previous match's section (L1, rule 5) | Lands where the user wrote |
| `#^block` scrolls to the block | Opens at the top | Deferred, below |
| Unicode fold of unknown form | `str::to_lowercase`, no NFC (L1) | No normalization dependency in `markdown`; deferred |
| A missing heading: not verified | Lands at the top or stays, with no message | A missing heading is not an error of the app; a notice (`crates/app/src/shell/notice.rs`) is for failures |

## Tests

Per slice; the "bug to catch" column states what each test must fail on.

**S1, `markdown` (no window).** `find_heading`:

- Case and spaces: `#  heading TWO ` matches `## Heading Two`.
- Punctuation: `#A B` matches `## A/B`; `#Bold title` matches `## **Bold** title`.
- Kept characters: `#Tables` does not match `## _Tables_`.
- Closing sequence: `#Title` matches `## Title ##`.
- Setext: `#Title` matches `Title\n===`.
- Duplicates: the first of two equal headings.
- Nested: `A#B` skips a `B` before `A` and a `B` in the next section of `A`'s level.
- Partial: `A#Missing` returns `A`.
- None: no heading, empty fragment, `^id`.
- A heading inside a fence is not a heading (the parser decides; one test pins it).
- Proptest: every range returned lies inside `text`, on char boundaries, inside some heading's
  range.

**S1, `index` (no window).** `Target::parse` percent-decodes an inline fragment;
a wikilink fragment stays as written.

**S1, `app` (headless, `MemFs`).**

- `cmd-enter` on `[[Note#Second]]` (a `simulate_keystrokes` test) opens `Note` with the caret
  at `Second`'s text, after a load gated with `MemFs::block_load`. Catches an `at` applied before
  the buffer lands.
- The same link with no matching heading lands at offset 0.
- A second open before the load lands applies only the second spot.
- `Reveal::Top`: the heading's row is the viewport's first row (asserted from the viewport,
  then rendered in a `shot` scene and looked at).

**S2, `app`.** `[[#Second]]` in the note showing moves the caret to `Second` without a rebind
(the editor's entity is unchanged) and without a history entry (`can_go_back` unchanged).
`[[Note#Second]]` to the note showing does the same.

**S3, `app`.**

- A press on an occurrence line opens the source with the caret at that link's start. Two
  occurrences in one source land on two different offsets. Catches a row-level range.
- A press on the source's name opens it at the top and emits no `LinkActivated`.
- An occurrence press emits one event, not two (the group's `FileActivated` does not fire).
- An open source with unsaved text above the link lands inside the buffer, no panic.
- The `backlinks` shot scene, looked at: the occurrence's hover reads as a target of its own.

`cargo mutants` on `markdown` and `tabs`-free `app` changes after each slice, zero missed.

## Risks

- **The fold is reverse-engineered.** If Obsidian's set differs, some links miss and land at the
  top. That fails safe. Before S1 merges, check the set against Obsidian with a test vault of one
  heading per punctuation character.
- **Self-links invite the back question.** Users may expect back to return to the link (L4). The
  roadmap entry records it.
- **The backlink range drifts on a dirty source** (L5), for at most one autosave period.
- **`backlinks-context-2b` is unmerged.** S3 touches `backlinks/mod.rs` and `render.rs`, which
  that branch rewrites. Build S3 after it merges.

## Plan (slices)

Each slice merges alone with `./scripts/check.sh` green.

| Slice | Content | Files |
|---|---|---|
| S1 | `[[Note#Heading]]`: `find_heading`; `Spot`; `at: Option<Spot>` through the openers; `EditorView::land_on`; `Reveal::Top`; inline fragments decoded | `markdown/src/` (a new `headings.rs` and its tests), `index/src/target.rs`, `app/src/open.rs`, `app/src/shell/{activation,tabs}.rs`, `app/src/shell.rs`, `app/src/editor/{motion,viewport}.rs` |
| S2 | `[[#Heading]]`: `SelfLink` lands in place | `app/src/shell/activation.rs`, its tests |
| S3 | A backlink occurrence lands on its link: `Occurrence.link`, `LinkActivated`, the subscription | `app/src/backlinks/{mod,render}.rs`, `app/src/shell/panes.rs`, tests, the shot |

Docs per slice: this spec's "As built" notes; the links spec's Deferred row "Scroll to
`#heading`, `#^block`, and to a backlink's line" amended; `docs/roadmap.md` gains the rows below.

## Deferred

| Item | Trigger |
|---|---|
| `#^block` links | A vault imported from Obsidian uses block ids |
| Back and forward within one note | The tabs spec's trigger, now also a self-link jump (L4) |
| `[[Note#` heading completion, with headings in the index | `[[` completion is built (links spec, Deferred) |
| NFC in the heading fold | A heading written in NFD misses a link that looks equal |
| GitHub slug fallback (`#my-heading`) | A vault from GitHub or Zed uses slug links |
