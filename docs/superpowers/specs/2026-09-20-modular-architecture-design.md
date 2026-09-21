# Modular architecture

Status: implemented (2026-09-20). Revised after red/blue team review, then
again after the review of the implementation — every claim below that cites a
file:line was verified against the vendored gpui source, not against memory.
Line references to `main.rs` describe the pre-restructure file, kept because
they are what the argument was made against.

## Why now

GPUI is validated (2026-09-20), accessibility gap accepted — see
`docs/gpui-accessibility.md`. This repo stops being an evaluation prototype and
becomes the skeleton of Inkra: a note application where text editing is one
component among several (vault of markdown files, links and backlinks, search,
panes).

Today all editor logic lives in one file, `crates/app/src/main.rs`, 860 lines:
view state, the `EntityInputHandler` protocol edge, editing commands, the
`TextElement` painter, the keymap, the process bootstrap, and 14 tests. Every
one of those tests needs a `Window`, because the logic they exercise is fused
to the gpui layer.

Multi-line is the next feature, deliberately scheduled *after* this
restructure, as the test of whether adding a feature is cheap.

## Crate graph

```
inkra-text   ── the text model. unicode-segmentation only.
    ↓            No gpui. No filesystem. No clock.
inkra-editor ── editing semantics: selection, logical movement, commands.
    ↓            No gpui. No filesystem. No clock.
inkra-app    ── gpui and the platform: views, elements, keymap, window,
                 bootstrap, and file I/O until a vault crate earns its place.
                 The only crate that reads a clock.
```

One direction, enforced by `scripts/check.sh`, extended to cover `editor` with
the same `-e normal,build,dev --target=all` hardening.

### Why `editor` is a crate and not a module

It is the weakest point of this design and it was challenged directly: the
logic that moves is roughly 100-130 lines, and a crate guarding 130 lines is
ceremony. `app/src/editor/` as a module would cost nothing.

The argument that decides it is reversibility, not size:

- `cargo tree -p` only exists for a crate. A module inside `app` cannot be
  stopped from importing gpui by tooling — only by review.
- module → crate later is mechanical **only if no gpui leaked in the
  meantime**. Crate → module is mechanical unconditionally.

So the crate is the option that stays cheap to reverse, and it is the one that
guarantees the precondition making the other direction cheap. That asymmetry,
not the line count, is the reason.

### `inkra-text` (today's `gpui-test-core`)

Unchanged content: `Buffer`, `ByteOffset`, `Utf16Offset`, transactions, undo,
grapheme stepping, UTF-16 conversion. Multi-line adds a logical line index
here — line count, line start offset, line containing offset. Pure `&str`
scanning, no layout input.

That line index belongs here and nowhere else: `markdown` will want it for
block boundaries and `vault` for backlink ranges. Two consumers, one scan.

### `inkra-editor` (new)

Extracted from `main.rs`. gpui-free, so its tests are plain `#[test]`.

```rust
pub struct Selection {
    pub range: Range<ByteOffset>,   // normalized: start <= end
    pub reversed: bool,
}

impl Selection {
    pub fn head(&self) -> ByteOffset;      // the moving end
    pub fn tail(&self) -> ByteOffset;      // the anchored end
    pub fn set_head(&mut self, offset: ByteOffset);   // re-normalizes
    pub fn collapse_to(&mut self, offset: ByteOffset);
    pub fn is_empty(&self) -> bool;
}
```

`head`/`tail`/`set_head` follow the shape of
`vendor/zed-reference/crates/text/src/selection.rs:17-42` (read as data, for
shape only). They absorb the four hand-rolled normalizations at `main.rs:73-75`,
`78-89`, `160-163`, `215-219`.

