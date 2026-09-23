---
paths:
  - "crates/app/src/**/tests/**"
  - "crates/app/src/**/tests.rs"
  - "crates/app/tests/**"
  - "crates/app/src/test_support.rs"
---

# app tests

- A test reaches only what its window is rooted at. `with_editor`/`dispatching`
  root at `EditorView` and cannot prove an `AppShell` binding (`cmd-s`) or
  where an activated note goes; use `open_vault`/`open_shell` for those.
- `MemFs` gates make races reproducible: `block_load` (editor and `current`
  disagree), `block_save` (two writers on one note), `make_unwritable`,
  `make_uncreatable`, `make_untrashable`.
- Test write *ordering* with a plain `#[test]` driving `write_through`, not
  `#[gpui::test]`: the test scheduler is single-threaded. Test the wiring with
  `flush_all` called directly, and hold its future.
- Quit through `TestAppContext::quit`; `cx.quit()` in an update fires nothing.
  Never assert the 200 ms shutdown timeout.
- Process behaviour before a window exists belongs in `app/tests/cli.rs`.
- Pixel-dependent tests measure pitches rather than computing them (gpui renders
  22.5 px where `phi()` predicts 22.65).
- Headless glyphs advance `0.6 × font_size`; wrap fixtures avoid breaks landing
  exactly on the column edge.
