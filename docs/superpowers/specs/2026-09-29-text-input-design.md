# One text-input core for NameField and EditorView

Date: 2026-09-29. Status: **implemented** (see As built).

A refactor from the architecture review. Behaviour stays identical. The one
binding change, `ctrl-a` and `ctrl-e` in `NameField` (K2), lands the caret
where it lands today.

## Context

`NameField` (`field.rs`, 444 lines) and `EditorView` (`editor/`) each own a
`Buffer`, an `EditorState` and the platform input protocol. Checked against
the code:

- `field.rs:206-236`, `text_for_range`, `selected_text_range` and
  `marked_text_range`, are the same bytes as `editor/input.rs:9-40`.
  `bounds_for_range` and `character_index_for_point` differ in form only:
  the field computes a one-row rectangle and hit-tests row 0 by hand; the
  editor calls `DocumentLayout::range_bounds` and `offset_for_position`. On a
  one-row layout both give the same result (`layout/mod.rs:233-292`).
- `NameField::edit_buffer` (`field.rs:63-80`) says it mirrors
  `EditorView::edit_buffer` (`editor/mod.rs:158-172`). It leases with
  `update` instead of `as_mut`, and emits `Edited` when the version moved.
- `field.rs:104-169` re-declares 9 handlers from `editor/actions.rs`, with a
  comment explaining that it must.
- `keymap.rs:56-72` lists 13 editing keys a second time for `NameField`.
  They drift: `ctrl-a`/`ctrl-e` are `LineStart`/`LineEnd` in the editor and
  `Home`/`End` in the field. `Home` differs too: row edge in the editor
  (`motion.rs:135`), `LogicalMovement::LineStart` in the field.
- `FieldElement::prepaint` (`field.rs:352-400`) shapes with
  `Markdown::default()` and builds its caret and selection quads by hand.
  `TextElement::prepaint` (`editor/element.rs:170-189`) builds the same quads
  through `caret_bounds` and `selection_quads`. On one row they agree.
- The paint tails agree line for line: `handle_input`, selection, text, caret
  when focused, then store `last_layout` and `last_bounds`
  (`field.rs:403-440`, `element.rs:192-231`).

The editor has no test for `ctrl-a`, `ctrl-e`, `cmd-left` or `cmd-right`.

## What Zed does

- One `Editor` entity with `EditorMode::SingleLine | AutoHeight | Full`
  (`editor/src/editor.rs:1825`). One `EditorElement` draws every mode and
  branches on it (`element.rs:8512`). The key context is `Editor` plus
  `mode = single_line | full` (`editor.rs:2772-2786`).
- The keymap binds every editing key once, in `Editor`. Keys that only make
  sense in a document go in `Editor && mode == full`: `enter` is `Newline`
  there, and nothing in single-line mode.
- `up`/`down` are bound in `Editor` for every mode. In single-line mode
  `move_up` calls `cx.propagate()` (`navigation.rs:52-55`), and
  `Picker > Editor` rebinds them to `menu::SelectPrevious`/`SelectNext`.
- macOS keymap (`assets/keymaps/default-macos.json` at the pinned commit,
  lines 103-108): `home` and `cmd-left` are `MoveToBeginningOfLine` with
  `stop_at_soft_wraps: true`, the row edge. `ctrl-a` is the same action with
  `stop_at_soft_wraps: false`, the logical line start. `end`, `cmd-right` and
  `ctrl-e` mirror them. This matches Cocoa, where `ctrl-a` is
  `moveToBeginningOfParagraph`.

Deviations, each simpler for a note app:

- **Two views, one core, not one entity with modes.** The editor's markdown,
  scroll, column inset, reload and links mean nothing to a name field. Zed
  pays for its single entity with 17 `is_single_line()` checks in
  `editor/src`. We share what is the same, the text, the selection, the protocol and the
  keys, and let each view keep what differs.
- **Host identifiers, not a `mode` key.** Shared keys bind in `TextInput`.
  Editor-only keys bind in `EditorView`, field-only keys in `NameField`. It
  gives the same dispatch as `Editor && mode == full` with one concept less.
