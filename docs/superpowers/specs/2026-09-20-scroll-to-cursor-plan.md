# Scroll to reveal the cursor — plan

Status: revised after red and blue review. Changes forced by review are marked
**[red]** / **[blue]** where they replace something the draft got wrong.

The roadmap's only fired trigger: typing past the bottom edge loses the caret
today. This makes every cursor move and every edit bring the caret back into
view.

Scope is **vertical only**. Horizontal is a separate, larger change — see D6.

## What the source says

Every claim below is against `vendor/zed-reference/`, the checkout we compile.

### gpui gives us an unclamped setter and a late clamp

- `ScrollHandle::set_offset(&self, position)` (`gpui/elements/div.rs:4394-4397`)
  writes straight into a shared `Rc<RefCell<Point<Pixels>>>`. It does **not**
  clamp. The `mut position` binding is never mutated.
- Clamping happens later, in `Interactivity::clamp_scroll_position`
  (`gpui/elements/div.rs:2428-2491`): `scroll_max = padded_content_size -
  bounds.size`, floored at zero (`:2467-2470`), then
  `scroll_offset.y.clamp(-scroll_max.y, px(0.))` (`:2479`).
- The offset is **negative when scrolled down** — stated at
  `gpui/elements/div.rs:4391-4393`. `max_offset()` is the positive magnitude.

### The frame order decides where we apply the scroll

Inside one frame, for our `div`:

1. `Render::render` builds the tree.
2. `Div::prepaint` → `Interactivity::prepaint` → `clamp_scroll_position`
   (`div.rs:2390-2391`) reads the offset, clamps it against **this frame's**
   `content_size`, and writes `bounds` / `max_offset` back into the handle
   (`div.rs:2482-2485`).
3. The clamped offset is applied to the children:
   `window.with_element_offset(scroll_offset, ...)` (`div.rs:2027`).
4. `TextElement::prepaint`, then `TextElement::paint`.

So an offset written **before** the frame lands in that same frame, already
clamped against the new line count. An offset written **during**
`TextElement::prepaint` is one frame late, because step 3 has already run.

This is why we do not copy Zed's placement. Zed applies autoscroll inside
`EditorElement::prepaint` (`editor/src/element.rs:8728-8755`) and gets away with
it because its element owns the layout and re-takes its snapshot in place
(`:8747-8749`). We sit inside a `div` that reads the offset before our element
prepaints, so the same placement would cost a frame.

### What we cannot use

- `Window::request_autoscroll` (`gpui/window.rs:4089`) is consumed only by
  `List` (`gpui/elements/list.rs:1278`). `Div` never calls `take_autoscroll`.
- `ScrollHandle::scroll_to_item` (`div.rs:4316`) indexes the *children of the
  tracked div*. We have exactly one child, `TextElement`. Useless for lines.
- `Div::on_children_prepainted` (`div.rs:1810`) is skipped entirely when a
  `ScrollHandle` is tracked — the tracked branch at `div.rs:1986-1995` never
  pushes to `children_bounds`. It would hand us an empty `Vec`.
- gpui's `ScrollStrategy` with `Top`/`Center`/`Bottom`
  (`gpui/elements/uniform_list.rs:84-99`) applies to `UniformListScrollHandle`
  only. `ScrollHandle`'s own strategy enum (`div.rs:4232-4237`) is private.

### What Zed does that is worth keeping

- The default strategy for a selection change is `Fit`
  (`editor/src/editor.rs:1382-1390`): **minimal scroll**. Scroll up only if the
  target is above, down only if below, nothing if it is already visible
  (`editor/src/scroll/autoscroll.rs:252-263`).
- A margin of context lines is kept around the cursor, and it is **clamped
  against the available viewport space**, not used raw
  (`editor/src/scroll/autoscroll.rs:302-322`). Zed's own test asserts that a
  margin of 999 on a 6-line viewport degrades to 2 lines, with the comment "Let
  viewport space, rather than the configured margin, limit the padding"
  (`editor/src/editor_tests.rs:3779-3800`).
