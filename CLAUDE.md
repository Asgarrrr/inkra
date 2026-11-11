# Inkra

A note application in Rust on GPUI: a vault of markdown files, links and
backlinks, search, panes. The editor is one component of it. GPUI was adopted
over Tauri on 2026-09-20, accessibility gap included.

Everything in this repo — docs, comments, commits, identifiers — is English.

## How we build

- **Zed is the reference.** Before designing something gpui or Zed already
  solves, read how Zed does it in `vendor/zed-reference/`. Follow it unless our
  case is simpler (a note app, not an IDE); record every deviation and its
  reason in the feature's spec.
- **Abstract only at a real seam**: two implementations exist, or one real and
  one test fake (`Fs` with `RealFs`/`MemFs`). No trait with one implementer, no
  layer for a future that is not on the roadmap.
- **Modular by seams, not by indirection.** Crates have one role and one
  dependency direction. Views talk through events (`FileActivated`), never by
  holding each other's entities, so a new pane plugs in without touching the
  others. Extensibility is a property of these seams; do not build a plugin
  system ahead of need.
- **One module per concern.** Split a file when it mixes two subjects or grows
  past ~400 lines (`sidebar/{tree,edit,render}.rs` is the pattern). Tests live
  in their own file.
- **Performance is a requirement, measured.** No per-frame or per-keystroke work
  that scans the whole document more than once; profile before optimizing.
- **Comments are rare and short**: one line, for a why the code cannot show (an
  invariant, a panic hazard, a gpui quirk). Rationale belongs in `docs/`.
- **Don't reinvent.** Before adding a helper, type or module, have an `Explore`
  subagent on `haiku` search for an existing one, so the search stays out of
  the main context.
- Large or unclear features get a spec in `docs/superpowers/specs/` first,
  reviewed before code.

## Commands

- verify: `./scripts/check.sh` — build, test, clippy `-D warnings`, fmt, deny, typos,
  the no-gpui-below-app check, the crate graph, the ast-grep rules in `.ast-grep/rules/`,
  the clock check, the line ceilings. Green is the definition of done.
- fmt: `cargo fmt`, from the repo root, formats every workspace member.
- look: `cargo run -p inkra-app --features shot --example shot [scene|--list]`
  renders scenes off-screen to `target/shots/*.png`. `check.sh` renders nothing.
- run: `cargo run -p inkra-app [path]`
- tests moved: `./scripts/test-names.sh` — diff before and after splitting a
  file; a forgotten `mod` line drops tests and still compiles green.
- mutants: after changing `text`, `editor`, `markdown`, `index`, `tabs` or `layout` logic,
  `git diff main... > $TMPDIR/m.diff && cargo mutants -j 4 --in-diff $TMPDIR/m.diff`.
  A whole crate is ~1000 mutants run one by one, near two hours; the diff is a few
  dozen. Run plain `cargo mutants` once, before the PR.
  Zero missed is the bar; exit 3 on the known `Buffer::clip` timeout is expected.
  After changing `vault` logic, run the diff command with
  `-p inkra-vault --test-package inkra-vault -f 'crates/vault/src/**/*.rs'`.
- compare: `./scripts/bench-compare.sh [--sizes 500,5000,50000] [--runs 5] [--apps inkra,obsidian]`
  — Inkra vs Obsidian measured from outside; method and results in `docs/benchmarks.md`.
  Quit Obsidian first; it runs in an isolated profile and never touches the user's.
- bench: `cargo bench -p inkra-text -- --quick` for `Buffer` hot paths,
  `-p inkra-markdown` for `parse` — before/after comparison, not a gate.
- gpui is a git dependency pinned by `rev`; cargo fetches it. `vendor/zed-reference`
  is gitignored, for reading only: `./scripts/vendor-gpui.sh` checks out the rev in `Cargo.lock`.

## Architecture

- `inkra-text` → `inkra-editor` → `inkra-app`, one way. `inkra-vault` sits
  beside the chain: `app` depends on it, it depends on no workspace crate.
  `inkra-markdown` depends on `inkra-text` only; `app` depends on it.
  `inkra-index` depends on `vault`, `markdown` and `text`; `app` depends on it.
  `inkra-tabs` and `inkra-layout` depend on no workspace crate; `app` depends on both.
- `text`, `editor`, `vault`, `markdown`, `index`, `tabs` and `layout` have no gpui dependency; `check.sh`
  enforces it for every workspace member except `inkra-app`.
