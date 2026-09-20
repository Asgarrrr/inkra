# Roadmap — deferred work, and what brings it back

Nothing here is scheduled. Each item was taken out of a shipped plan on
purpose, and each carries the **trigger** that makes it worth doing. An item
with no trigger fired is code without a reason.

Sizes are estimates for one person at this repo's verification bar: proptest
over edit sequences in `text`, one `simulate_keystrokes` test per binding,
`check.sh` green, and a manual render pass because `check.sh` renders nothing.
They are estimates, not commitments — soft wrap and multi-cursor carry most of
the uncertainty, for reasons given below.

## Editor

| Item | Trigger | Estimate | Blocked by |
|---|---|---|---|
| Scroll to reveal the cursor | Fired. Typing past the bottom edge loses the caret today. | ~0.5 d | — |
| CRLF normalization | File I/O or a paste action, whichever lands first. | ~0.5 d | `inkra-vault` |
| Soft wrap | A real note measured to need it. | 3–5 d | — |
| Virtualization | A profile on a real note showing per-frame shaping or `SharedString` allocation costs anything. | 2–3 d | Soft wrap |
| Multi-cursor | No trigger identified yet. | 4–6 d | — |
| Anchors | Named prerequisite of multi-pane and of the vault — see the modular architecture design doc. | not sized | — |

Sequential total for the five sized editor items: roughly 11–16 working days.

### Scroll to reveal the cursor

The only item whose trigger has already fired. `DocumentLayout` gives the
cursor's y, so this is `ScrollHandle::set_offset` against `offset()` and
`bounds()`, driven from the two gates (`edit_buffer`, `move_cursor`). It is
testable: gpui force-draws dirty windows as the effect queue drains under
`cfg(test)`, so `last_layout` is fresh between two simulated keystrokes.

### CRLF normalization

Small in isolation. It stays deferred because nothing can put a `\r` in the
buffer today — there is no file I/O and no paste action — so writing it now
means writing a normalizer no input can reach.

### Soft wrap

The expensive one, and the cost is not in the shaping call. It breaks the
uniform row height that the current layout is built on:

- `DocumentLayout::top_for_line(i)` stops being `line_height * i` and needs the
  per-line `top` table that D5 deliberately cut as derivable.
- Visual line stops equalling logical line, which propagates to `Home`/`End`,
  up/down and the goal column, to all four selection-quad cases, and to mouse
  hit-testing.

Do **not** port Zed's `display_map`/`wrap_map` to get there. The reasoning is
in `gpui-notes.md`.

### Virtualization

Must come *after* soft wrap, not before. `uniform_list` is viable only while
every row is one `line_height` tall, so doing it first buys roughly one day of
code that soft wrap then throws away. After soft wrap it is a custom windowed
element that shapes visible lines only — the same conclusion markdown live
preview reaches independently, per the modular architecture design doc.

### Multi-cursor

`Selection` is one range by construction, so this is a change across all three
crates: a selection collection with overlap merging, every `EditorState`
operation applied per selection in descending offset order, and undo grouping
across N carets.

The genuine risk is the IME, not the editing. The macOS input protocol assumes
a single `marked_range`, and `EntityInputHandler` is written against that
assumption throughout `input.rs`. What composition should even mean with N
carets is an open question here, not a known design being typed in.

## Application

The crates that do not exist yet — `inkra-vault`, `inkra-markdown`,
`inkra-index` — and where panes, tabs and the command palette land are recorded
in the "Where future work lands" table of
`superpowers/specs/2026-09-20-modular-architecture-design.md`. Not repeated
here, so the two cannot drift.

## Ordering

Only two constraints are real:

1. Soft wrap before virtualization.
2. Scroll to reveal the cursor before anything else — it is the only visible
   defect today, and it is independent of the rest.

Everything else is free. Spending three weeks on this list before the vault,
links and search exist is a choice about what the application is, not a
sequencing requirement.
