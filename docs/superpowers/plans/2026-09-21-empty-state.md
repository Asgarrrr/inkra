# Plan — the empty state, and the editor that does not exist yet

**Status: IMPLEMENTED. Red team returned REVISE with two defects, both accepted;
blue team returned one design simplification that removes a third of the diff.
Both are arbitrated inline and marked [arbitrated]. Code review then returned
FIX FIRST with three findings, all accepted — see "What implementing it changed".**

## What implementing it changed

- **Test 2 was decoration and is deleted.** The plan proposed
  `launching_on_a_vault_opens_no_editor` and called it "HAS TEETH, structurally".
  It was not: the `open_vault` helper passes `AppShell::new(None, …)` itself, so
  the assertion only reads back a `None` the test supplied, and nothing between
  setup and assertion could flip it. Test 5 already guards the real regression,
  because it is the only test that calls `bootstrap::root`. Its rationale moved
  into test 5's doc comment rather than being lost.
- **The empty state paints `theme::editor_background` itself.** Not in the plan,
  and only the shot found it: `EditorView` was what painted that surface, so the
  first render left the slot pure black beside a sidebar that was not. Exactly
  the class of defect the eyeball pass exists for — `check.sh` was green through
  it.
- **Two stale claims in the vault spec, and one in the shots spec.** The plan
  listed the launch-state table and the Deferred row, and missed that the same
  spec asserts twice more that `AppShell::focus_handle` "delegates
  unconditionally to the editor", and that the shots spec still tabled
  `editor-empty`. All corrected in the same change.
- **All four behavioural tests were verified by breaking the arm**, per the
  plan's own instruction, rather than by reasoning: removing the focus move,
  removing `track_focus`, making `root` build an editor unconditionally, and
  dropping the `None` arm's assignment each produce exactly one failure.

Branch `empty-state`, base commit `7117455` (`./scripts/check.sh` green — exit 0,
run before the first edit).

Continues `docs/superpowers/plans/2026-09-21-autosave-quit-flush.md`, whose step 3
this is. That plan shipped steps 0–2 and 4–5 and split this one out, because its
review found two defects it had not solved: **F1**, that nothing could move focus
to an editor created after the window opened, and **F2**, that the test helper's
contract inverts. Closing both is this plan's substance; the empty state itself is
about fifteen lines of `div`.

**Goal:** when no note is open, render an empty state instead of an editable
buffer that silently loses what is typed into it.

---

## Why this is a data-loss fix and not a cosmetic one

`bootstrap::root` (`crates/app/src/bootstrap.rs:53-69`) builds a `Buffer` and an
`EditorView` for all three launch states. For `Launch::Note` it then calls
`NoteStore::adopt`, which tracks the note. For `Launch::Nothing` and
`Launch::Vault` it does not, and nothing else does either. That buffer therefore
has:

- no observer, so no autosave is ever scheduled for it;
- no entry in `NoteStore.open`, so `flush_all` cannot see it at quit;
- no sidebar row;
- and `NoteStore::current == None`, so `save_current` returns at its `None` guard
  and `⌘S` is a silent no-op.

It is an editable surface, focused at launch, from which every escape route is
closed. Autosave made it worse rather than better: every *other* note now writes
itself unprompted, so the one buffer that does not is the one the user was never
taught to press `⌘S` in.

Decided with the user, recorded in the autosave handoff: render an empty state, so
there is no buffer to lose. Not "make the unnamed buffer saveable" — that needs a
name, which needs the create/rename/delete slice, deferred with its own trigger.

---

## Verified constraints

Every claim was read in `vendor/zed-reference`, the checkout we compile, at pinned
commit `526c95d`. Line numbers are from that checkout. V1–V10 were independently
re-checked by the red team; the two corrections it returned are folded in.