- **`up`/`down` stay unbound in the field.** They bubble to `Picker` or
  `Sidebar` as they do today. Zed binds and then propagates; we skip both.
- **No `stop_at_indent`.** Our `Home` has never stopped at an indent.

## K2. The `ctrl-a` decision

`ctrl-a`/`ctrl-e` are `LineStart`/`LineEnd` in both views, as in Zed and
Cocoa. `home`, `end`, `cmd-left` and `cmd-right` are the row edge in both.
In the field there is one row and no line break, so all six land where they
land today. The change is in the binding table only, and it is what makes
one table possible.

## The module: `text_input`

`crates/app/src/text_input.rs`, with `text_input/actions.rs`,
`text_input/paint.rs` and `text_input/tests.rs`.

It hides: the buffer lease and its notify, the version check that tells an
edit from a no-op, the UTF-16 ↔ byte conversions, the IME marked range, the
single-line text rule, the row-edge moves, and the caret and selection
quads. A view hands it a host and gets back a working text input.

It lives in `app` because it holds an `Entity<Buffer>`, a `FocusHandle` and
a `DocumentLayout`, and implements `gpui::InputHandler`. The pure editing
logic stays in `inkra-editor` (`EditorState`), which has no gpui.

### Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode { SingleLine, Multiline }

pub(crate) struct TextInput {
    buffer: Entity<Buffer>,
    state: EditorState,
    mode: Mode,
    focus_handle: FocusHandle,
    last_layout: Option<Rc<DocumentLayout>>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl TextInput {
    pub(crate) fn new(buffer: Entity<Buffer>, selection: Selection, mode: Mode, cx: &mut App) -> Self;
    /// Only `EditorView::open_buffer` calls this; the state and the layout go with the old buffer.
    pub(crate) fn rebind(&mut self, buffer: Entity<Buffer>);
    pub(crate) fn buffer(&self) -> &Entity<Buffer>;
    pub(crate) fn state(&self) -> &EditorState;
    pub(crate) fn cursor(&self) -> ByteOffset;
    pub(crate) fn focus_handle(&self) -> &FocusHandle;
    pub(crate) fn last_layout(&self) -> Option<&Rc<DocumentLayout>>;
    pub(crate) fn last_bounds(&self) -> Option<Bounds<Pixels>>;
    /// Called by the element's paint, the one writer of the layout.
    pub(crate) fn store(&mut self, layout: Rc<DocumentLayout>, bounds: Bounds<Pixels>);
    /// `"TextInput <host>"`: the shared bindings match every host.
    pub(crate) fn key_context(host: &'static str) -> KeyContext;
}

/// The view that owns a `TextInput`. Two implementers: `EditorView`, `NameField`.
pub(crate) trait InputHost: Render + Sized {
    fn input(&self) -> &TextInput;
    fn input_mut(&mut self) -> &mut TextInput;
    /// After every gated mutation; `edited` is false when the version did not move.
    fn after_edit(&mut self, edited: bool, window: &mut Window, cx: &mut Context<Self>);
    /// After a selection change; `old` is the selection range before it.
    fn after_move(&mut self, old: Range<ByteOffset>, window: &mut Window, cx: &mut Context<Self>);

    // Provided: the gates.
    fn edit_buffer<R>(&mut self, window: &mut Window, cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &mut Buffer) -> R) -> R;
    fn move_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>,
        f: impl FnOnce(&mut EditorState, &Buffer));
}