```rust
/// Per-view editing state.
pub struct EditorState {
    pub selection: Selection,
    pub marked_range: Option<Range<ByteOffset>>,
}

pub enum LogicalMovement { Left, Right, DocStart, DocEnd }
// LineStart/LineEnd were in the sketch and deliberately not shipped: with one
// line they are indistinguishable from DocStart/DocEnd, and a *logical* line
// start dies at soft-wrap anyway, where Home needs font metrics. Home and End
// map to the document ends until the line index exists.

impl EditorState {
    pub fn edit(&mut self, buffer: &mut Buffer, now: Instant, range: Range<ByteOffset>, text: &str);
    pub fn backspace(&mut self, buffer: &mut Buffer, now: Instant);
    pub fn delete(&mut self, buffer: &mut Buffer, now: Instant);
    pub fn move_(&mut self, buffer: &Buffer, movement: LogicalMovement, extend: bool);
    pub fn move_to(&mut self, buffer: &Buffer, offset: ByteOffset, extend: bool);
    pub fn undo(&mut self, buffer: &mut Buffer);
    pub fn redo(&mut self, buffer: &mut Buffer);
}
```

The state does not own the `Buffer`; it borrows it per operation. This is
**prepared for** multi-pane, not **justified by** it — see below, where that
claim is withdrawn. Today a `Buffer` owned outright would behave identically
for a single view. What the borrowing shape buys is that the change, when
anchors land, is not a rewrite. Verified: the
`&mut Buffer`-per-call shape compiles under this workspace's edition 2024
disjoint closure captures — `self.buffer.update(cx, |buf, _| self.state.edit(buf, …))`
borrows disjoint fields. The cost is ergonomic, not a borrow-check wall: the
private helpers (`target_range`, `range_to_utf16`, `range_from_utf16`, `slice`)
each gain a `cx: &App` parameter, and IME composition takes two entity leases
per keystroke instead of one field access.

`backspace` and `delete` share one private "range to remove" helper. Relocating
two copies of the same conditional would make this restructure buy nothing
here. Likewise `marked_range = None`, copy-pasted today at `main.rs:183`, `196`
and `298`, moves inside the `EditorState` methods that invalidate it.

Movements needing font metrics — up, down, click-to-offset, wrap-aware Home —
are **not** in this enum. See "Known seam cost" below; this is not free.

### `inkra-app`

```
src/
  main.rs            bootstrap only: run, bind keys, open window (~50 lines)
  keymap.rs          key bindings
  editor/
    mod.rs           EditorView: Entity<Buffer> + EditorState + focus + layout cache
    actions.rs       actions! declarations and handlers, delegating to inkra-editor
    input.rs         EntityInputHandler: the UTF-16 protocol edge
    element.rs       TextElement: request_layout / prepaint / paint
```

`EditorView` holds `Entity<Buffer>`, not `Buffer`. Verified sound: leases are
keyed per `EntityId` (`entity_map.rs:108-123`), so updating the buffer entity
from inside an `EntityInputHandler` method that already holds
`&mut Context<EditorView>` neither deadlocks nor panics — the ids differ.

`EditorView::new` takes an already-constructed `Buffer`, not a `&str`. Today
the caller passes `Buffer::new("…")`; tomorrow it passes one built from a file.
`EditorView` never learns where text comes from. One line, moved during a
restructure that touches it anyway.

Cost, stated rather than waved away: `prepaint` now does two entity accesses
per frame where it did one, because the buffer moved from a struct field to a
handle. Each is a `SecondaryMap` access — `slotmap`, so a dense array index,
not a hash lookup (`entity_map.rs:6`, `:130-139`) — plus an `FxHashSet` insert
for access tracking. An earlier draft called this "costs nothing" and a review
pass called it a `HashMap` lookup; both were wrong in opposite directions. It
is two O(1) array reads per frame.

### The notification obligation

`Entity<T>` is a shared handle, not a notification. `Buffer` is gpui-free by
rule, so nothing inside `Buffer::replace` can ever call `cx.notify()`. Three
independent review passes converged on this as the design's likeliest silent
failure. Therefore:

- All buffer mutation goes through **one** method on `EditorView`, the way
  `EditorView::edit` is the single path today. Not one call site per action.
- View-local state that owes a repaint but must *not* mark the buffer changed
  goes through a second gate, `update_state`. Marking the buffer dirty because
  a caret moved would lie to anything subscribed to real edits — an autosave, a
  vault reload. `unmark_text` belongs here: dropping the composing region
  removes its underline.
- The price of an unforgettable repaint is a spurious one. `GpuiBorrow` notifies
  on drop whether or not the closure changed anything, so undo on an empty
  history now schedules a frame it previously skipped. A wasted frame is worth
  more than a lost one; `EditorState::undo`/`redo` still return whether they did
  anything, for callers that need to tell.