| # | Claim | Source |
|---|---|---|
| V1 | `Context<T>::subscribe_in` takes `&Window` and hands the callback a `&mut Window`. It calls `ensure_window`. **Not used by this plan** — see V15. | `gpui/src/app/context.rs:341-364` |
| V2 | `Context<T>::spawn_in(&self, window, f)` hands the future a `WeakEntity<T>` and an `&mut AsyncWindowContext`. **Not used** — see V16. | `gpui/src/app/context.rs:672-680` |
| V3 | `Context<T>::spawn(&self, f)` hands the future a `WeakEntity<T>` and an `&mut AsyncApp`. This is what `open_note` already uses today, discarding the first argument. | `gpui/src/app/context.rs:230-238` |
| V4 | `WeakEntity::update_in(cx, |entity, window, cx|)` exists for any `C: AppContext`, resolving the window through `cx.with_window`. | `gpui/src/app/entity_map.rs:815-827` |
| V5 | `Window::focus` is a no-op when the handle is already focused, and otherwise calls `self.refresh()`. | `gpui/src/window.rs:2302-2312` |
| V6 | **`Window::refresh` does nothing while a draw is in progress**: its whole body is guarded by `if self.invalidator.not_drawing()`. | `gpui/src/window.rs:2272-2277`, `:268-270` |
| V7 | gpui compensates for V6 **only for focus listeners**, not for focus moved during render. `focus_before_listeners` is captured at `:3359` — after prepaint and paint — so the compensating `if self.focus != focus_before_listeners { self.refresh(); }` at `:3403` cannot see a focus set during `Render::render`. gpui's own comment states the suppression. | `gpui/src/window.rs:3352-3404` |
| V8 | Zed's `Dock` forwards focus to its active panel with `cx.on_focus(&focus_handle, window, …)`. Registration takes a `&Window`. | `workspace/src/dock.rs:432-441` |
| V9 | `EditorView` routes text input through `.track_focus(&self.focus_handle)` on its root element. An unfocused editor receives no keystrokes. | `crates/app/src/editor/mod.rs:259` |
| V10 | Both `bootstrap::root` call sites receive a `&mut Window` and discard it. Recorded because the first draft of this plan proposed to use them; V15 makes that unnecessary. | `crates/app/src/bootstrap.rs:121`, `crates/app/src/shot/stage.rs:82` |
| V11 | Zed's empty pane is an **inline branch** in `Pane::render`, not a separate view type, and the placeholder carries no `track_focus` of its own — the enclosing pane div already tracks the pane's handle. | `workspace/src/pane.rs:4610-4649`, `:4432-4434` |
| V12 | Zed **never** calls `window.focus(…)` as a direct statement in a `Render::render` body. Every occurrence is inside a deferred listener closure. Searched ~18 files; no counter-example. | `agent_ui/../manage_profiles_modal.rs:993`, `edit_prediction/src/onboarding_modal.rs:141`, `debugger_ui/src/new_process_modal.rs:585` |
| V13 | Zed opens an item by spawn → `await` → `pane.update_in(cx, …)`, constructing the item entity inside that closure and focusing it there, rather than stashing a `Window` across the await. | `workspace/src/workspace.rs:5074-5107`, `pane.rs:1159` |
| V14 | `Pane::focus_active_item` calls `window.focus(…)` from `activate_item` on **every** activation, not only the first. | `workspace/src/pane.rs:2565-2570`, `:1496-1498` |
| V15 | **Every entity created by `cx.new` inside a window update is registered against that window.** `App::new` publishes `Effect::EntityCreated { window: cx.window_update_stack.last() }`; `apply_entity_created_effect` inserts it into `current_window_by_entity`, with the comment "so `with_window` resolves correctly before the entity has ever been rendered". `open_window` pushes that stack around `build_root_view` — which is exactly where `bootstrap::root` builds `AppShell`. | `gpui/src/app.rs:2975`, `:1934-1946`, `:1356-1359`, `:1968-1976` |
| V16 | **`spawn_in` buys nothing over `spawn` for resolving that window.** `AsyncWindowContext::with_window` delegates to `self.app.with_window(entity_id, f)` — the same `current_window_by_entity` lookup `AsyncApp` uses. The two resolve identically. | `gpui/src/app/async_context.rs:80-86`, `:100-112` |
| V17 | The seeding in V15 is identical under test: `TestAppContext::add_window` and `VisualTestContext::open_offscreen_window` both call `cx.open_window`. Production, `#[gpui::test]` and the shot runner behave the same. | `gpui/src/app/test_context.rs:284`, `gpui/src/app/visual_test_context.rs:111` |
| V18 | `Window::is_action_available(&self, action, cx) -> bool` resolves the focused node in the **rendered** frame and asks the dispatch tree whether the action is reachable from it. It therefore needs the window to have drawn at least once. | `gpui/src/window.rs:3187-3193` |
| V19 | When the focused `FocusId` resolves to no tracked node, dispatch falls back to `root_node_id()` — `DispatchNodeId(0)` — which is the anonymous `ViewElement` boundary *above* the div `AppShell::render` returns, not the div itself. An action listener on that div is therefore unreachable from node 0. | `gpui/src/window.rs:6244-6251`, `key_dispatch.rs:611-614`, `view.rs:462-476` |