- Where code goes:
  - no layout, filesystem or clock → `text` (the text) or `editor` (editing it)
  - parsing markdown into styled ranges → `markdown`; never a colour or a pixel
  - resolving links, backlinks, search → `index`; it reads no file
  - deciding which notes to load, which load is superseded, when words merge → `index` (`LiveIndex`)
  - which tab is active, what a close activates, which place back and forward reach, what `⌘⇧T`
    reopens, where a dragged tab lands → `tabs`; what a place holds → `app`
  - the group tree, which docks show, splits and resizes → `layout`; which dock holds a view,
    which view is active, where a moved view lands, what a view draws → `app`
  - pixels, fonts, a `Window`, a clock, the platform → `app`
  - reading or writing a file → `vault`, behind `Fs`; `app` opens no file
  - binding a file to a `Buffer` → `app` (`NoteStore`); `vault` never sees a `Buffer`
  - a colour or text size → `app/src/theme.rs`, never a literal
  - network → `app`, until a crate earns it
  - a reusable piece of chrome (row, icon, button) → `app/src/ui/`; a view composes it and keeps its own handlers
- `inkra-app` is a library with two entry points (`main.rs`, `examples/shot.rs`);
  `examples/seed_session.rs` writes the session `scripts/screenshot.sh` opens. Views stay `pub(crate)`. An example touching a feature-gated module needs
  `required-features`, or `cargo test` breaks.
- Area rules load on demand from `.claude/rules/` when you read the matching
  files: editor, shell and sidebar, vault, app tests, gpui.

## Code

- Offsets are `ByteOffset`, `Utf16Offset` and, in layout, `DisplayOffset`;
  never mix them, never add a char offset. Move and delete by grapheme.
- Mutate a buffer only inside `Buffer::transact_at`, through its `Txn`. A
  one-step gesture (paste, cut, `apply_diff`) uses `Buffer::transact` instead:
  it merges with neither neighbour and is a no-op when it edits nothing.
  `finalize_transaction` is only for sealing without an edit.
  Exception: a tab switch's, an open's, a back's capture, or a group switch's pin, calls it
  on the note left even with no edit, so typing on both sides of `⌃Tab` is two undo steps (tabs spec, A3).
- `Buffer` assumes LF: text entering from outside (load, paste) goes through
  `inkra_vault::normalize_line_endings`.
- Only `app` reads a clock; `text` and `editor` take the `Instant` as an argument.
- Infallible buffer operations panic on caller bugs. `VaultError` is the one
  error enum.
- An external dependency used by two crates is declared in
  `[workspace.dependencies]`.
- Never disable a lint, test or type check to get green.
- A justified suppression is `#[expect(lint, reason = "...")]`; clippy denies `#[allow]`,
  and an expectation that stops firing fails the build.

## Testing

- `text`, `editor`, `vault`: `#[test]` plus `proptest`; commit the
  `.proptest-regressions` files. A test that needs no `Window` stays out of `app`.
- `app`: headless `#[gpui::test]` against `MemFs`, never the real disk.
- Every key binding gets a `simulate_keystrokes` test.
- Tests assert state, so a layout defect passes them: after adding a row, a
  message or a gesture to a pane, render the scene and look at it.

## Docs

Read the spec before changing its area:

- `docs/superpowers/specs/2026-09-23-markdown-live-preview-design.md` —
  markdown styling and concealment
- `docs/superpowers/specs/2026-09-30-markdown-blocks-design.md` — quote
  gutters and bars, code backgrounds, task boxes
- `docs/superpowers/specs/2026-11-07-code-blocks-design.md` — fenced and
  indented blocks: fence conceals, `CodeBlock`, `CODE_BLOCK`, the background, the label
- `docs/superpowers/specs/2026-11-07-code-block-scroll-design.md` — a code block's
  horizontal scroll; replaces decision 5 of the code-blocks spec
- `docs/superpowers/specs/2026-11-11-code-highlighting-design.md` — highlighting
  in code blocks: the library choice, token classes, the per-block cache
- `docs/superpowers/specs/2026-09-23-soft-wrap-design.md` — rows, wrap, caret
- `docs/superpowers/specs/2026-09-24-links-and-backlinks-design.md` — `inkra-index`,
  resolution, `IndexSync` (was `LinkSync`), following links, the backlinks panel
- `docs/superpowers/specs/2026-11-08-backlink-context-on-demand-design.md` — links
  as `NoteRef`s, one shared slice per note, contexts read on demand by the panel, cache format 3
- `docs/superpowers/specs/2026-11-11-link-landing-design.md` — following `[[Note#Heading]]`
  and a backlink row to their place: the heading fold, `Spot`, `Reveal::Top`, `LinkActivated`
