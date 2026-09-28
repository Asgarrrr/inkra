# Vault search in the app (`⌘⇧F`)

Date: 2026-09-28. Status: **reviewed**, before code.

Slice 6 of [`2026-09-24-search-design.md`](2026-09-24-search-design.md) (the
search spec): the delegate, headers, `cmd-shift-f`, `HitActivated`,
`open_note` with a range, `pending_reveal`, the key tests and the shots. The
index side is built: `search`, `Searched::merge`, word-start matching, limits
and cancel ([`2026-09-28-search-word-start-design.md`](2026-09-28-search-word-start-design.md),
the word-start spec). This spec records the decisions the search spec leaves
open, and two changes to `inkra-index` that the rows need.

The search spec's D7 (picker), D8 (modal layer), D9 (rows and states) and D10
(opening a hit) hold, as amended by the word-start spec. Where this spec
differs, it says so under "Amendments".

A first draft was reviewed by a red team and a blue team. The blue team
checked each red finding in the code. Every confirmed finding is folded in
below; a **Corrected** note marks the change.

## Decisions taken with the user

- The run logic of quick open moves to `picker/` and both delegates use it
  (S1).
- A hit row shows its line number (S2).
- `⌘⇧F` opens with an empty field, as `⌘P` does.

## S1. `picker/runs.rs`: one run logic for two pickers

Quick open holds the cancel flag, `search_id`, `Scan`, `Run`, `next_run`,
an `IndexWatch` and `refresh_or_defer` (`quick_open/mod.rs`,
`quick_open/scan.rs`). Vault search needs the same rules: a keystroke
supersedes a background run, a refresh never cancels a scan in flight (D7),
and a notify during loads is throttled by `BUILDING_REFRESH`. Two users make
this a real seam (CLAUDE.md, "Abstract only at a real seam").

- A `Runs` struct in `picker/runs.rs` owns those fields and the state
  machine. Each delegate owns one.
- The `Run` → wait mapping stays in each delegate's `update_matches`. Quick
  open waits `NOTES_CHANGED_REFRESH` on `Run::Refresh` and keeps its
  `NotesChanged` subscription. Vault search never subscribes to
  `NotesChanged`; its `Run::Refresh` arm waits nothing, as `Run::Notify`,
  never `unreachable!`.
- The excerpt machinery (`rows_shown`, `ask_excerpts`) stays in quick open.
  A vault-search hit carries its `context`, so its rows are built when the
  results land.
- **The move is its own commit, with no behavior change.** The quick-open
  tests do not change and are its oracle. `./scripts/test-names.sh` is diffed
  before and after.

**As built (slice 1).** `Runs` is private state with a small API: `start`
returns a `Started` (`id: RunId`, `cancel`, `run`, `behind_user`),
`scanning(&started)` marks that run's scan in flight, `is_current(RunId)`
checks a result, `confirmed` drops a pending re-run. The scan in flight is
`Option<Scan { user, rerun }>`. The parts that must call back into the `Picker`
(`refresh_or_defer`, `landed`, `observe`) are generic over
`D: PickerDelegate + AsMut<Runs>`: a std trait, so no one-implementer trait.
The tests changed only where a moved field is read, through `delegate.runs`.

## S2. `inkra-index`: the line and the per-note cap

### `Hit.line`

`Hit` gains `line: u32`: the 1-based line of the hit's start in the saved
text. It is a logical line, not a wrapped row. The corpus is LF-only:
`normalize_line_endings` runs on every load, and a save writes the `Buffer`'s
LF text. So the count of `\n` before the start, plus one, is the line.

- `locate` stays as it is. A second pass over its returned ranges counts `\n`
  between consecutive starts.
- **Corrected.** The draft counted during the walk. `light` may still extend
  the last range when a later word merges into it (`find/highlight.rs:67-72`),
  so a range is final only once `locate` returns.
- The pass reads `cancel` every `CANCEL_EVERY` bytes (`find/terms.rs`), as
  `each_word` does. A set flag returns `Searched::default()`.