---

## F1 — how focus reaches an editor created after the window opened

Four mechanisms were considered.

**Rejected — focus from inside `Render::render`.** `AppShell::render` already holds
a `&mut Window`, so this costs four lines and no signature changes. It is wrong:
V6 says `refresh` is suppressed for the whole draw, and V7 says the compensating
refresh is captured after paint and so cannot observe a focus moved during render.
The focus would be set with no frame scheduled to reflect it. V12 is the
independent corroboration: across ~18 Zed files where the pattern nearly occurs,
every `focus` call sits inside a deferred listener closure rather than a render
body.

**Rejected — `cx.on_focus` forwarding, Zed's dock shape (V8).** The right shape the
day the sidebar becomes focusable, and it does not solve *this* problem: `on_focus`
fires when focus moves **to** a handle, and when the first editor is created focus
is already on `AppShell`'s handle and does not move. Blue team checked every
neighbour — `on_focus_in`, `on_focus_out`, `on_focus_lost`, `on_next_frame`,
`defer_in` (`app/context.rs:288-301`, `:542-660`) — and all of them take a `Window`
at registration, so none avoids the plumbing either.

**Rejected — thread a `&mut Window` through `bootstrap::root` and `AppShell::new`.**
This was the first draft's answer and what the parent plan's F1 called "a real
design change". **[arbitrated]** Blue team showed it is unnecessary, and V15/V16
are why: `AppShell` is the window's own root view, built by `cx.new` inside
`open_window`'s `build_root_view`, so its entity is registered against the window
at construction — for free, before it has ever rendered. Red team reached the same
conclusion independently while auditing V4, calling the first draft's causality "a
minor overclaim": `subscribe_in`'s `ensure_window` was never what made `update_in`
resolve. V16 closes it: `spawn_in` would resolve the window through the *same*
lookup, so it buys nothing here. Changing the signature of `bootstrap::root` — the
seam the architecture rule makes shared between two entry points — to obtain
something already in hand is churn.

**Adopted — plain `cx.spawn`, then `update_in`, then focus.** The existing
`open_note` already spawns and already discards a `WeakEntity<AppShell>` (V3). It
starts using it.

The cost of this choice is discoverability: nothing at the call site says "this
works because the entity was created inside a window update". **A doc comment on
`open_note` citing `app.rs:1934-1946` is part of this plan, not optional** — without
it a future reader will "fix" the absent window by threading one through. Blue team
is right that the risk of the mechanism itself being wrong is low, because test 1
fails loudly and immediately if it is.

---

## F2 — the test helper's contract inverts

`shell.rs::open_vault` (`:159-182`) hands callers an `Entity<EditorView>` as a
*precondition*. With the editor optional there is none until the first activation,
so it becomes a *postcondition*.

The accessor is one function, not a visibility change: `crate::shell::tests` is a
**descendant** of `crate::shell`, so it can read the private `AppShell.editor`
field directly — which is already how `open_vault` reaches `shell.sidebar` today.

```rust
/// The editor, once a note has been activated. Panics before that, which is the
/// contract: under an optional editor, "there is an editor" is something a test
/// establishes rather than something it is handed.
fn editor(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Entity<EditorView>
```

**[arbitrated] Red team defect 1, accepted.** The first draft said "`open_vault`
drops its fourth return; four call sites move to the accessor". That is wrong.
`open_vault` has **two** direct callers: `clicking_a_row_hands_its_buffer_to_the_editor`
(`:259`) and `two_notes` (`:210`). The other three tests reach the editor through
`two_notes` (`:198-214`), an intermediate helper the first draft never mentioned —
and which discards the `WindowHandle` as `_window`, so its callers have nothing to
pass to the accessor.