- `docs/superpowers/specs/2026-09-24-search-design.md` — quick open, vault
  search, `NoteIndex`/`IndexSync`, the picker and modal layer
- `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md` — `⌘⇧F`
  in the app: `Runs`, the delegate, `Hit.line`, hits grouped by note, opening a hit
- `docs/superpowers/specs/2026-09-25-smart-quick-open-design.md` — `⌘P` by word
  start on names and text, the word index, `find_notes`
- `docs/superpowers/specs/2026-09-25-open-first-design.md` — opening before the
  scan, the scan worker, `Walk`/`applied`
- `docs/superpowers/specs/2026-09-27-file-watcher-design.md` — the watcher:
  watch, scoped rescans, reload, gone notes
- `docs/superpowers/specs/2026-09-27-coalesced-reconcile-design.md` — `IndexSync`
  builds, mtime fast path
- `docs/superpowers/specs/2026-09-29-index-scheduler-design.md` — `LiveIndex`:
  loads, supersede, own-save echo, the startup settle; `IndexSync` spawns
- `docs/superpowers/specs/2026-09-30-index-turn-effects-design.md` —
  `LiveIndex::turn`: `Turn`, `Effects`, the turn's end, `revision`, when `IndexSync` notifies
- `docs/superpowers/specs/2026-09-30-index-status-once-design.md` — the index status
  (scanning, indexing, ready), answered once for quick open, vault search and backlinks
- `docs/superpowers/specs/2026-11-06-persisted-index-design.md` — the cache
  file, the seed, text read on demand, the writer, Rebuild, the scrub
- `docs/superpowers/specs/2026-11-06-file-kinds-design.md` — assets in the vault
  and the tree, `FileKind`, `Images`, `Shown::Image`, session v3, links to files
- `docs/superpowers/specs/2026-09-28-sidebar-rebuild-design.md` — the tree's
  walk: skipped subtrees, note runs, rows built when drawn
- `docs/superpowers/specs/2026-09-29-sidebar-rows-design.md` — `Rows`: the
  name field's slot, item vs row indices, `neighbour` and `step`
- `docs/superpowers/specs/2026-09-28-search-word-start-design.md` — `⌘⇧F` by
  word start, the word-index prefilter, `word_prefix`
- `docs/superpowers/specs/2026-09-28-ranking-idf-design.md` — key 2 of a text
  match: BM25, per-ordinal note lengths
- `docs/superpowers/specs/2026-11-11-vault-search-ranking-design.md` — `⌘⇧F` groups
  ranked by BM25: the score, where the order is made, the limit
- `docs/superpowers/specs/2026-11-11-search-underscore-words-design.md` — part starts after `_`, trimmed parts, the joined scan, the 64-part bound
- `docs/superpowers/specs/2026-09-29-text-input-design.md` — `TextInput`, the
  core `EditorView` and `NameField` share: gates, protocol, one binding set
- `docs/superpowers/specs/2026-11-11-word-wise-editing-design.md` — `⌥←`/`⌥→`
  by UAX #29 word, `⌥⌫`/`⌥⌦`, double and triple click, the word drag
- `docs/superpowers/specs/2026-09-29-query-run-design.md` — one query entry
  in `inkra-index` (`find_notes`/`search` with `Jobs`), `IndexDelegate`, `Runs`
  owning the run, `IndexWatch::observe`
- `docs/superpowers/specs/2026-09-30-jobs-module-design.md` — `Job`, `Jobs`, `Serial`: their
  own module in `inkra-vault`
- `docs/superpowers/specs/2026-09-30-runs-testable-design.md` — `Ledger`: which
  run is current, D7's deferred re-run, the wait; `Runs` drives it, `begun` in tests
- `docs/superpowers/specs/2026-09-30-picker-owns-landing-design.md` — `Picker`
  lands every answer: `Update`, `apply_matches`, `Arrival`, `Ran`, the `None` rule
- `docs/superpowers/specs/2026-09-29-command-palette-design.md` — `⌘⇧P`: the
  listed actions, the hide list, `word_starts`, the handle a command runs on,
  keys, recent order
- `docs/superpowers/specs/2026-09-29-editor-geometry-design.md` — `Viewport`:
  the text top, the scroll request, a reload's top line; `vertical`, `hit_test`
- `docs/superpowers/specs/2026-09-29-row-text-design.md` — `row_text`: lit
  one-line row text, the path split, `name_row`
