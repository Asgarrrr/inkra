# Accessibility: where GPUI stands

Checked 2026-09-20 against `gpui` 0.2.2 from crates.io and Zed at
`526c95d474b2bf800e95aab30222fa0341f08e19`. Re-check both if either moves.

Screen-reader support is a **requirement** for Inkra, so this is the finding
that decides whether GPUI can replace Tauri — not multi-line layout, not
input handling.

## Two gpui builds, same version number

| | latest | accessibility |
|---|---|---|
| crates.io `gpui` | 0.2.2, 2025-10-22 | none — no `accesskit` dependency at all |
| Zed in-tree `gpui` | also `0.2.2` | AccessKit, widget level only |

gpui is developed inside the Zed monorepo and published rarely. The crates.io
release is a snapshot roughly eleven months stale, and it carries the same
version string as the in-tree source, so the version tells you nothing about
which one you have.

`Cargo.toml` therefore points at the git rev, not the published crate. That
buys the widget layer and costs semver: the pin is a commit of a monorepo that
changes daily, and it drags in Zed's own crates (`gpui_platform`, `gpui_macos`,
…) plus their forks of `font-kit` and `scap`.

## What the AccessKit bridge actually does

`crates/gpui/src/window/a11y.rs`, 888 lines. Every node property it sets:

```
set_label  set_focus  set_focusable  set_children
set_active_descendant  set_window_title
```

It never reads the input handler — zero references. Elements opt in through
`.role(…)`, `.aria_label(…)` and `.accessibility_id(…)`.

Absent: `text_selection`, `character_lengths`, `word_lengths`, line metrics.
Those are what a screen reader needs to walk a document by character, word or
line and to follow the caret as it moves.

The newer `EntityInputHandler` methods that look like accessibility hooks —
`text_length_utf16`, `set_selected_text_range`, `text_input_configuration`,
`text_input_editable_range` — are consumed by `platform.rs` for platform text
services (on-screen keyboards, autocorrect), not by the AccessKit bridge.

## Consequence

An editor built on GPUI today can announce itself as
`Role::MultilineTextInput` with a label. Its content is an opaque blob:
no caret position, no selection, no navigation inside the text.

This is not "GPUI needs polish". The mechanism does not exist, at Zed's HEAD.

Two things that bound the problem:

- The gap is in gpui's bridge, not in AccessKit, which has a complete text node
  model. The work is well-defined: build text nodes from the shaped lines plus
  the selection, and keep them updated.
- But it means contributing to Zed's monorepo, against a moving target with no
  stability contract, as a prerequisite for shipping Inkra. Tauri gives the
  browser's accessibility tree for free.

## Not yet verified

This is a source-level conclusion. It has **not** been confirmed with
VoiceOver. To close that gap:

```
cd ~/.cargo/git/checkouts/zed-*/526c95d
cargo run -p gpui --example a11y
```

Turn VoiceOver on (⌘F5) and check what it announces. The example covers
buttons, a switch and a list — if text navigation is genuinely missing, no
amount of correct widget markup will expose document content.