`two_notes` therefore changes shape too: it returns
`(WindowHandle<AppShell>, Entity<Sidebar>, Entity<NoteStore>, EntryId, EntryId)`,
drops its own `editor` binding, and its three callers call `editor(&window, cx)` at
the point they first assert on the editor. Red team verified line by line that all
three — including `the_current_note_is_the_one_in_the_editor`, which the first draft
flagged as doubtful because it activates with both loads gated — do activate and
park before touching the editor. The doubt is retired.

`a_keystroke_through_app_shell_reaches_the_editor` (`:228`) builds its editor up
front and passes it in. It keeps doing so: `AppShell::new` takes
`Option<Entity<EditorView>>`, and `Some(editor)` is a real launch state
(`Launch::Note`), not a test affordance. `vault.rs::open_shell` (`:696`) is the same
case — it `adopt`s a detached note — and changes only by wrapping in `Some`. Red
team confirmed none of `open_shell`'s callers touch the editor.

---

## Design

### `AppShell`

```rust
pub(crate) struct AppShell {
    editor: Option<Entity<EditorView>>,
    notes: Entity<NoteStore>,
    sidebar: Entity<Sidebar>,
    /// Focused while no editor exists, and the root of the dispatch tree either
    /// way. Without `track_focus` on the root element this handle is not in the
    /// dispatch tree, and dispatch falls back to the anonymous view-boundary
    /// node *above* the div carrying `.on_action` (V19) — so `cmd-s`, bound at
    /// the `AppShell` context, would never reach its handler with no note open.
    focus_handle: FocusHandle,
    activation: Option<Task<()>>,
}
```

`Focusable::focus_handle` returns the editor's when there is one and its own
otherwise. That keeps `bootstrap::run`'s existing focus call (`bootstrap.rs:126-129`)
correct for `Launch::Note` with no change, and makes it land somewhere real for the
other two.

`Render` gains `.track_focus(&self.focus_handle)` on the root div and branches the
right-hand slot between the editor and the empty state. Red team independently
confirmed, through the V19 trace, that this `track_focus` is load-bearing rather
than decorative.

### The open path

Unchanged in shape from today: same `cx.subscribe`, same `cx.spawn`. What changes
is that the closure's first argument stops being discarded.

```rust
fn open_note(&mut self, id: EntryId, cx: &mut Context<Self>) {
    let notes = self.notes.clone();
    let opened = self.notes.update(cx, |notes, cx| notes.open_note(id, cx));
    self.activation = Some(cx.spawn(async move |shell, cx| {
        let Some(buffer) = opened.await else { return };
        let _ = shell.update_in(cx, |shell, window, cx| {
            match &shell.editor {
                Some(editor) => editor.update(cx, |editor, cx| editor.open_buffer(buffer, cx)),
                None => shell.editor = Some(cx.new(|cx| EditorView::new(buffer, cx))),
            }
            let focus = shell.focus_handle(cx);
            window.focus(&focus, cx);
            notes.update(cx, |notes, cx| notes.set_current(id, cx));
        });
    }));
}
```

The `None` arm creates rather than rebinds, and must not call `open_buffer`: there
is no previous note whose undo stack could be polluted, and `EditorView::new`
already does the subscribe-and-clear-layout work `open_buffer` exists to repeat.

**[arbitrated] Blue team improvement 2, accepted.** Focus goes through
`shell.focus_handle(cx)` — the `Focusable` impl — rather than reaching into
`editor.focus_handle(cx)`. Two hand-rolled answers to "what should be focused" would
have to stay in agreement with nothing pinning them; one answer cannot disagree with
itself. It also means test 1 exercises the `Focusable` branch transitively, which
is otherwise reachable only from `bootstrap::run` and therefore untestable at this
layer. The assignment to `shell.editor` must precede the call, which the ordering
above gives.

Focus is moved unconditionally on every activation, not only the first — V14: Zed's
`activate_item` does the same. Today nothing else in our window can hold focus, so
this is unobservable until the sidebar becomes focusable, at which point it is still
what a user expects.

Red team traced the whole closure for double-leases against
`entity_map.rs:231` and found none: `shell.update_in`, the nested `editor.update`,
the nested `notes.update` and the `cx.new` all act on distinct entities. It also
confirmed there is no `.await` inside the closure, so the bind-focus-`set_current`
sequence runs atomically in one poll — the invariant CLAUDE.md states, now covering
the focus too, and a dropped `Task` can discard the whole sequence but never half of
it.