- `docs/superpowers/specs/2026-11-11-plain-row-text-design.md` — search and
  backlink rows as prose: `Plain` shared, what stays raw, the window
- `docs/superpowers/specs/2026-09-29-names-delete-design.md` — the name table
  `find` reads, why the fuzzy matcher is gone
- `docs/superpowers/specs/2026-09-29-scan-session-design.md` — `Scans` in
  `inkra-vault`: the re-key log, the swap's order, coalescing, `Watcher` as a
  drop guard
- `docs/superpowers/specs/2026-09-29-note-session-design.md` — one note's
  lifecycle in `NoteStore`: `Sessions`, `Lifecycle`, `Queue`, the live set
- `docs/superpowers/specs/2026-09-29-tabs-design.md` — tabs: `inkra-tabs`,
  `Place`, per-tab history, parked anchors, one rebound `EditorView`
- `docs/superpowers/specs/2026-09-30-tab-views-one-owner-design.md` — `Parked`: an inactive
  tab holds its own parked view
- `docs/superpowers/specs/2026-09-30-no-vault-mode-design.md` — no vault open: `VaultPanes`,
  what `⌘N` does then
- `docs/superpowers/specs/2026-09-30-restore-tabs-design.md` — the session file,
  its one writer and throttle, `Restore`'s states, the landing predicate, the window frame
- `docs/superpowers/specs/2026-10-01-shell-chrome-design.md` — the `ui` components (`ListItem`,
  `Tab`, `Icon`, `Indicator`, `IconButton`, `PanelHeader`, `Assets`), row overlays, Lucide
  icons; the panel toggles (`cmd-\`, `cmd-|`), headers and hiding; the chrome slices
- `docs/superpowers/specs/2026-11-04-obsidian-chrome-design.md` — Obsidian chrome: surfaces, the split title row, browser tabs, the strip's history arrows and menu, the backlinks header
- `docs/superpowers/specs/2026-11-05-docks-design.md` — docks: `inkra-layout`, `Docks`, headers and the icon bar,
  the move actions, the menu, the drag, the session's `docks` field
- `docs/superpowers/specs/2026-11-05-splits-design.md` — splits: `Layout`'s tree of groups in the centre
  and docks, L1–L7, the title row from the top groups, the gestures, session v2
- `docs/superpowers/specs/2026-11-04-themes-components-design.md` — semantic tokens, `GlobalTheme`, `ui::Color` and `Label`,
  vault themes in `.inkra/`, `settings.json`, hot reload, the theme picker
- `docs/superpowers/specs/2026-09-21-clipboard-design.md` — copy, cut, paste
- `docs/superpowers/specs/2026-09-21-autosave-design.md` and
  `2026-09-21-conflict-detection-design.md` — the save path
- `docs/superpowers/specs/2026-11-02-quit-and-failures-design.md` — close is quit, the chained pre-quit flush, the unsaved prompt, `moves`, save retries, the notice
- `docs/superpowers/specs/2026-11-02-type-safety-hygiene-design.md` — the audit's type and hygiene
  batches: opaque offsets, positions vs lengths, typed UTF-16 ranges at the protocol boundary
- `docs/superpowers/specs/2026-11-02-perf-batch-design.md` — performance batch 2: the
  baseline, carried shaped lines, painting inside the content mask
- Dropped candidates, not to be proposed again unchanged:
  `2026-09-30-note-index-read-interface-design.md`, `2026-09-30-own-save-echo-design.md`
- `docs/superpowers/specs/2026-09-20-inkra-vault-design.md` — `Fs`, identity
- `docs/superpowers/specs/2026-11-11-open-a-vault-design.md` — `OpenVault`, `⌘O`, the empty
  state's button, a fresh `AppShell` by `replace_root`, the leave path, the last vault
- `docs/superpowers/specs/2026-09-22-create-rename-delete-design.md` — tree ops
- `docs/superpowers/specs/2026-11-11-tree-organizer-design.md` — moving files: `Move to…`, drag,
  which links break, the rewrite plan per source note, the written form, folder trash deferred
- `docs/superpowers/specs/2026-09-21-offscreen-shots-design.md` — `shot/`
- `docs/superpowers/specs/2026-09-20-modular-architecture-design.md` — crates
- `docs/startup-floor.md` — the launch's floor, the WindowServer oracle,
  why `first frame` reads late
- `docs/roadmap.md` — deferred work and the trigger for each
- `docs/gpui-notes.md`, `docs/buffer-model.md`, `docs/gpui-accessibility.md`,
  `docs/tooling.md`