- That method uses `Entity::as_mut`, whose `GpuiBorrow` guard calls `notify` on
  `Drop` (`app.rs:2164-2419`), rather than `update` with a hand-written
  `cx.notify()`.
- **Invariant:** no observer callback on an `Entity<Buffer>` may call
  `.update()` or `.as_mut()` on that same buffer. A second lease of the same
  entity trips `double_lease_panic` (`entity_map.rs:116`, `182-187`).

## Known seam cost — the logical/visual split

Real, and named here so it is not rediscovered later.

`LogicalMovement::LineStart` can only mean "start of the unwrapped source
line", because `inkra-editor` has no font metrics. Real editors make Home mean
"start of the *visual* line". When soft-wrap ships, Home must be computed in
`app` and the enum variant goes dead for the wrapped case.

The goal column — the remembered horizontal position that makes repeated
up/down arrows track a ragged right edge — needs metrics, so it lives in `app`,
while the selection it qualifies lives in `editor`. Every cursor-moving
`EditorState` call must reset it.

Mitigation, not elimination: `app` gets a single `move_cursor` wrapper that
resets the goal column alongside every `EditorState` movement call, the same
way one `edit` method owns notification. State split across a boundary needs a
single gate on the `app` side, or it becomes an invariant nobody owns.

## Multi-pane: wiring yes, correctness no

The earlier draft sold `Entity<Buffer>` plus per-view `EditorState` as making
two panes on one note work. **That claim is withdrawn.**

Undo and redo stacks live on `Buffer` (`crates/core/src/buffer.rs:47-54`), and
`collapse_to` reconciles the selection only of the pane that invoked the undo
(`main.rs:201-213`). Pane B holds raw `ByteOffset`s. The moment pane A edits or
undoes, B points at the wrong text — or, after `clip`, at valid and meaningless
text. This is precisely the case `docs/buffer-model.md` names as the trigger
for anchors: "multi-cursor, external reload, or background formatting".

So:

- The structure chosen here is the one multi-pane will need, and costs nothing
  extra today.
- Multi-pane is **not supported**. `inkra-text` grew anchors on 2026-09-20,
  but nothing consumes them: `Selection` still holds raw `ByteOffset`s, so
  what is left is anchored selections wired through `editor`.
- `inkra-vault` needs the anchors *and* diff-based reload: the obvious
  whole-buffer reload puts every anchor inside the replaced span, which
  collapses all of them.

Recorded so that whoever adds a second pane finds the blocker in the design
rather than in a bug report.

## Where future work lands

Not created now. Recorded so the first person to need them does not invent a
different shape.

| Concern | Crate | Depends on |
|---|---|---|
| Open, save, watch files; vault paths | `inkra-vault` | ~~`text` **+ anchors**~~ — superseded, see below |
| Parse markdown; inline styling | `inkra-markdown` | `text` |
| Links, backlinks, search | `inkra-index` | `vault`, `markdown` |
| Panes, tabs, sidebar, command palette | `app` | all |
| Accessibility text nodes | `app` | shaped lines; see the gap doc |

The `inkra-vault` row is superseded by
`2026-09-20-inkra-vault-design.md`, which built it. The crate depends on
**nothing** in the workspace, not `inkra-text`: `Fs::load` returns a `String`
and `app` builds the `Buffer`. Anchors belong to diff-based reload, which sits
above the vault rather than inside it, and is deferred there with its trigger.
The row is left in place rather than deleted because it records what was
believed when the split was designed.

Placement rule, for CLAUDE.md:

- Needs no layout, no filesystem, no clock → `text` or `editor`.
- Needs pixels, font metrics, a `Window`, a clock, or the platform → `app`.
- Filesystem and network live in `app` until a dedicated crate earns its
  place. They never go in `text` or `editor`.

### Markdown: what the seam actually covers

The earlier draft said markdown sits beside the editor, which consumes styled
runs. That is true for **inline** styling only — bold, italic, link colour —
and for those it is already free: `TextRun` is the format
(`src/text_system.rs:733`) and `shape_line(text, size, &runs, None)` already
takes it, which is what `main.rs:426-463` does by hand today. A markdown crate
produces ranges plus `HighlightStyle`; nothing new is invented.