/// gpui's `ElementInputHandler`, generic over the host instead of over `EntityInputHandler`.
pub(crate) struct InputAdapter<V: InputHost> { view: Entity<V>, bounds: Bounds<Pixels> }
impl<V: InputHost> gpui::InputHandler for InputAdapter<V> {
    /* the 8 protocol calls, plus: */
    /// `ElementInputHandler` forwards `accepts_text_input`, true for both hosts; the trait default is false.
    fn prefers_ime_for_printable_keys(&mut self, _: &mut Window, _: &mut App) -> bool { true }
    fn element_bounds(&mut self, _: &mut Window, _: &mut App) -> Option<Bounds<Pixels>> { Some(self.bounds) }
}
```

`InputAdapter` keeps the whole surface `ElementInputHandler` gives both
views today, not only the 8 protocol calls. `prefers_ime_for_printable_keys`
answers `true`: `ElementInputHandler` forwards it to `accepts_text_input`
(`gpui/src/input.rs:297-299`), which neither view overrides, while the bare
`InputHandler` default is `false` (`gpui/src/platform.rs`). Losing it would
route printable keys past the IME, ahead of keybinding matching
(`key_dispatch.rs`). `element_bounds` answers `Some(bounds)` for the same
parity.

`InputAdapter` exists because the orphan rule forbids
`impl<V: InputHost> EntityInputHandler for V`. `Window::handle_input` takes
any `impl InputHandler` (`gpui/src/window.rs:5643`), so neither view
implements `EntityInputHandler` any more.

`actions.rs` holds the shared actions and their generic handlers:
`Backspace`, `Delete`, `Left`, `Right`, `SelectLeft`, `SelectRight`, `Home`,
`End`, `LineStart`, `LineEnd`, `SelectAll`, in namespace `text_input`, and

```rust
/// Every shared handler, on the host's root div. A host adds its own after.
pub(crate) fn on_actions<V: InputHost>(div: Div, cx: &mut Context<V>) -> Div;
```

`paint.rs` holds the element half both views share:

```rust
pub(crate) struct Quads { selection: Vec<PaintQuad>, caret: Option<PaintQuad> }
impl Quads {
    /// A caret for an empty selection, else one quad per row; `origin` is the text's.
    pub(crate) fn new(layout: &DocumentLayout, selection: &Range<ByteOffset>, origin: Point<Pixels>) -> Self;
}
/// Input handler, selection, text, caret when focused, then `TextInput::store`.
pub(crate) fn paint<V: InputHost>(view: &Entity<V>, layout: Rc<DocumentLayout>, quads: Quads,
    bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App);