- Any manual scroll cancels a pending autoscroll request
  (`editor/src/scroll.rs:398-399`, unconditional `autoscroll_request.take()`
  before the no-op early return).

## Decisions

### D1 — apply at the two gates, eagerly, not as a deferred request

Zed stores a request and consumes it in prepaint. We apply immediately, from
`edit_buffer` and `move_cursor`.

Three reasons:

- **It lands in the same frame.** See the frame order above. A deferred request
  consumed in `TextElement::prepaint` would be a frame late.
- **The gates already exist and are mandatory.** `CLAUDE.md` says new actions go
  through `edit_buffer` or `move_cursor`. Putting the reveal inside them means
  every future action inherits it, which is the same reason the repaint
  notification lives there.
- **Zed's cancellation rule comes free.** With no pending request, a later
  mouse-wheel scroll simply overwrites our offset. Nothing to take back.

The reveal runs on mouse clicks and drags too, and that is safe by construction,
not by luck **[blue]**: `position_for_point` (`app/src/editor/mod.rs:160-163`)
resolves a point the user actually clicked, which is inside the viewport by
definition, so `reveal` returns `None`. Zed gates clicks behind a setting
(`autoscroll_on_clicks`, `editor/src/selection.rs:1195-1199`) because its
autoscroll can *centre*; ours only ever scrolls a target that is off-screen.

IME composition inherits the reveal for free **[blue]**:
`replace_text_in_range` and `replace_and_mark_text_in_range`
(`app/src/editor/input.rs:84-96`, `:98-128`) already route through
`edit_buffer`. Worth stating, because it is otherwise an undocumented accident.

`update_state` is deliberately **not** a third call site: its only caller is
`unmark_text`, which drops the composing underline without moving the caret.

### D2 — the geometry is a pure function, in its own file

`crates/app/src/editor/scroll.rs` holds one function and one constant. No
`Window`, no `Entity`, no `Buffer` — three `Pixels`-valued arguments in, one
`Option<Pixels>` out:

```rust
pub(super) fn reveal(
    line: Range<Pixels>,      // the line's top and bottom, unscrolled
    viewport: Range<Pixels>,  // the scroll container, window coordinates
    offset: Pixels,
) -> Option<Pixels>
```

It is tested with plain `#[test]`, no headless window.

This sits against `CLAUDE.md`'s "a test that does not need a `Window` does not
belong in `app`". The rule's subject is *editing semantics*, which must not leak
upwards. This is pixel geometry, and the same file's architecture rule sends
pixels and font metrics to `app` explicitly. It cannot move down: `inkra-editor`
has no `Pixels` and must not gain one. Keeping the arithmetic testable without a
window is the whole point of splitting it out; the alternative is that the only
coverage of the margin clamp is an end-to-end keystroke test.

### D3 — minimal scroll with a margin clamped by the viewport

Following Zed's `Fit`. With `m` the margin in pixels:

```
lo = viewport.start + m - line.start
hi = viewport.end   - m - line.end
```

- `offset < lo` → the line is above the viewport → new offset `lo`.
- `offset > hi` → below → new offset `hi`.
- otherwise → `None`, nothing moves.

```rust
const MARGIN_LINES: f32 = 3.;
let m = (line.len() * MARGIN_LINES).min(((viewport.len() - line.len()) / 2.).max(px(0.)));
```

**The draft claimed `lo <= hi` always holds because the margin clamp guarantees
it. That is false, and it was the worst bug in the plan [red].** The clamp can
only drive `m` down to zero; it cannot make a line fit in a viewport shorter
than one line. When `line.len() > viewport.len()`, `m = 0` and `lo` exceeds `hi`
by exactly `line.len() - viewport.len()`.