### The empty state

A private `fn render_empty(&self, cx: &App) -> impl IntoElement` on `AppShell`, not
a view entity and not a new module. It is static text with no state, no focus of its
own and no events. V11 is the precedent: Zed's own empty pane is an inline branch in
`Pane::render` whose placeholder carries no `track_focus`, because the enclosing
div already tracks the pane's handle — exactly the arrangement `AppShell` gets here.
Blue team places the method shape locally: `sidebar.rs::row_label` (`:120`) is the
same `&self, cx: &App` stateless helper, where `render_rows` takes `&mut self` only
because it needs `cx.listener` for real interaction. `render_empty` has none.

Zed also adds a `KeyContext` marker (`"EmptyPane"`) in that branch. **Not copied.**
No binding in that checkout targets it either, and a key context no binding names is
dead code.

Two strings, because the two states are not equally actionable:

- **A vault is open, no note selected:** `"Select a note."` The sidebar is right
  there, and this names something the window can do.
- **No vault** (`Launch::Nothing`): `"No vault is open."` / `"Restart with a folder
  path to open one."` **[arbitrated]** Blue team wrote this and argued against
  naming the literal invocation: `cargo run -p inkra-app <path>` is a development
  command that goes stale the day this ships as a binary. "Restart with a folder
  path" names the real mechanism — `bootstrap.rs:81-95`, the first CLI argument,
  tested with `path.is_dir()` — without over-specifying it.

The rule neither string may break: the text must not name an action the window
cannot perform. The native picker is deferred (`prompt_for_paths`,
`gpui/src/app.rs:1687`) and creating a note needs the create/rename/delete slice.

Chrome scale, **stated**, per CLAUDE.md: `theme::UI_TEXT_SIZE` and
`theme::UI_LINE_HEIGHT`. A new `theme::placeholder_text()`. Blue team checked
reusing `sidebar_text()` and rejected it on the file's own terms: `theme.rs` names
one function per (surface, role) pair, and `sidebar_text()` is calibrated for
`sidebar_background()` — a different literal from the `editor_background()` this
text sits on.

---

## Steps

### Step 1 — the editor becomes optional, and focus follows it

One slice, not two: a focus test written before the editor is optional passes
against both arms and is therefore decoration.

- `bootstrap::root`: build a `Buffer` and an `EditorView` only in the `Launch::Note`
  arm; pass `Option<Entity<EditorView>>` to `AppShell::new`. **Signature unchanged**
  (V15/V16), and both call sites keep discarding their window.
- `AppShell`: the struct above, the branching `focus_handle`, `track_focus`,
  `render_empty`, and `open_note` using its `WeakEntity` — with the V15 doc comment.
- `theme::placeholder_text()`.
- `shell.rs::tests`: the `editor(…)` accessor; `open_vault` drops its fourth return;
  `two_notes` returns the `WindowHandle` instead of the editor; its three callers and
  `clicking_a_row_hands_its_buffer_to_the_editor` move to the accessor.
- `vault.rs::open_shell` (`:711-719`): wrap the editor in `Some`.

**Tests.** Each must fail against the arm it guards. Verify that by breaking the
arm, running the test, and restoring it — not by reasoning about it.

1. `typing_reaches_a_note_opened_after_launch` — the F1 guard, and the reason this
   plan is not four lines. Open a vault (no editor at launch), activate a note,
   `simulate_input("hi")`, assert the buffer holds it. Against an arm that creates
   the editor but never focuses it, V9 says the keystrokes go nowhere. Red team
   confirmed the teeth.
2. `launching_on_a_vault_opens_no_editor` — asserts `shell.editor.is_none()`.
   Nothing here renders, so "the empty state is drawn" is not a claim a
   `#[gpui::test]` can make; the shot in step 2 is what checks it was drawn.
