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
| Virtualization | A profile on a real note showing per-frame shaping or `SharedString` allocation costs anything. | 2–3 d | — |
| Multi-cursor | No trigger identified yet. | 4–6 d | — |

### Soft wrap and horizontal scroll

Soft wrap shipped on 2026-09-23 and closed the horizontal item without building
it: a line longer than the 70 `ch` column paints on several rows, so no line
reaches the right edge. The design, what red/blue review changed in it, and
the edges it accepts — End stopping one character short on a mid-word wrap, a
resize shifting the text under the viewport — are in
`superpowers/specs/2026-09-23-soft-wrap-design.md`, with a Deferred table of
its own.

### Virtualization

Soft wrap has shipped, so rows are no longer one `line_height` per line and
`uniform_list` is out. It is a custom windowed element that shapes visible rows
only — the same conclusion markdown live preview reaches independently, per the
modular architecture design doc. It removes two passes, not one: `edit_buffer`
now shapes the whole document too, before revealing the caret.

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

What the vault still leaves out — the file watcher, diff-based reload,
serializing writes *across* notes, buffer eviction, reporting a failed open,
and the native folder picker — is recorded with a trigger each in the Deferred
table of `superpowers/specs/2026-09-20-inkra-vault-design.md`.

The unnamed launch buffer has left that table. It is closed, by removal rather
than by saving: a launch with no note builds no editor at all, and `AppShell`
renders an empty state, so there is no untracked buffer left to lose. See
`superpowers/plans/2026-09-21-empty-state.md`. Half of what that empty state
could not name is now real — `⌘N` makes a note from anywhere in the window —
and the other half, the folder picker, is still deferred.

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

No constraint is left. Soft wrap before virtualization was the last one, and
soft wrap shipped.

Everything else is free. Spending three weeks on this list before the vault,
links and search exist is a choice about what the application is, not a
sequencing requirement.