An inverted `[hi, lo]` gives the three-way test **no fixed point**. Red team ran
it: viewport `[0, 10]`, line height 28, line at 1000 → `lo = -1000`, `hi =
-1018`. Feed each result back in as `offset`, which is exactly what happens in
production once `reveal_line` writes to the handle, and it alternates
`-1018 → -1000 → -1018` forever. Every keystroke would jitter the document by
18px, indefinitely. D5's `viewport.len() <= 0` guard does not catch it: 10 is
not zero.

So the degenerate case gets its own branch, and picks one deterministic target:

```rust
let target = if line.len() > viewport.len() {
    // No fixed point exists here, so always align the line's top. Alternating
    // between `lo` and `hi` would jitter on every keystroke, and `hi` would put
    // the caret quad — painted at the line's top — above the viewport, hiding
    // the very thing being revealed.
    lo
} else if offset < lo {
    lo
} else if offset > hi {
    hi
} else {
    return None;
};
Some(target.min(px(0.)))
```

Picking `lo` rather than `hi` matters for a second reason red team found: the
caret quad is painted at the line's *top* (`app/src/editor/element.rs:294-297`,
via `position_for_offset`), so bottom-aligning a too-tall line pushes the caret
off the top edge while the function believes it revealed it.

We clamp in **pixels**, not in whole lines as Zed does
(`editor/src/scroll/autoscroll.rs:310-321`). Zed floors to whole rows because its
scroll position is stored in row units (`editor/src/scroll.rs:32-38`); ours is
raw pixels, so the floor buys nothing and the rounding-remainder rule Zed needs
does not arise.

The result is clamped to `min(px(0.))` here. The lower bound is left to
`clamp_scroll_position`, on purpose: `max_offset()` is stale between an edit that
changed the line count and the next prepaint, so clamping against it here would
stop one line short after pressing Enter at the bottom of the document.

The lower bound stays with gpui, but note the consequence **[blue]**: our
content has no padding past the last line, so the final lines of a document
never get a full margin below them. gpui's clamp simply gives whatever room
exists. Zed has `scroll_beyond_last_line` for this; we do not, and do not need
it yet.

### D4 — `last_bounds` is scroll-shifted, so store the unscrolled top

`TextElement`'s `bounds` arrives already translated by step 3, and `mod.rs`
documents that. So `last_bounds.origin.y` moves whenever the offset moves, and a
reveal computed from it would double-count.

Deriving it live as `last_bounds.origin.y - scroll_handle.offset().y` is wrong in
one real case: two cursor moves between two paints. The second would subtract an
offset the stored bounds never saw.

So `paint` stores the unscrolled value alongside the rest, in a field named
`last_text_top` to match its two neighbours' "as of last paint" prefix **[blue]**:

```rust
editor.last_text_top = Some(bounds.origin.y - editor.scroll_handle.offset().y);
```

Read during paint, the offset *is* the one applied to `bounds` this frame. The
stored value is the y the text would have at offset zero — stable across scrolls
and across edits, because adding lines below the element's origin does not move
it.

Red team confirmed this line compiles inside the existing
`self.editor.update(cx, |editor, _| ...)` closure and is **not** a second entity
lease: `ScrollHandle` is an `Rc<RefCell<ScrollHandleState>>`
(`gpui/elements/div.rs:4243`), not an `Entity`, so it never touches the entity
map and `gpui/app/entity_map.rs:231` does not apply.

The argument above belongs in the field's doc comment, not only in this file —
a spec nobody opens cannot stop the next reader from "simplifying" the field
away.

### D5 — guards

- `text_top` is `None` before the first paint → skip.
- `viewport.len() <= 0` → skip. `ScrollHandle::bounds()` is `Bounds::default()`
  until the first `clamp_scroll_position` runs (`div.rs:2484`), and a zero
  viewport at the origin would otherwise yank the offset.

Both are "nothing has been painted yet", where there is no viewport to reveal
into.

### D6 — horizontal is out of scope, and it is not free

