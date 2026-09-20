# Accessibility: where GPUI stands

> **Status: accepted risk (2026-09-20).** GPUI is adopted for Inkra with this
> gap open and unresolved. The measurements below still hold; what changed is
> the decision, not the finding. Inkra therefore ships without screen-reader
> access to document text until someone closes the gap in gpui's AccessKit
> bridge — a known debt with a known owner-less cost, carried deliberately.
>
> Re-run `scripts/ax-text-probe.swift` before acting on anything here: the
> answer changes the day gpui's bridge gains text properties, and that is the
> cheapest path to paying the debt off.

Checked 2026-09-20 against `gpui` 0.2.2 from crates.io and Zed at
`526c95d474b2bf800e95aab30222fa0341f08e19`. Re-check both if either moves.

Screen-reader support was stated as a **requirement** for Inkra, which made
this — not multi-line layout, not input handling — the finding that decided
the GPUI-versus-Tauri question. The decision went to GPUI anyway. The
requirement did not disappear with it; it became the debt described above.

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

## Confirmed at runtime

Measured, not inferred. `scripts/ax-text-probe.swift` queries the running
editor's real macOS accessibility tree through `AXUIElement`:

```
found text node after 2 poll(s)
role: AXTextArea
title: Document

--- what a screen reader would read ---
AXValue: ABSENT
AXNumberOfCharacters: ABSENT
AXSelectedText: ABSENT
AXSelectedTextRange: ABSENT
AXVisibleCharacterRange: ABSENT
AXInsertionPointLineNumber: ABSENT

AXFocusedUIElement role: AXWindow
```

The markup half works. `.id(…).role(Role::MultilineTextInput).aria_label(…)`
produces a correctly-roled, correctly-labelled `AXTextArea` covering the
editor's bounds — so AccessKit is wired up and reaching macOS.

Every text attribute on it is absent. VoiceOver would find "Document, text
area" and be unable to read one character of it, report the caret, or report
the selection. Keyboard focus also resolves to the window rather than the text
area.

This matches the bridge exactly: it sets label, focus, focusable, children and
window title, and nothing else.

Two traps when re-running the probe:

- The tree is built **asynchronously**. Reading it immediately after launch
  returns a tree with no `AXTextArea` at all — a false negative. The probe
  polls for up to 20 s rather than sleeping a fixed amount.
- Without `.role(…)` the editor appears as a bare `AXGroup`. The text
  attributes that show up on a naive tree walk belong to the title bar's
  10×16 px `AXStaticText`, not to the document.