- Cost: O(bytes up to the last hit) of one note, a byte scan. It adds to the
  10 MB late-hit case, which is already over budget (48.7 ms against 16,
  word-start spec, Budget). `cargo bench -p inkra-index --bench search` runs
  before and after, and the word-start spec's table records the new figure.
- The "two boxes agree" oracle also checks each `line` against a naive count
  of `\n` in `text[..start]`.

### `Searched::capped`

A header shows how many hits its note gave. A note cut short must say so.

- **Corrected.** The draft showed `100+` when a note gave `HITS_PER_NOTE`
  hits. `Searched` has no per-note flag, so a note with exactly 100 hits
  would show `100+`. A note can also be cut at fewer than 100, by the global
  limit: `room` is `HITS_PER_NOTE.min(limit - hits.len())` (`run.rs:53`), and
  `Searched::merge` truncates a second time, across ranges.
- `Searched` gains `capped: Vec<EntryId>`, in vault order.
  - `search` pushes a note when its walk stopped past `room`
    (`lit.len() > room` before the truncate).
  - `merge` pushes the note of the last kept hit when the dropped hit that
    follows is of the same note, and keeps each range's `capped` notes that
    still have a kept hit.
- The oracle: `merge` over any split still equals one range, `capped`
  included.
- Invariant, stated at `merge`: the ranges of `chunks` are disjoint and
  contiguous, so the hits of one note never span two ranges.

## S3. The delegate: `vault_search/`

Modelled on `quick_open/`: `mod.rs` (the delegate), `search.rs` (the
background part), `rows.rs` (built rows), `render.rs`, and tests in their own
files.

**Per keystroke.** One `lookup`, then `search` over each range of
`chunks(num_cpus)`, then `Searched::merge`, limit 1,000. All of it runs off
the foreground, as `quick_open/search.rs`. A field whose text has no term
(`""`, `"   "`, `"[["`) runs no lookup. `search` cannot check that, since the
caller takes the lookup, so a delegate test does (word-start spec).

**A changing vault.** The delegate observes `IndexSync` through `IndexWatch`:
at once when built, throttled by `BUILDING_REFRESH` while loads land. A
refresh keeps the selected hit, keyed by `(note, range.start)`; else the first
hit is selected and scrolled to. A refresh never cancels a scan in flight
(S1).

**Rows.** Built once, when the results land, never per frame: `SharedString`s
and highlight runs. Every row is one `UI_LINE_HEIGHT` tall, so
`uniform_list` measures from row 0.

- **Header**, not selectable: the note's name, its folder dimmed, then its hit
  count, or `{n}+` when the note is in `Searched::capped`. The path comes from
  the vault's entry on landing, as the backlinks panel reads it. A hit whose
  note has no entry any more is dropped with its header.
- **Hit**: the line number, dimmed, in a column as wide as the largest listed
  line number, then `context` with `context_match` in `theme::match_text`.
  Control chars draw as spaces (`one_line`), one byte for one, so the
  highlight stays valid.

**States.** D9's table, plus the first row below:

| State | Shows |
|---|---|
| Before the first build | "Scanning the vault…", as quick open |
| Empty query | nothing; placeholder "Search notes…" |
| Text, no term, or no match | "No matches" |
| No match while indexing | "No matches in 12,340 of 50,000 notes indexed so far" |
| Indexing, with matches | footer "Indexing 12,340 of 50,000 notes" |
| `limit_reached` | footer "1,000+ results; refine the query" |

**Labels.** The dialog is "Search notes". A header's `row_label` is
"`name`, `N` matches"; a hit's is "line `N`: `context`".

## S4. The shell and the editor

**The key.** `cmd-shift-f` → `SearchVault`, in the `AppShell` context, as
`cmd-p`. It goes through `toggle_picker`: it replaces quick open, a second
press dismisses it, and it does nothing when no vault is open.

