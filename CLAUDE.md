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
  dependency direction. Views talk through events (`NoteActivated`), never by
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

- verify: `./scripts/check.sh` — build, test, clippy `-D warnings`, fmt, deny,
  and the no-gpui-below-app check. Green is the definition of done.
- fmt: `cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown -p inkra-index`.
  **Never `cargo fmt --all`**: it rewrites the vendored gpui source. A new
  workspace member joins this list and `check.sh`'s in the same change.
- look: `cargo run -p inkra-app --features shot --example shot [scene|--list]`
  renders scenes off-screen to `target/shots/*.png`. `check.sh` renders nothing.
- run: `cargo run -p inkra-app [path]`
- tests moved: `./scripts/test-names.sh` — diff before and after splitting a
  file; a forgotten `mod` line drops tests and still compiles green.
- mutants: `cargo mutants` after changing `text`, `editor`, `markdown` or `index` logic.
  Zero missed is the bar; exit 3 on the known `Buffer::clip` timeout is expected.
- bench: `cargo bench -p inkra-text -- --quick` for `Buffer` hot paths,
  `-p inkra-markdown` for `parse` — before/after comparison, not a gate.
- `vendor/zed-reference` is gitignored; `./scripts/vendor-gpui.sh` restores it
  at the pinned commit. Nothing builds before it runs.

## Architecture

- `inkra-text` → `inkra-editor` → `inkra-app`, one way. `inkra-vault` sits
  beside the chain: `app` depends on it, it depends on no workspace crate.
  `inkra-markdown` depends on `inkra-text` only; `app` depends on it.
  `inkra-index` depends on `vault`, `markdown` and `text`; `app` depends on it.
- `text`, `editor`, `vault`, `markdown` and `index` have no gpui dependency; `check.sh`
  enforces it and a new crate below `app` joins that check.
- Where code goes:
  - no layout, filesystem or clock → `text` (the text) or `editor` (editing it)
  - parsing markdown into styled ranges → `markdown`; never a colour or a pixel
  - resolving links, backlinks, search → `index`; it reads no file
  - pixels, fonts, a `Window`, a clock, the platform → `app`
  - reading or writing a file → `vault`, behind `Fs`; `app` opens no file
  - binding a file to a `Buffer` → `app` (`NoteStore`); `vault` never sees a `Buffer`
  - a colour or text size → `app/src/theme.rs`, never a literal
  - network → `app`, until a crate earns it
- `inkra-app` is a library with two entry points (`main.rs`, `examples/shot.rs`).
  Views stay `pub(crate)`. An example touching a feature-gated module needs
  `required-features`, or `cargo test` breaks.
- Area rules load on demand from `.claude/rules/` when you read the matching
  files: editor, shell and sidebar, vault, app tests, gpui.

## Code

- Offsets are `ByteOffset`, `Utf16Offset` and, in layout, `DisplayOffset`;
  never mix them, never add a char offset. Move and delete by grapheme.
- Mutate a buffer only through `Buffer::transact_at`. Call
  `finalize_transaction` on **both** sides of a gesture that must not merge
  (paste, cut), and on neither side when it edits nothing.
- `Buffer` assumes LF: text entering from outside (load, paste) goes through
  `inkra_vault::normalize_line_endings`.
- Only `app` reads a clock; `text` and `editor` take the `Instant` as an argument.
- Infallible buffer operations panic on caller bugs. `VaultError` is the one
  error enum.
- An external dependency used by two crates is declared in
  `[workspace.dependencies]`.
- Never disable a lint, test or type check to get green.

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
- `docs/superpowers/specs/2026-09-23-soft-wrap-design.md` — rows, wrap, caret
- `docs/superpowers/specs/2026-09-24-links-and-backlinks-design.md` — `inkra-index`,
  resolution, `IndexSync` (was `LinkSync`), following links, the backlinks panel
- `docs/superpowers/specs/2026-09-24-search-design.md` — quick open, vault
  search, `NoteIndex`/`IndexSync`, the picker and modal layer (vault search not yet built)
- `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md` — `⌘⇧F`
  in the app: `Runs`, the delegate, `Hit.line`, `Searched::capped`, opening a hit
- `docs/superpowers/specs/2026-09-25-smart-quick-open-design.md` — `⌘P` by word
  start on names and text, the word index, `find_notes`
- `docs/superpowers/specs/2026-09-25-open-first-design.md` — opening before the
  scan, the scan worker, `Walk`/`applied`
- `docs/superpowers/specs/2026-09-27-file-watcher-design.md` — the watcher:
  watch, scoped rescans, reload, gone notes
- `docs/superpowers/specs/2026-09-27-coalesced-reconcile-design.md` — `IndexSync`
  builds, mtime fast path
- `docs/superpowers/specs/2026-09-28-sidebar-rebuild-design.md` — the tree's
  walk: skipped subtrees, note runs, rows built when drawn
- `docs/superpowers/specs/2026-09-28-search-word-start-design.md` — `⌘⇧F` by
  word start, the word-index prefilter, `word_prefix`
- `docs/superpowers/specs/2026-09-28-ranking-idf-design.md` — key 2 of a text
  match: BM25, per-ordinal note lengths
- `docs/superpowers/specs/2026-09-21-autosave-design.md` and
  `2026-09-21-conflict-detection-design.md` — the save path
- `docs/superpowers/specs/2026-09-20-inkra-vault-design.md` — `Fs`, identity
- `docs/superpowers/specs/2026-09-22-create-rename-delete-design.md` — tree ops
- `docs/superpowers/specs/2026-09-21-offscreen-shots-design.md` — `shot/`
- `docs/superpowers/specs/2026-09-20-modular-architecture-design.md` — crates
- `docs/startup-floor.md` — the launch's floor, the WindowServer oracle,
  why `first frame` reads late
- `docs/roadmap.md` — deferred work and the trigger for each
- `docs/gpui-notes.md`, `docs/buffer-model.md`, `docs/gpui-accessibility.md`,
  `docs/tooling.md`