```

### Invariants and ordering

- **I1.** `edit_buffer` is the only path that mutates the text. It leases
  with `as_mut`, runs `f`, drops the lease, compares the version, notifies
  the host, then calls `after_edit`. The lease is dropped before the hook,
  so a hook that reads or reshapes the buffer cannot panic on a second lease.
- **I2.** `move_cursor` reads the buffer, runs `f`, notifies, then calls
  `after_move` with the old range. It never leases, so no buffer observer
  (autosave) fires on a caret move.
- **I3.** `unmark_text` changes the state and notifies, and calls neither
  hook: it is not a text change.
- **I4.** In `Mode::SingleLine`, text entering through the protocol has
  every `\n` replaced by one space before `EditorState` sees it, so UTF-16
  ranges stay valid and the buffer never holds a line break. This is
  `flatten_newlines`, moved. `Mode::Multiline` passes text unchanged.
- **I5.** Every pixel ↔ offset call goes through `last_layout`. `Home`/`End`
  do nothing before the first paint, as the editor's do today.
- **I6.** A keystroke is bound in `TextInput` or in a host context, never in
  both. Enforced by a keymap test.

Hooks, per host:

- `EditorView::after_edit` reshapes, then reveals the caret (`Minimal`).
  `after_move` reshapes when the conceals touched by the old and new
  selection differ, then reveals. `select_range` keeps its own `Center`
  reveal through a private `move_cursor_revealing`.
- `NameField::after_edit` emits `Edited` when `edited`. `after_move` does
  nothing: the gate already notified.

## What moves where

- **F1. `field.rs`.** `NameField { input: TextInput }`. `edit_buffer`,
  `move_cursor`, `movement`, the 9 handlers, `flatten_newlines` and the
  `EntityInputHandler` impl go. `FieldElement::prepaint` keeps its
  `shape_document` call with `Markdown::default()`: an empty `Markdown` is
  the right data for text with no markup, and `Vec::new` allocates nothing.
  It builds `Quads::new` and calls `text_input::paint`. `render` uses
  `TextInput::key_context("NameField")` and `on_actions`, then `Confirm` and
  `Cancel`. `Confirmed`, `Cancelled` and `Edited` stay.
- **E1. `editor/`.** `EditorView { input: TextInput, .. }` replaces `buffer`,
  `state`, `focus_handle`, `last_layout` and `last_bounds`. `editor/input.rs`
  goes. `edit_buffer`, `move_cursor` and `update_state` become the trait's;
  `open_buffer` calls `TextInput::rebind`. `move_to_row_edge` and `RowEdge`
  move to `text_input/actions.rs` as the `Home`/`End` handlers.
  `move_vertically` stays: it is multiline only. `TextElement` keeps its
  measure, stash and inset, and its prepaint and paint tail become
  `Quads::new` and `text_input::paint`, then its own `last_text_top` write.
  Superseded by `2026-09-29-editor-geometry-design.md`: `scroll.rs` is now
  `viewport.rs`, `scroll_to` is now `Viewport`, `request_scroll` is
  `Viewport::request`, `last_text_top` is `Viewport`'s private `text_top`,
  written only by `Viewport::painted` (G1).
- **E2. `editor/actions.rs`.** Keeps `Up`, `Down`, `SelectUp`, `SelectDown`,
  `DocStart`, `DocEnd`, `Newline`, `Copy`, `Cut`, `Paste`, `Undo`, `Redo`,
  `FollowLink` and their handlers, all multiline only.
- **K1. `keymap.rs`.** The 13 shared keys bind once, in `TextInput`, with
  `ctrl-a`/`ctrl-e` on `LineStart`/`LineEnd` (K2). `EditorView` keeps
  `up`, `down`, `shift-up`, `shift-down`, `cmd-up`, `cmd-down`, `enter`,
  `cmd-enter`, `cmd-c`, `cmd-x`, `cmd-v`, `cmd-z`, `cmd-shift-z`.
  `NameField` keeps `enter` and `escape`. The comment about repeating the
  editor's bindings goes.
- **L1. `layout/mod.rs`.** `range_bounds` and `selection_quads` become
  `pub(crate)`. The two comments naming `NameField` name `TextInput`.

## Tests

The new interface is the test surface. Each line is one executable criterion.

`text_input/tests.rs`, `#[gpui::test]`, on both hosts through their test
constructors, driving `InputAdapter` as the platform does:

- The protocol tests move here from `editor/tests/protocol.rs` and
  `field/tests.rs:266-366`, bodies unchanged, calls routed through
  `InputAdapter`. Multiline-only ones (`bounds_for_range_follows_the_line_…`,
  `composition_on_a_later_line_…`) run on `EditorView`, the rest on the host
  they ran on.
- `a_single_line_input_turns_an_inserted_newline_into_a_space` and the
  marked variant (moved from `field/tests.rs:155-179`), I4.
- `a_multiline_input_keeps_an_inserted_newline`, I4's other half. Fails if
  the flatten runs in every mode.
- `the_adapter_prefers_the_ime_for_printable_keys_on_both_hosts`: through
  `InputAdapter`, `prefers_ime_for_printable_keys` is `true` for `NameField`
  and `EditorView`, as `ElementInputHandler` answers today. Fails if the
  override goes and the trait default (`false`) applies.
- `a_caret_move_fires_no_buffer_observer`: observe the buffer, press `left`,
  count zero. Fails if `move_cursor` leases (I2).
- `a_key_that_edits_nothing_reports_edited_false`: `backspace` at 0 calls
  `after_edit(false, ..)`, asserted through `NameField`'s missing `Edited`
  (existing `a_key_that_changes_nothing_emits_no_edited`, kept in place).

Added:

- `editor/tests/dispatch.rs::ctrl_a_and_ctrl_e_reach_the_line_ends_across_a_wrap`:
  on a line wrapped in three rows, caret on row 2, `ctrl-a` lands on the
  line's start, `ctrl-e` on its end.