Long lines are clipped today: with `overflow.y == Scroll` and `overflow.x ==
Visible`, `Style::overflow_mask` takes the `(true, false)` arm
(`gpui/style.rs:668-672`) and masks to the element bounds on **both** axes. So
the caret can already be lost off the right edge.

It is a bigger change than the vertical half:

- `TextElement::request_layout` reports `width: relative(1.)`
  (`app/src/editor/element.rs:269`). Horizontal scrolling needs the real
  widest-line width, which means measuring every line's advance.
- The `div` needs `overflow_x_scroll` too, which changes wheel behaviour on both
  axes (`div.rs:3348-3365` remaps a zero delta between axes only while the other
  axis is not `Scroll`).
- Zed runs the horizontal pass *after* line layout, because it needs
  `x_for_index` (`editor/src/element.rs:9383-9402`). For us that is after
  `DocumentLayout` is built — i.e. in prepaint, which D1 rejected for the
  vertical case.

Recorded in the roadmap with its trigger instead.

## The change

### `crates/app/src/editor/scroll.rs` (new)

`MARGIN_LINES`, `reveal`, and its unit tests.

### `crates/app/src/editor/mod.rs`

- `mod scroll;`
- new field `last_text_top: Option<Pixels>`, carrying D4's argument as its doc
  comment.
- new private method `reveal_line(&mut self, line: usize)`, which bails unless
  `last_text_top` and `last_layout` are both set and `bounds().size.height > 0`,
  then calls `scroll::reveal` and `scroll_handle.set_offset`.

  It reads `line_height` from `last_layout` (a new accessor), **not** from
  `window.line_height()`: outside render the text-style stack is empty, so
  `Window::line_height` (`gpui/window.rs:3009`) would return the default
  rem-derived height, not our `px(28.)`.

Both gate bodies below were compiled by red team against the real crate, not
sketched. `edit_buffer` is the plan's original and checks out:

```rust
fn edit_buffer<R>(&mut self, cx: &mut Context<Self>, f: ...) -> R {
    let mut buffer = self.buffer.as_mut(cx);
    let result = f(&mut self.state, &mut buffer);
    let line = buffer.line_for_offset(self.state.cursor());
    drop(buffer);
    self.reveal_line(line);
    result
}
```

`GpuiBorrow::drop` (`gpui/app.rs:3280-3286`) notifies before `reveal_line` runs,
as D1 assumes.

`move_cursor` is the one call site the draft left as prose, and **the natural
completion does not compile [red]**. Appending the reveal after `cx.notify()`
gives a real `E0502`: `Entity::read` ties the returned `&Buffer`'s lifetime to
`cx` (`gpui/app/entity_map.rs`), so using `buffer` after `cx.notify()` holds an
immutable borrow of `cx` across a mutable one. The line must be computed first:

```rust
fn move_cursor(&mut self, cx: &mut Context<Self>, f: ...) {
    let buffer = self.buffer.read(cx);
    f(&mut self.state, buffer);
    let line = buffer.line_for_offset(self.state.cursor());
    cx.notify();
    self.reveal_line(line);
}
```

The line index comes from `Buffer::line_for_offset`, which is exact and fresh —
**not** from `DocumentLayout::line_for_offset`, which clamps against the previous
frame's line count and would be one line short right after Enter.

### `crates/app/src/editor/element.rs`

- `DocumentLayout::line_height()` accessor.
- `paint` stores `text_top`.

### Tests

Review rewrote this section substantially. Two of the draft's twelve were
tautological — they passed with the whole feature deleted, because gpui's own
`clamp_scroll_position` forces the offset into `[-max_offset, 0]` on every
paint regardless of what we do.

**Facts the tests rest on, verified by red team:**

- The draw happens in `App::finish_update` → `flush_effects`, which draws every
  dirty window under `GpuiMode::Test` (`gpui/app.rs:1818-1829`). It runs at the
  end of *every* outer `App::update`. `run_until_parked` only drains background
  tasks — it is not what produces the frame.