Block-level live preview is **not** covered, and two mechanisms say so:

- `TextRun` has no widget field. A clickable link or an inline image is an
  `Element`, not glyph styling. `shape_line` takes one text style per line, so
  there is no per-run line height either.
- `uniform_list` cannot place rows of differing height. Its entire vertical
  algorithm is `item_height = longest_item_size.height`
  (`gpui/src/elements/uniform_list.rs:371`) and `item_top = item_height * ix`
  (`:497`) —
  one scalar times an index, with no per-item table. A rendered heading or a
  padded code fence puts every subsequent row at the wrong `y`.

Consequence, decided now rather than discovered later: live preview requires
abandoning `uniform_list` for a custom element, as Zed did. The multi-line plan
in `docs/gpui-notes.md` step 3 may still use `uniform_list` — constant row
height is correct for plain multi-line text — but it is a stepping stone, not
a foundation.

## Testing

Moving to plain `#[test]` in `inkra-editor`: grapheme stepping, selection
extension and normalization, cursor placement after undo, undo grouping driven
directly through `EditorState`.

Staying in `app` as `#[gpui::test]`, because they exercise the gpui edge: the
UTF-16 protocol tests, and the IME composition tests. Note that
`a_whole_composition_undoes_in_one_step`, `replacing_a_selection_is_a_single_undo_step`
and `shift_arrows_extend_and_typing_replaces_the_selection` (`main.rs:719-793`)
each call an `EntityInputHandler` method — which requires `&mut Window` by
trait signature — *and* exercise logic moving to `inkra-editor`. They cannot be
relocated unchanged. The implementation plan carries a test-by-test
disposition; "same tests, relocated" is not accurate enough to work from.

### The coverage hole this restructure would otherwise open

Only two tests drive real dispatch: `keystrokes_reach_the_buffer_through_gpui_dispatch`
(backspace, shift-left) and `a_typing_run_undoes_in_one_step` (cmd-z). Every
other test calls the handler function directly — `view.left(&Left, window, cx)`.

Once those bodies move to `EditorState`, the relocated tests call `EditorState`
directly and **bypass the app-side wrapper entirely**. A wrong `actions!`
mapping, a broken `KeyBinding`, or a dropped notification in the wrapper for
any action except backspace and cmd-z becomes untestable, and `check.sh` stays
green.

So: one `simulate_keystrokes` test per remaining action group — movement,
selection, redo — added as part of this change, not after it.

`inkra-editor` also gains a proptest over edit sequences asserting the
invariant the current code only implies: the selection always lands on a
grapheme-safe char boundary within the buffer.

## Verification

`./scripts/check.sh` green, plus:

- The isolation check extended to `inkra-editor`.
- The app run and looked at, per the `font-kit` rule in CLAUDE.md. `check.sh`
  renders nothing.
- Behaviour unchanged. This restructure adds no feature; the only new tests are
  the dispatch-coverage ones and the selection proptest, which assert existing
  behaviour.

`check.sh` green does not cover accessibility markup regressions — the probe in
`scripts/ax-text-probe.swift` is manual and stays manual. A pre-existing gap,
not introduced here, but not closed here either.

## Deliberately not doing

- No `ui` / `app` split. The binary would be 50 lines.
- No empty `vault` / `markdown` / `index` crates.
- No trait abstracting layout for the editor crate: one implementer.
- No `Document` wrapper around `Buffer`. It arrives with the vault; the change
  is a mechanical rename of `Entity<Buffer>`.
- No anchors yet. They are now a named prerequisite of multi-pane and of the
  vault, rather than an open "deferred" item.
- No `id` on `EditorState`. Raised in review for command-palette addressing;
  adding a field later is not a breaking change for a struct nobody stores in a
  map yet.
- No switch to `paint_background` for the selection. The hand-rolled quad at
  `main.rs:474-489` is equivalent while text is single-line; it becomes wrong
  only at wrap boundaries, where `WrappedLine::paint_background`
  (`text_system/line.rs:158-181`) is mandatory. Revisit with multi-line.
- No `StyledText` adoption. `compute_runs` is private (`elements/text.rs:208`)
  and the element would take away the raw `ShapedLine` that `bounds_for_range`
  and `position_for_x` need.
- No multi-line. It is the next change, and the measure of this one.