- `editor/tests/dispatch.rs::cmd_left_and_cmd_right_stop_at_the_row_edges`:
  same fixture, `cmd-left` lands on row 2's start, `cmd-right` on its
  `caret_end`. With the previous test, fails if K2 swaps the two.
- `keymap/tests.rs::no_key_is_bound_both_shared_and_per_host` (I6): collect
  the keystrokes bound in `TextInput`; none is also bound in `EditorView` or
  `NameField`. Fails on a re-added duplicate line.

Kept, unchanged: every `field/tests.rs` keystroke test (`cmd-left`,
`cmd-right`, `ctrl-a`, `ctrl-e`, `cmd-a`, arrows, `delete`, `backspace`,
`enter`, `escape`); they now prove the `TextInput` bindings in single-line
mode. Every `editor/tests/dispatch.rs` test.

Run `./scripts/test-names.sh` before step 1 and after each step, and diff.
The only names that vanish are the moved ones, under their new path.

## Look

Render `sidebar-naming`, `sidebar-renaming`, `sidebar-rejected`,
`quick-open-results`, `vault-search-results`, `editor-caret`,
`editor-selection`, `editor-wrapped-selection` and `editor-markdown` before
step 1 and after the last step, and look at them. They must match pixel for
pixel: the field's quads now come from `selection_quads` and `caret_bounds`,
which give the same rectangles on one row.

## Risks

- **`Home` in the field now needs a layout** (I5). Before its first paint it
  does nothing, where it moved to 0 before. A field is focused only once
  drawn, so a key cannot arrive first. `cmd_left_reaches_the_start` covers
  the drawn case.
- **The field now leases with `as_mut`.** It notifies the buffer's observers
  on drop. The field's buffer has none; `edit_buffer` notifies the host
  itself.
- **Handler order.** `on_actions` registers before the host's own handlers.
  No action is registered twice, so order does not change dispatch.
- **The editor touches `input.` in ~15 files.** The change is mechanical,
  but a missed `last_layout` write would stall IME placement with every test
  green. `paint` owns the only write, and `bounds_for_range` tests on both
  hosts catch a missing one.
- **`element_bounds` has no macOS caller today.** No backend under
  `gpui_macos` or `gpui/src/platform/` calls it, so no test can see a
  regression. `InputAdapter` still answers it, so a future backend that
  pulls the focused element's geometry finds it.
- **`prefers_ime_for_printable_keys` is easy to lose.** Nothing in
  `check.sh` would notice it flip to the trait default. A test asserts it
  is `true` through `InputAdapter` on both hosts.
- **The trait's generic methods cost two monomorphised copies.** They are
  small; no measurement is needed.

## Plan

Each step builds, passes `./scripts/check.sh`, diffs `test-names.sh`, and is
one commit.

1. **Core on the field.** Add `TextInput`, `Mode`, `InputHost`,
   `InputAdapter` and `paint.rs`. Port `NameField` (F1); its handlers
   stay in `field.rs` until step 3, calling the trait gates. Move the
   field's protocol and newline tests to `text_input/tests.rs`; add the
   caret-observer test against `NameField`. Shots of the sidebar scenes match.
2. **Editor on the core.** E1 and L1. Move `editor/tests/protocol.rs` into
   `text_input/tests.rs`. Add the multiline-newline test. Every editor test
   passes unchanged except for `view.input.` paths. Shots of the editor scenes match.
3. **One binding set.** Move the shared actions and handlers (E2),
   `on_actions` in both renders, `key_context` in both, K1 with K2. Add the
   two editor dispatch tests and the keymap test. Every field keystroke test
   passes unchanged.
4. **Close.** Update `.claude/rules/editor.md`: the gates are `InputHost`'s,
   its `paths:` include `text_input/**`. Add this spec to the CLAUDE.md docs
   list. Add to `docs/roadmap.md`: clipboard and undo in `NameField` are now
   a keymap move from `EditorView` to `TextInput` plus moving their handlers,
   deferred until asked. `cargo mutants` is not required: no `text`,
   `editor`, `markdown` or `index` change.