- `simulate_keystrokes` dispatches each keystroke in its own `update_window`
  (`gpui/app/test_context.rs:582-591`, `:607-611`), so **every keystroke gets
  its own frame**.
- The test window is 1536x1095 (`gpui/window.rs:81`). With `p_4()` and
  `line_height` 28, 37 lines are fully visible. A 200-line document overflows it
  about five times over.
- A wheel scroll *is* simulable: `ScrollWheelEvent` implements `InputEvent`
  (`gpui/interactive.rs:522`) and `VisualTestContext::simulate_event` names it
  in its doc (`gpui/app/test_context.rs:1017`). The div registers its own wheel
  handler (`gpui/elements/div.rs`, `paint_scroll_listener`), so no app wiring is
  needed — but `position` must land in the editor's hitbox.
- `ScrollHandle` tracks exactly one child, `TextElement`, so `bounds_for_item`
  cannot answer "is line N visible". Tests compute the line's box by hand, the
  way `middle_of_line` (`app/src/editor/tests.rs:541`) already does.

**Pure, in `scroll.rs`** (`#[test]`, no window):

1. A visible line returns `None`.
2. A line above scrolls so its top sits `m` below the viewport top.
3. A line below scrolls so its bottom sits `m` above the viewport bottom.
4. A viewport too short for `MARGIN_LINES` still reveals, margin clamped — the
   analogue of Zed's `test_autoscroll_high_margin_keeps_cursor_centered`
   (`editor/src/editor_tests.rs:3778-3796`).
5. **Idempotence on a viewport shorter than one line [red].** Call `reveal`,
   feed its result back as `offset`, and assert the second call returns `None`.
   The draft's version checked one call and would have shipped the oscillation.
6. A viewport exactly one line tall: `lo == hi`, one valid offset, no inversion.
7. **A proptest [blue]**, replacing the draft's two hand-picked invariant
   checks: over arbitrary line, viewport and offset, a returned offset is never
   positive, and feeding it back returns `None`. That second clause is the
   general form of the oscillation bug. `proptest` is already a dev-dependency
   of `inkra-text` and `inkra-editor`; this adds it to `inkra-app`.

**Headless, in `editor/tests.rs`** (`#[gpui::test]`):

8. `cmd-down` on a 200-line document scrolls, and the last line sits inside
   `scroll_handle.bounds()`.
9. `cmd-up` from there returns the offset to `px(0.)`.
10. Enter at the bottom of a full viewport scrolls, and the new last line is
    visible — the defect the roadmap named, and the test that fails if the
    reveal reads the stale `DocumentLayout` line count instead of the buffer's.
11. **Two moves inside one `window.update` [red+blue].** This is the only test
    that falsifies D4's rejected formula. Both `down`s must scroll, and the
    total must be two lines, not three: with one `update` there is one frame at
    the end, so the second reveal sees a `last_bounds` that predates the first
    one's `set_offset`. Deriving the unscrolled top live would double-count.
12. Wheel-scroll away with a real `ScrollWheelEvent`, then type: the caret comes
    back. Also exercises hit-testing, which test 11 does not.
13. **The margin is actually applied end to end.** Tests 8 and 10 only assert
    "on screen"; nothing yet proves `MARGIN_LINES` survives the gate rather than
    landing the caret flush against the edge.
14. **A selection extension reveals the head, not the anchor.** `shift-down`
    past the viewport bottom. Should work by construction, since `move_vertically`
    ends in `move_cursor` — but nothing proves it.
15. **A click outside the current viewport reveals.** After scrolling, the click
    path resolves against scrolled content while `event.position` is unscrolled
    window space; that is where a coordinate-space bug would show.
16. **A shrinking document.** Undo or backspace that removes lines while scrolled
    deep. Tests 10 and 11 only cover growth.
17. **`offset().x` stays `px(0.)`** even with the caret far right on a long line
    — a regression guard on D6 staying out of scope.