**Opening a hit.** The delegate emits `HitActivated { note, range }`. The
shell's `show_hit` follows D10:

- The note is showing: select the range and reveal it, without rebinding,
  as `show_note` avoids reopening. The pending activation of any other note
  is dropped: the pick is the last word (D8).
- Else `open_note(id, Some(range))`. Inside the activation's `update_in`,
  right after `open_buffer`, the editor selects the range. So the first
  layout is shaped with the selection, and its conceals reveal around it.
- A superseded activation is a dropped `Task`, so it applies no stale range.

**The editor.** `move_cursor` is `pub(super)` (`editor/motion.rs:22`), so the
shell cannot call it.

- A new `pub(crate) EditorView::select_range(range, window, cx)` clips both
  ends with `Buffer::clip` and selects through `move_cursor`. The saved
  offsets may not fit a note with unsaved edits; the clip keeps them valid.
- The shell calls it as `editor.update(cx, |editor, cx|
  editor.select_range(range, window, cx))`, with the `window` of its own
  `update_in`. **Corrected:** the draft implied a nested `update_in`, which
  does not compile: `Context` does not implement `VisualContext`.
- `EditorView` gains `pending_reveal`. `open_buffer` clears `last_text_top`,
  so `reveal_cursor` cannot run before the first paint. The flag is consumed
  after the first paint with a non-zero viewport: it calls `reveal_cursor`
  and notifies. The note shows at its top for one frame, then scrolls (D10).

## Tests

Headless `#[gpui::test]` against `MemFs`, each key with
`simulate_keystrokes`:

- `cmd-shift-f` opens the picker; a second press dismisses it; it replaces
  an open quick open; it does nothing without a vault.
- `up` and `down` skip headers and stop at the ends.
- `enter` opens the selected hit; `escape` dismisses and restores focus;
  `enter` typed before the results land confirms the first hit.
- A stale result is dropped. With `num_cpus` overridden, the merged result
  equals one range.
- A blank or `[[` query runs no lookup.
- A hit far down a long note opens with its range selected and the caret
  visible. A hit in the note showing selects without rebinding: the
  editor keeps the same `Buffer` entity and its undo stack.
- Trashing a listed note removes its header and hits.
- A refresh during loads keeps the selected hit.
- `inkra-index`: `line` and `capped` in the oracles (S2); `cargo mutants` on
  `index` after S2.

**Shots**, each looked at: `vault-search-results` (headers, line numbers, a
`{n}+` header), `vault-search-empty`, `vault-search-no-match`,
`vault-search-indexing`, `vault-search-limit`, `vault-search-scanning`, and
`vault-search-opened`, the editor after a hit far down a note opens.

## Slices

Each merges alone with `check.sh` green.

1. **`Runs`**: the move of S1, no behavior change.
2. **The index**: `Hit.line`, `Searched::capped`, the oracles, the bench
   figure.
3. **The picker**: the delegate, rows, states, `cmd-shift-f`, the key tests,
   the shots. Confirming a hit opens its note, at the top.
4. **Opening at the hit**: `HitActivated`, `show_hit`, `open_note` with a
   range, `select_range`, `pending_reveal`, their tests, the
   `vault-search-opened` shot.

## Deviations from Zed's Text Finder

`vendor/zed-reference/crates/search/src/text_finder/` is the reference. Not
taken, for a note app in v1:

- Collapsible headers.
- Streaming results: the search spec's Evolution row "Streaming results".
- A preview of the hit.
- Multi-select and its checkboxes.

## Amendments to the search spec

- **D9**: the table gains "Before the first build: Scanning the vault…" for
  both pickers. A capped note's header shows `{n}+` from `Searched::capped`.
- **D1 / the word-start spec's S2**: `Hit` gains `line`; `Searched` gains
  `capped`.
- **Slices**: slice 6 is split as above.
- **Roadmap**: when slice 4 ships, `docs/roadmap.md` marks vault search as
  shipped, and CLAUDE.md drops "(vault search not yet built)".