## As built

Deviations from the plan above, each with its reason:

- **The protocol is `InputHost`'s.** The 8 calls are provided methods on
  `InputHost`, named and typed as `EntityInputHandler` names them.
  `InputAdapter` forwards to them, as `ElementInputHandler` does. A test that
  already holds the view's lease calls them directly, with `InputHost`
  imported in place of `EntityInputHandler`.
- **`editor/tests/protocol.rs` stays where it is.** Its bodies run inside
  `with_editor`, which holds the view's lease; `InputAdapter` would lease it
  again and panic. Moving it meant rewriting every body. `text_input/tests.rs`
  drives `InputAdapter` on both hosts: the IME preference and the multiline
  newline on `EditorView`, the rest on `NameField`. The file's one
  `select_all` action call became `move_cursor(.., EditorState::select_all)`,
  since the handler is now generic.
- **No `move_cursor_revealing`.** `select_range` goes through `move_cursor`,
  whose hook reveals `Minimal`, then calls `reveal_cursor(Center)`. The
  result is the same: both reveals read the same geometry, the last offset
  set wins, and without geometry a `Center` request replaces the `Minimal`
  one (`request_scroll`).
  Superseded by `2026-09-29-editor-geometry-design.md`: `scroll.rs` is now
  `viewport.rs`, `scroll_to` is now `Viewport`, `request_scroll` is
  `Viewport::request`.
- **The two proposed dispatch tests were not added.** The Context claim that
  the editor has no `ctrl-a`, `ctrl-e`, `cmd-left` or `cmd-right` test was
  wrong: `editor/tests/wrap.rs` has `cmd_left_is_home`, `cmd_right_is_end`,
  `ctrl_a_goes_to_the_start_of_the_logical_line` and
  `ctrl_e_goes_to_the_end_of_the_logical_line`, on a wrapped line. They pin
  K2 in the editor; the `field/tests.rs` keystroke tests pin it in the field.
- **`paint` is not the only layout writer.** `EditorView::reshape` stores its
  reshape through `TextInput::set_layout`. `TextInput` also gained
  `map_through` (a reload) and a `#[cfg(test)]` `state_mut` for the two tests
  that set up a state no key reaches (`scroll.rs`, `swap.rs`).
- **`DocumentLayout::position_for_offset` is `#[cfg(test)]`.** Its one
  non-test caller was the field's hand-made `bounds_for_range`.
- **Commits stay clippy-clean.** Items the next step needs were added in that
  step, not in step 1: `Mode::Multiline`, `rebind`, `set_layout`,
  `map_through` in step 2; `CONTEXT`, `key_context` and the actions in step 3.
- **`flatten_newlines` is `Mode::admit`.** It still replaces `\n` only, not
  every ASCII control char as `one_line` does: that would change what a
  name field accepts.

Verification:

- The 12 scenes of Look (plus `editor-wrapped`, `editor-concealed` and
  `vault-search-opened`) render byte-identical before step 1 and after step 3.
- `test-names.sh`: two names renamed on the move, four added, none lost.
  `a_pasted_newline_does_not_give_the_field_a_second_line` became
  `a_single_line_input_turns_an_inserted_newline_into_a_space`;
  `a_marked_newline_does_not_give_the_field_a_second_line` became
  `a_single_line_input_turns_a_marked_newline_into_a_space`. Added:
  `a_caret_move_fires_no_buffer_observer`,
  `a_multiline_input_keeps_an_inserted_newline`,
  `the_adapter_prefers_the_ime_for_printable_keys_on_both_hosts`,
  `no_key_is_bound_both_shared_and_per_host`.
- `no_key_is_bound_both_shared_and_per_host` fails when a duplicate binding
  (`ctrl-a` in `NameField`) is added back.
- `cargo mutants` does not apply: the diff touches only `inkra-app`.