**Dropped as tautological [red]:** "a document that fits never scrolls" — for a
fitting document `max_offset` is zero, so gpui's clamp pins the offset to zero
whatever we do. It is already covered by the existing
`a_document_that_fits_has_no_room_to_scroll`, and by pure test 1. The draft's
"between two already-visible moves the offset does not change" is folded into
test 13, which pre-scrolls to a non-trivial offset so a `Center`-style bug has
somewhere different to land; at offset 0 both strategies agree and the test
proves nothing.

### Docs

- `docs/roadmap.md`: drop the "Scroll to reveal the cursor" row, add a
  "Horizontal scroll and reveal" row with the D6 trigger and cost, and fix the
  "Ordering" section, which currently names this item as the thing to do first.
- `CLAUDE.md`: add `EditorView::reveal_line` and `scroll::reveal` to the
  don't-recreate table. Also clarify the Testing rule **[blue]**: what does not
  belong in `app` is an *editing-semantics* test, not any window-free test.
  D2 currently argues around that rule in a spec file nobody will reopen.
- `docs/gpui-notes.md` **[blue]**: a new section. The frame order that makes D1
  work, `set_offset` being unclamped, and the four scroll primitives that look
  applicable and are not, are exactly the hard-won facts that file exists to
  hold — and the next scroll feature (D6) will need all of them again.

## As built

Three divergences from the test list above, all found by code review after the
first commit. Recorded here rather than left to be rediscovered.

- **The degenerate-case test is `top >= bottom`, not `line > viewport`.** The
  plan's version handles a line taller than the viewport and misses a second
  case the proptest found: when the margin clamp binds exactly, the two edges
  are one number in real arithmetic and can land an ulp apart in f32. Both want
  the same answer, and `top >= bottom` covers both.
- **`reveal` returns `None` when the target equals the current offset.** Not in
  the plan. Without it a line pinned at the document start reports a move to
  where it already is, on every keystroke.
- **Two headless tests were rewritten because they did not test their own
  names.** "A shrinking document" deleted at the very end, where gpui's clamp
  pins the caret to the bottom whatever we do; it now deletes everything and
  undoes, which moves the caret both ways. "Horizontal stays untouched" used a
  wide but short document, which never scrolls at all, so it held against an
  implementation that never wrote an offset; the document is now wide *and*
  tall, so the write happens and preserving x is what is proved.

Each of the eleven headless tests was checked by deleting `reveal_line` and
confirming it fails — the only way to tell a test of this feature from a test of
gpui's clamp.

## Verification

`./scripts/check.sh`, plus a manual run: `check.sh` renders nothing, and this
change is visible by definition. Hold a key down at the bottom of a long
document and watch the caret stay on screen.

## Risks

- **`ScrollHandle::bounds()` is the padded box.** The `div` has `p_4()`. The
  viewport we reveal into therefore includes 16px of padding, so a cursor may sit
  under it at the extremes. `text_top` already carries the padding on the line
  side, so the two are consistent; the effect is at most a 16px encroachment at
  the very top and bottom of the scroll range. Accepted; revisit if it shows.
- **Two moves between two paints.** D4 is the fix, and test 11 is what proves
  it, by putting both moves in one `window.update`.
- **A window resize does not re-trigger a reveal.** Nothing calls the gates on
  resize, so a resize that hides the caret leaves it hidden until the next move.
  This matches Zed, whose own resize test has to request an autoscroll
  separately (`editor/src/editor_tests.rs:3792-3798`). Intentional, recorded so
  it is not filed as a bug.
- **Naming.** Blue review kept `reveal` / `reveal_line` — the repo already
  reuses one name across layers for the same concept over different data
  (`DocumentLayout::line_for_offset` against `Buffer::line_for_offset`). It
  rejected wrapping the `Range<Pixels>` arguments in a named viewport type:
  bare ranges are the vocabulary of every neighbour in `element.rs`, and a
  one-implementer type is the abstraction the Code rules forbid.
