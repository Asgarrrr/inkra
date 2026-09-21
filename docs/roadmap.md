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
| Horizontal scroll and reveal | A note with lines long enough to push the caret off the right edge. **Fired** on 2026-09-21: one `⌘V` of a URL creates that line in a single keystroke. | 1–2 d | — |
| Soft wrap | A real note measured to need it. | 3–5 d | — |
| Virtualization | A profile on a real note showing per-frame shaping or `SharedString` allocation costs anything. | 2–3 d | Soft wrap |
| Multi-cursor | No trigger identified yet. | 4–6 d | — |

Sequential total for the four sized editor items: roughly 10–16 working days.

### Horizontal scroll and reveal

The vertical half shipped on 2026-09-20; see
`superpowers/specs/2026-09-20-scroll-to-cursor-plan.md`. The horizontal half did
not, and it is not the same size.

Long lines are already clipped rather than overflowing: with `overflow.y` set to
`Scroll` and `overflow.x` left `Visible`, `Style::overflow_mask` masks to the
element's bounds on **both** axes (`gpui/style.rs:668-672`). So the caret can be
lost off the right edge today, silently.

The trigger has fired. Clipboard landed on 2026-09-21, and a paste puts an
arbitrarily long line in the buffer in one keystroke where typing one takes
sustained effort. Nothing is broken that was not broken before — the caret was
already losable off the right edge — but the way to reach it is now ordinary
use rather than deliberate effort.

Three costs the vertical half did not pay:

- `TextElement::request_layout` reports `width: relative(1.)`. Scrolling
  horizontally needs the real width of the widest line, which means measuring
  every line's advance rather than only the shaped ones.
- The `div` needs `overflow_x_scroll`, which changes wheel behaviour on both
  axes: a zero delta is remapped between them only while the other axis is not
  `Scroll` (`gpui/elements/div.rs:3348-3365`).
- The x position of a column is only known after shaping. Zed runs its
  horizontal pass after line layout for exactly that reason
  (`editor/src/element.rs:9383-9402`), which puts it in prepaint — the placement
  the vertical half rejected, because our `div` reads the offset before our
  element prepaints.

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

## Vault

`inkra-vault` ships open, list and save. Saving is atomic — a temp file renamed
over the destination — and a note writes itself on a timer, on a note switch,
and as the application quits, not only on `⌘S`. A write that finds the file
changed underneath it preserves those bytes beside the note before replacing
them, so no write destroys what it did not put there.

What it deliberately leaves out — the file watcher, diff-based reload,
create/rename/delete, serializing writes *across* notes, buffer eviction,
reporting a failed open, and the native folder picker — is recorded with a
trigger each in the Deferred table of
`superpowers/specs/2026-09-20-inkra-vault-design.md`.

The unnamed launch buffer has left that table. It is closed, by removal rather
than by saving: a launch with no note builds no editor at all, and `AppShell`
renders an empty state, so there is no untracked buffer left to lose. That empty
state names no action, because both it would want — the folder picker and
creating a note — are still deferred. Which is the strongest argument for doing
create/rename/delete next. See
`superpowers/plans/2026-09-21-empty-state.md`.

What conflict detection did **not** close is the clean note. A buffer with no
unsaved edits never reaches a write, so it never reaches the check, and a note
changed on disk keeps showing stale text until it is reopened. Fixing that needs
the watcher, and acting on it needs diff-based reload, which needs anchored
selections. See `superpowers/specs/2026-09-21-conflict-detection-design.md` for
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

## Application

The crates that do not exist yet — `inkra-vault`, `inkra-markdown`,
`inkra-index` — and where panes, tabs and the command palette land are recorded
in the "Where future work lands" table of
`superpowers/specs/2026-09-20-modular-architecture-design.md`. Not repeated
here, so the two cannot drift.

## Ordering

Only one constraint is real: soft wrap before virtualization.

Scroll to reveal the cursor used to be the second, as the only visible defect.
It shipped, so the list no longer has a forced first item.

Everything else is free. Spending three weeks on this list before the vault,
links and search exist is a choice about what the application is, not a
sequencing requirement.
