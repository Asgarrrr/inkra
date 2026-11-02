# Roadmap — deferred work, and what brings it back

Nothing here is scheduled. Each item was taken out of a shipped plan on
purpose, and each carries the **trigger** that makes it worth doing. An item
with no trigger fired is code without a reason.

Sizes are estimates for one person at this repo's verification bar: proptest
over edit sequences in `text`, one `simulate_keystrokes` test per binding,
`check.sh` green, and a manual render pass because `check.sh` renders nothing.
They are estimates, not commitments — multi-cursor carries most of the
uncertainty, for reasons given below.

## Editor

| Item | Trigger | Estimate | Blocked by |
|---|---|---|---|
| ~~Horizontal scroll and reveal~~ | **Closed 2026-09-23, by removal.** Soft wrap leaves no right edge to lose the caret past. | — | — |
| ~~Soft wrap~~ | **Shipped 2026-09-23.** Its trigger was the one above: a pasted URL. | — | — |
| ~~One shape per keystroke~~ | **Shipped 2026-09-28.** A frame reuses the layout its inputs already produced, a reload included; see Measurements below. | — | — |
| Virtualization | A profile on a real note showing per-frame shaping or `SharedString` allocation costs anything. Measured 2026-09-28: it costs above ~290 KB. | 2–3 d | — |
| Multi-cursor | No trigger identified yet. | 4–6 d | — |
| ~~Reveal in prepaint~~ | **Shipped 2026-09-29 (tabs S0)**: a waiting scroll resolves in `TextElement::prepaint`, the element shifts its own paint by the offset it wrote; the flash and the deferred notify are gone (`superpowers/specs/2026-09-29-tabs-design.md`, As built (S0)). | — | — |
| `TextElement` owns its vertical scroll: an anchor (`TopLine`, as Zed's `ScrollAnchor` in its `ScrollManager`) becomes the source of truth instead of the `div`'s pixel offset, which removes `text_top`, the prepaint shift and the only-child invariant (`superpowers/specs/2026-09-29-tabs-design.md`, As built (S0)) | A resize shifting the text under the viewport is reported, or virtualization needs the visible rows before layout | 1–2 d | — |
| Clipboard and undo in `NameField` (`cmd-c`, `cmd-x`, `cmd-v`, `cmd-z`, `cmd-shift-z`). Since `TextInput` (`superpowers/specs/2026-09-29-text-input-design.md`), a move of their bindings from `EditorView` to `TextInput` and of their handlers to `text_input/actions.rs` | Someone reaches for them in a name field | 0.5 d | — |

### Soft wrap and horizontal scroll

Soft wrap shipped on 2026-09-23 and closed the horizontal item without building
it: a line longer than the 70 `ch` column paints on several rows, so no line
reaches the right edge. The design, what red/blue review changed in it, and
the edges it accepts — End stopping one character short on a mid-word wrap, a
resize shifting the text under the viewport — are in
`superpowers/specs/2026-09-23-soft-wrap-design.md`, with a Deferred table of
its own.

### Measurements, 2026-09-24

Release build, Apple M5 Pro. `parse` comes from
`cargo bench -p inkra-markdown`. The keystroke figures come from a throwaway
off-screen scene: one character typed at the top of a synthetic note, each
phase timed around its call, then the timers removed. The note repeats one
markdown section: a heading, styled prose, links, a quote, a list and a fence.

| Note | `parse` | `reshape` | `measure`, twice | Keystroke, dispatch to paint |
|---|---:|---:|---:|---:|
| 5 KB, 218 lines | 37 µs | 0.4 ms | 0.2 ms each | 1.0 ms |
| 50 KB, 2,162 lines | 0.35 ms | 3.3 ms | 2.5 ms each | 10.5 ms |
| 500 KB, 21,602 lines | 3.5 ms | 32 ms | 27 ms each | 115 ms |

Every cost is linear in the note, and shaping dominates parsing about 20 to 1.
A keystroke shapes the whole note three times:

- `EditorView::reshape`, to reveal the caret.
- `request_layout`'s measure, in taffy's `MinContent` sizing pass. It shapes
  at the widest column, then reports a width of 0 and discards the layout.
- The same measure in the definite pass. It shapes at the width `reshape` just
  used, from the same buffer and selection.

The two redundant shapes went on 2026-09-28. `DocumentLayout` records what
it was shaped from, so the measure reuses `edit_buffer`'s layout when those
inputs still hold. Taffy's intrinsic-width queries now shape at the last
width, not the widest one. Measured again with 20 keystrokes through the
off-screen harness, which draws a frame before each keystroke:

| Note | Before | After |
|---|---:|---:|
| 5 KB | 0.69 ms | 0.34 ms |
| 50 KB | 6.68 ms | 2.51 ms |
| 500 KB | 81.4 ms | 27.9 ms |

The 16 ms frame budget now holds up to about 290 KB. Past that, only
windowed shaping helps.

After the 2026-11-02 perf batch (S4), a typed key costs 0.23, 0.82 and 11.4 ms
and an idle draw 0.050, 0.048 and 0.057 ms at 5, 50 and 500 KB; the idle draw no
longer grows with the note (`superpowers/specs/2026-11-02-perf-batch-design.md`).

### Virtualization

Soft wrap has shipped, so rows are no longer one `line_height` per line and
`uniform_list` is out. It is a custom windowed element that shapes visible rows
only — the same conclusion markdown live preview reaches independently, per the
modular architecture design doc. One whole-document pass is left:
`edit_buffer` shapes before revealing the caret, and the frame reuses that
layout. The windowed element must shape only visible rows there as well.

### Multi-cursor

`Selection` is one range by construction, so this is a change across all three
crates: a selection collection with overlap merging, every `EditorState`
operation applied per selection in descending offset order, and undo grouping
across N carets.

The genuine risk is the IME, not the editing. The macOS input protocol assumes
a single `marked_range`, and `EntityInputHandler` is written against that
assumption throughout `input.rs`. What composition should even mean with N
carets is an open question here, not a known design being typed in.

## Vault

`inkra-vault` ships open, list and save. Saving is atomic — a temp file renamed
over the destination — and a note writes itself on a timer, on a note switch,
and as the application quits, not only on `⌘S`. A write that finds the file
changed underneath it preserves those bytes beside the note before replacing
them, so no write destroys what it did not put there.

Create, rename and delete landed on 2026-09-22. The vault is no longer
read-only: a note or a folder can be made from the tree, renamed in place, and
a note moved to the platform Trash. Every filesystem operation on one note is
ordered on that note's chain, so a rename cannot be overtaken by a write
decided before it. Deleting a *folder* is the one part left out, with its own
trigger.

The file watcher and diff-based reload landed on 2026-09-28: the vault follows
the disk, a clean open note reloads, and a note deleted on disk stays open. See
`superpowers/specs/2026-09-27-file-watcher-design.md`.

What the vault still leaves out — serializing writes *across* notes, buffer
eviction, reporting a failed open, and the native folder picker — is recorded
with a trigger each in the Deferred table of
`superpowers/specs/2026-09-20-inkra-vault-design.md`.

The unnamed launch buffer has left that table. It is closed, by removal rather
than by saving: a launch with no note builds no editor at all, and `AppShell`
renders an empty state, so there is no untracked buffer left to lose. See
`superpowers/plans/2026-09-21-empty-state.md`. Half of what that empty state
could not name is now real — `⌘N` makes a note from anywhere in the window —
and the other half, the folder picker, is still deferred.

What conflict detection did **not** close is the clean note. A buffer with no
unsaved edits never reaches a write, so it never reaches the check, and a note
changed on disk kept showing stale text until it was reopened. The watcher
closed that gap on 2026-09-28. See `superpowers/specs/2026-09-21-conflict-detection-design.md` for
what the check covers, and for the two gaps it leaves open inside its own
scope — the window between the stat and the rename, and two writes inside one
timestamp granule that happen to share a length.

Not repeated here, for the same reason as the section below: one table, one
place to change it. Read the trigger before building any of them; several look
independent and are not.

CRLF normalization has left this document. Its trigger was "file I/O or a paste
action, whichever lands first", and file I/O landed: `Fs::load` normalizes
before text can reach a `Buffer`. The CRLF *round-trip* — preserving a file's
original line endings on save — is a different item and is in that same
Deferred table.

## Markdown

Live preview shipped on 2026-09-23, in three slices: styled source,
concealment, then heading sizes. Slice 5, on 2026-09-24, hangs a revealed
heading's `#` in a margin left of the text, so revealing a heading moves no
text. Syntax hides except on the element the
caret or selection touches. The design, and each edge the implementation corrected, are in
`superpowers/specs/2026-09-23-markdown-live-preview-design.md`, with a
Deferred table of its own.

| Item | Trigger | Estimate |
|---|---|---|
| ~~Measure slice~~ | **Done 2026-09-24.** Figures under Editor, Measurements. | — |
| Checkboxes, a bar for quotes, code-block background with hidden fences | **Asked 2026-09-30.** Specced in `superpowers/specs/2026-09-30-markdown-blocks-design.md`: a gutter, not only a quad per row. S1 (quote gutters and bars) built on `feat/markdown-blocks-s1`; S2 code blocks and S3 tasks remain. | 4–5 d |
| ~~Clickable links~~ | **Shipped 2026-09-24**, with `inkra-index` and the backlinks panel. ⌘-click and ⌘↵ follow a link. See `superpowers/specs/2026-09-24-links-and-backlinks-design.md`, whose Deferred table holds what links left out. | — |
| Inline images | Images need an element beside text. | — |

Known limits, accepted:

- Revealing an inline element shifts the text after it on the same row, as
  in Obsidian. A heading's `#` hangs instead, except inside a quote or a list
  item, or indented.
- A heading's closing ` ##` stays visible, dimmed.
- Selecting up to the middle of a `**` and typing leaves the other delimiter.
  Backspace after a revealed element deletes its last delimiter character.
- Images, quotes, task boxes, code fences, tables and frontmatter keep their
  syntax, dimmed.
- Debug builds overstate shaping cost about 30×; measure in release.

## Application

`inkra-vault`, `inkra-markdown` and `inkra-index` exist now. Where full-text
search, panes, tabs and the command palette land is recorded in the "Where
future work lands" table of
`superpowers/specs/2026-09-20-modular-architecture-design.md`. Not repeated
here, so the two cannot drift.

- A note activation that lands after a rename field opened (from the tree's
  `enter`, or the palette's `Rename` while a load is held) takes focus from the
  field; the field stays open and keys go to the note. Fix by data: record the
  focus at activation, and move it on landing only if it has not moved since,
  keeping quick open's dismiss-lands-on-the-note. **Trigger:** a load slower
  than a keystroke (a network or synced vault, or a note large enough that
  `open_note` takes over ~100 ms), or the first report of a name typed into a
  note.
- A click elsewhere during a rename leaves the name field open without focus:
  commit or cancel on blur. This predates tabs: gpui's `track_focus` on the
  tree takes the focus on any press inside it. **Trigger:** the first report
  of a name field left open.
- The session file's folder comes from a platform data directory, not the
  macOS path `$HOME/Library/Application Support` that `bootstrap::launch`
  joins today (`superpowers/specs/2026-09-30-restore-tabs-design.md`, R1).
  **Trigger:** a non-macOS build.

## Startup performance

Paused on 2026-09-26 by the user: slices 5–7 took the first frame from
489–514 ms to 125–138 ms and indexing from 5.5–10.2 s to about 0.95 s at
50,000 notes (figures in `superpowers/specs/2026-09-24-search-design.md`,
`2026-09-25-open-first-design.md` and `2026-09-25-smart-quick-open-design.md`).
Two levers are identified and left, each with its trigger.

| Item | Trigger | Estimate |
|---|---|---|
| Overlap loads with parsing: `LOADS_PER_CORE` 2, or free a load slot when its read lands rather than when the foreground applies it. The pipeline is latency-bound per slot (~450 µs a cycle, slice 7's profile); 1 per core was set in slice 5, before the word parse made a load heavier. Probably 20–40 % off indexing, not measured | Indexing a real vault is noticed, or search results arrive late after launch | 0.5 d, with the release timeline before and after |
| ~~The window start-up floor~~ | **Profiled 2026-09-28**: a bare AppKit window with one Metal frame is on screen 156–167 ms after spawn; Inkra is 1–11 ms above it. The time is macOS's, not gpui's or the vault's, and `first frame` read ~27 ms late (it marked the second frame). **Mark fixed 2026-09-29**: it marks the first draw's start. See `startup-floor.md` | Keeping the process resident is the only lever left, a product choice |

## Index and search: the 2026-09-27 audit

The next quest, chosen by the user on 2026-09-27. An independent audit (advisor,
Opus) compared `inkra-index` and the search specs with tantivy, Lucene,
Meilisearch, charabia, git and Obsidian. Verdict: the word index is sound, a
small tantivy tuned for sub-microsecond inserts, and nothing forces a rewrite.
The risk is in what surrounds it. The file watcher leads, by the user's
priority: a note changed while the app runs must show in the app.

| Order | Item | Why | Fix |
|---|---|---|---|
| 1 | ~~Fast path in `IndexSync::reconcile`~~ | **Done 2026-09-27**: one build in flight, none for an mtime-only rescan, forgotten notes retried. See `superpowers/specs/2026-09-27-coalesced-reconcile-design.md` | — |
| 2 | The file watcher | External edits stay invisible until a restart; a clean open note shows stale text (Vault, above) | **W1 done 2026-09-27**: the tree, quick open, search and backlinks follow the disk. **W2b done 2026-09-28**: a clean open note reloads silently. **W2c done 2026-09-28**: an open note deleted on disk stays open under a banner, typing recreates it under its id, and a restore brings it back. Done. See `superpowers/specs/2026-09-27-file-watcher-design.md` |
| 3 | ~~`⌘⇧F` semantics, before slice 10~~ | **Done 2026-09-28**: `⌘⇧F` matches word starts after `⌘P`'s fold, reads only the notes the word index lists, and has no case or whole-word toggle. See `superpowers/specs/2026-09-28-search-word-start-design.md`. **Vault search shipped 2026-09-28**: the `⌘⇧F` picker, hits grouped by note, and a hit opens selected and in view. See `superpowers/specs/2026-09-28-vault-search-integration-design.md` | — |
| 4 | ~~Ranking key 2 (`index/src/find/rank.rs`)~~ | **Done 2026-09-28**: a text match ranks by BM25, rarity times a saturated, length-normalized count; the segment stores a length per ordinal. See `superpowers/specs/2026-09-28-ranking-idf-design.md` | — |
| 5 | ~~CJK and Thai words~~ | **Recorded 2026-09-27** in the smart-quick-open spec's Evolution table (branch `docs/cjk-limit`), with its trigger and the bigram fix | — |
| 6 | Persisted index | Only a spec for now. **Measured 2026-09-28**, release, 50,000-note nested vault (`startup_vault`), warm cache, runs 2–4 of `launch-figure.sh`: first frame 172–184 ms (the window floor above, not the vault), names ready 306–318 ms, indexed 1.18–1.26 s, longest turn 2.5 ms, RSS 349–352 MB (corpus 158 MB). Until `indexed`, quick open matches names only. A persisted words index would bring text matches to about the scan's end (~0.3–0.4 s) and cost a versioned format; the files are still read for the resident corpus. Not built: trigger is a user noticing text matches arrive late | Measure cold start first: the corpus stays resident, so a warm start still reads every file and saves only the parse, about 100 ms. If built: validate by `(mtime, size)` (the scan's `Entry` has no size yet), key by path plus stamp with inode for rename pairing only, embed a format version and a fold fingerprint so a Unicode table bump cannot silently miss notes, write the cache outside the vault |
| 7 | ~~`Sidebar::rebuild` walks every entry on each vault change~~ | Measured 2026-09-28 (watcher branch review): 1.1–2.2 ms of foreground work per real vault change at 50,000 notes, even with folders collapsed, against the 0.1 ms target. An echo that changes nothing no longer reaches it | **Done 2026-09-28**: a collapsed folder is skipped by binary search, a level's notes are copied as one run, and rows are built only when drawn. 0.005–0.051 ms. See `superpowers/specs/2026-09-28-sidebar-rebuild-design.md` |
| 8 | `⌘⇧F` walk over a huge note | The word-start spec's 10 MB walk case takes 48.7 ms against 16: `word_spans` alone costs about 27 ms. Trigger: a user-visible slow `⌘⇧F` on a huge note | Scan for the chars whose fold can start a term before splitting words. See `superpowers/specs/2026-09-28-search-word-start-design.md`, Evolution |

Additive later, no rewrite: typo tolerance (FST or bounded edit distance at
merge), NFKC folding, camelCase splitting, proximity re-ranking of the top K
from the corpus. Semantic search is additive as a second index, but fuse its
ranks with the lexical ones: a group below `Text` never shows under the
100-row cap.

## Ordering

No constraint is left. Soft wrap before virtualization was the last one, and
soft wrap shipped.

Everything else is free. Spending three weeks on this list before the vault,
links and search exist is a choice about what the application is, not a
sequencing requirement.