3. `the_save_action_reaches_the_shell_with_no_note_open` — **[arbitrated] red team
   defect 2, accepted; its proposed fix rejected.** The first draft asserted "no
   panic and no write", which is vacuous: `save_current` (`vault.rs:403-407`)
   returns at its `None` guard anyway, so both arms are observationally identical.
   Red team proposed instrumenting `AppShell::save` with a test-only counter. This
   repo does not add production state to make a test work. `Window::is_action_available`
   (V18) is the positive signal with no production change: with no note open, assert
   `Save` is reachable from the current focus. Against a missing `track_focus`, V19
   says dispatch falls back to the view-boundary node above the div carrying the
   listener, so the action is unreachable and the assertion fails. The window must
   have drawn once first (V18), which `VisualTestContext` gives.
4. `activating_a_note_replaces_the_empty_state` — `is_none()` before, `is_some()`
   after, and the buffer is the activated note's.
5. **[arbitrated] Blue team's missing test, accepted.** `root_builds_an_editor_only_for_a_named_file`
   — call `bootstrap::root` with each of the three `Launch` variants against a
   `MemFs` and assert `editor.is_none()` for `Nothing` and `Vault`, `is_some()` for
   `Note`. Every other test builds `AppShell` through the hand-rolled helpers and
   never calls `root`, so a regression that restores the unconditional editor build
   — defeating this plan entirely — passes all four tests above and ships.

**Rejected — blue team's second missing test.** It proposes factoring the copy
selection into `fn empty_state_text(has_vault: bool) -> &'static str` and asserting
it in a plain `#[test]`. That test restates the match arm it tests; it cannot fail
except by being edited alongside the thing it guards. CLAUDE.md's bar is that a
visual constant is guarded only where a *behaviour* reads it, and choosing between
two literals is not one. The shot is the check.

The four existing `AppShell`-rooted tests must still pass unchanged in substance.

**Documentation, in this same change** (CLAUDE.md requires it, and the launch-state
table is now wrong in two rows):

- `CLAUDE.md`: `AppShell` holds an `Option<Entity<EditorView>>` and its own
  `FocusHandle`; the don't-recreate table gains the empty state and
  `theme::placeholder_text()`; a line on why `open_note` needs no `Window`.
- `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`: the launch-state table's
  "no argument" and "vault scanned, nothing opened yet" rows; the Deferred row "Save
  the unnamed launch buffer" closes, with the reason it closed by removal rather
  than by saving.
- `docs/roadmap.md`: the Application section, if it names this.

### Step 2 — the shot scene

`editor-empty` (`shot/scenes.rs:41-45`) claims to show "an empty buffer, an empty
sidebar". After step 1 that is false. Rename to `empty-no-vault`, restate `shows`,
and let `sidebar-vault` cover the vault-with-no-note state it already produces. Red
team confirmed all six scenes still produce the states they are built for.

A scene is not a test and proves nothing on its own. This step exists so the eyeball
pass has something to look at; step 1 does not discharge it.

### Step 3 — verification

`./scripts/check.sh` green, plus the eyeball pass CLAUDE.md makes mandatory for a
change to what the window draws:

- `cargo run -p inkra-app --features shot --example shot`, and look at
  `empty-no-vault` and `sidebar-vault`.
- `cargo run -p inkra-app` with no argument — the empty state, and `⌘S` does not
  panic.
- `cargo run -p inkra-app <a directory>` — the tree, the empty state, then click a
  row and **type**. This is the one check `check.sh` cannot make and the shot runner
  can only half-make: F1's symptom is that the editor appears and typing does
  nothing.

### Step 4 — `/comment-cleanup`

Once, at the end, over the whole change.

---

## Known gaps this plan does not close

- **The empty state offers no action.** Both it would want are deferred: the native
  folder picker, and creating a note. The text therefore instructs rather than
  affords. That is honest but not good, and it is the strongest argument for doing
  create/rename/delete next.
- **The sidebar still cannot take focus**, so "focus the editor on every activation"
  is untestable as a *preference* — there is nothing to take focus from. The
  `on_focus` forwarding shape (V8) is where that lands.
- **`Launch::Note` is unchanged.** A file named on the command line still builds its
  editor at launch and still adopts.
- **Nothing asserts the empty state's pixels.** No snapshot testing while the UI is
  unstable; the shot is looked at, not diffed.
- **`open_note`'s window resolution is invisible at the call site.** Mitigated by a
  doc comment, not by the type system. If a later change moves `AppShell`'s
  construction out of `build_root_view`, focus breaks silently and only test 1
  catches it.
