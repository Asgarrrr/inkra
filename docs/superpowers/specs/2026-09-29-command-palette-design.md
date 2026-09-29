# Command palette

Status: **built 2026-09-29.** Red/blue reviewed before code.

## Why

Every Inkra command has a key and no other entry point. A user who forgets
`⌘⇧N` has no way to find it. The palette lists the commands the focused view
can run, finds one by typing, shows its key, and runs it. The modular
architecture design places it in `app` (Where future work lands).

## What the user decided

Taken in conversation on 2026-09-29, one question at a time:

1. The palette lists the actions available at the focus, as Zed does, minus a
   hide list of low-level actions. A new action shows by default.
2. A command matches by **word start**, the rule `⌘P` applies to a name. Fuzzy
   subsequence matching was weighed: it suits ~20 short names, but it adds a
   second matching model to the app, and the user chose one rule everywhere.
3. Commands run from the palette list first, newest first, kept in memory. The
   rest follow in alphabetical order. Nothing persists across launches.

## What Zed does

`crates/command_palette/src/command_palette.rs`:

- `CommandPalette::toggle` hides any other modal first, then takes
  `window.focused(cx)`. `new` builds the list from `window.available_actions(cx)`,
  drops what `CommandPaletteFilter` hides, and names each action with
  `humanize_action_name` (`editor::GoToDefinition` → `editor: go to definition`).
- `render_match` shows the name, lit, and `KeyBinding::for_action_in(action,
  &previous_focus_handle)`.
- `confirm` refocuses the previous handle, dismisses, then
  `window.dispatch_action(action)`, which defers.
- It matches with `crates/fuzzy`, ranks by invocation counts stored in a
  database, and keeps a query history.

## Deviations from Zed

| Zed | Inkra | Why |
|---|---|---|
| `namespace: humanized name` | Humanized name only, sentence case: `New note` | Inkra's namespaces are module names. `quick_open: quick open` repeats itself; a note app needs no namespace to tell commands apart. A test asserts no two shown commands share a name. |
| Hidden by namespace or type, through a global filter | Hidden by type, one list in the palette module | One consumer; a global earns its place with a second. Types, not strings: a renamed action fails to compile. |
| Fuzzy subsequence | Word start, as `⌘P` | Decision 2. |
| Counts in a database | Recent commands in memory | Decision 3. Inkra persists no app state yet. |
| Query history (`up`/`down` over past queries) | None | `up`/`down` select rows, as in every Inkra picker. |
| Secondary confirm opens the keymap editor | None | Inkra has no keymap editor. |
| `confirm` focuses `previous_focus_handle` as is | The captured handle only while the last frame still draws it, else the shell's root | A trash can drop the editor under the palette; focus on a dropped view's handle reaches no action, not even the shell's. |
| Key built at render (`KeyBinding::for_action_in`) | Key built at open into `Command` | One-shot list; a row draws clones only. |
| Keys as gpui prints them (`⌘⇧N`, `⌘enter`) | Apple order and glyphs (`⇧⌘N`, `⌘↩`) | gpui's `Display` prints modifiers as `^⌥⌘⇧` and multi-char keys raw. Zed's `ui::KeyBinding` draws its own; Inkra has no `ui` crate. |

## Design

### D1. Opening

`⌘⇧P` binds `command_palette::Toggle` in the `AppShell` context, as `⌘P`
binds `QuickOpen`. The handler runs on `AppShell`, in this order:

1. If the modal is already the palette, dismiss it and return.
2. If another picker is open, `dismiss_modal` it. Focus returns to that
   picker's restore target at once (`Window::focus` sets it immediately), and
   the last rendered frame still holds that target's node.
3. `target` is `window.focused(cx)`, else the shell's own `focus_handle` field
   (the root, which `track_focus`es; not `Focusable::focus_handle`, which is the
   editor's). The handler runs only from a key in the `AppShell` context, and
   gpui draws before it dispatches a key, so this handle is drawn: the list,
   the keys and the dispatch start from one handle.
4. The commands are built for `target` (D2, D5), then `toggle_picker`
   opens the palette.

The list, the keys and the dispatch therefore all answer for one handle, over
a picker or not. It opens with no vault too: `Quit` needs none. Vault commands
are listed and do nothing, as their keys do.

### D2. What is listed

`Command { name: SharedString, key: Option<SharedString>, action: Box<dyn Action> }`,
built once at open from `window.available_actions(cx)`. It lists every action
with a listener on the focused element's dispatch path, bound or not, plus the
global listeners (`Quit`).

A command is hidden when its action is in one list of types, compared by
`Action::name_for_type()`:

- `text_input`: every action except `SelectAll` (moves and deletions);
- `editor::{Up, Down, SelectUp, SelectDown, DocStart, DocEnd, Newline}`;
- `sidebar::{SelectPrev, SelectNext}`;
- `field::{Confirm, Cancel}`;
- `command_palette::Toggle`.

`Select all`, `Copy`, `Cut` and `Paste` show. A new action shows until it joins
the list. `picker::*` needs no entry: the palette dismisses any picker before it
builds the list (D1).

`name` is `humanize(action.name())`: the part after the last `::`, a space
before each uppercase letter that follows a lowercase one, that letter
lowercased. `vault_tree::NewNote` → `New note`, `editor::FollowLink` →
`Follow link`, `inkra::Quit` → `Quit`.

### D3. Matching and order

`inkra-index` gains one public function, in `find/label.rs`:

```rust
/// Each lit byte range of `text` when every term of `query` starts one of its
/// words, as `⌘P` matches a name; `None` when a term starts none.
pub fn word_starts(query: &NoteQuery, text: &str) -> Option<Vec<Range<usize>>>;
```

The delegate builds `NoteQuery::new(query, None, &[])` once per keystroke, as
vault search does, so the query folds once. `word_starts` reuses `word_spans`,
`fold`, `word_prefix` and `light`, so a term lights the same graphemes it lights
in a quick-open name. An empty query matches with no range. One word may serve
several terms: `new n` matches `New folder` as well as `New note`. Placing it
in `index` follows the placement rule: matching is search, and it reads no
file.

Order, for any query: recent commands first, newest first, then the rest by
`name`. The list is sorted once at open; a query filters it in place.

`update_matches` sets the rows synchronously, then calls `matches_updated(0)`
through `App::defer` on the picker's weak handle. The delegate holds only
`&mut self` while `Picker` is borrowed, so it cannot call the picker directly.
`App::defer` runs in the same effect flush, before any draw and before the next
key, so the first frame shows row 0 selected and `enter` never waits. A spawned
task could land after a frame and flash the empty text.

With no match, `empty_text` is `No matching command`.

### D4. Running a command

`Picker<CommandPaletteDelegate>` emits `CommandChosen(Box<dyn Action>)` from
`confirm`, then `Dismissed` as every picker does. The palette handler opens it
with `toggle_picker` and subscribes with a window itself (`cx.subscribe_in`),
holding the subscription in the `Modal` (`Modal::hold`).

`toggle_picker_with` stays on `cx.subscribe`, without a window. Quick open and
vault search reach `show_note`, which borrows the window through
`cx.with_window`. Inside a `subscribe_in` callback the window is already
leased, so `with_window` returns `None` and the selection and focus are skipped
silently (`gpui/src/app.rs`, `update_window_erased`). Found while building
slice 1: three vault-search tests failed. Passing the window into `show_note`
would remove the lease hazard, but its other callers (the sidebar's
`NoteActivated`, `follow`) have no window. That refactor is not this feature's.

On `CommandChosen` the shell, in this order:

1. remembers the action name in `recent_commands` (`remember`, made generic
   over the element type, `RECENT` long);
2. focuses the handle captured at open while it is still drawn, else the
   shell's root handle; then `dismiss_modal`, which leaves focus alone because
   the picker no longer holds it;
3. calls `window.dispatch_action(event.0.boxed_clone(), cx)`, as Zed's
   `confirm` does.

`window.dispatch_action` captures the focus step 2 just set and defers, so the
target is the captured handle while it is drawn, else the shell's. Deferring
matters: a command `AppShell` handles (`Save`, `Quick open`) would otherwise
update the shell inside the shell's own update, and gpui panics.
`FocusHandle::dispatch_action` is not used: it runs at once, and it drops the
action when its node is gone, for example after the note was trashed while the
palette was open.

Step 2 does not rely on the `Modal`'s restore target: a note activation in
flight when the palette opened lands while it is open and calls
`Modal::restore_to` with the editor's handle. `dismiss_modal` would then run a
command listed for the tree on the editor. The handle captured at open is the
one the list was built for, so the command runs where it was listed. A test
holds a load, opens the palette over the tree, releases the load, and runs
`Rename`.

Dismissing before dispatching makes a command that opens a picker (`Quick
open`) open it over an empty modal layer. Dismissing drops the palette's
subscriptions, so its queued `Dismissed` reaches no one and cannot close the
new picker.

### D5. Row

One `UI_LINE_HEIGHT` row: the name, lit through `LitText::line` and `styled`,
`flex_1().min_w_0()` with an ellipsis; the key right, `flex_none`, in
`theme::placeholder_text()`.

The key is `window.highest_precedence_binding_for_action_in(action, &target)`,
each keystroke drawn by `key_label`: modifiers in Apple order `⌃⌥⇧⌘`, then the
key — one character uppercased, `enter` `↩`, `backspace` `⌫`, `delete` `⌦`,
`escape` `⎋`, `tab` `⇥`, the arrows as arrows, `space` `Space`, any other
capitalised. Keystrokes join with a space. `cmd-shift-n` → `⇧⌘N`,
`cmd-enter` → `⌘↩`.

Name and key are computed at open into the `Command`, never per frame: a row
draws clones of `SharedString`s. `row_label` is `name`, then `, key` when there
is one.

### D6. Tests

`inkra-index`, `find/label_tests.rs`:
- unit cases: two terms in any order; a term inside a word does not match; one
  word serving two terms; case folding; `ete` on `Été` lights `0..5`, `et`
  lights `0..3` (whole graphemes); an empty query.
- proptest, over text with a combining mark and `-`: `word_starts(text, text)`
  is `Some` and each word start lies in a lit range (a word that folds to
  nothing, a lone combining mark, is never a term and is not lit); term order
  and query case change nothing; adding a term never turns `None` into `Some`;
  ranges are ascending, disjoint, on char boundaries, each starts on a
  grapheme boundary and holds a word start.

`app`, headless `#[gpui::test]` against `MemFs`:
- `cmd-shift-p` opens the palette; a second `cmd-shift-p` closes it.
- Editor focused: `Undo`, `Save`, `Select all` listed; `Left`, `Up`,
  `Backspace` not.
- A note open, tree focused: `Rename` listed, `Undo` and `Select next` not.
- The rename field focused: `Select all` listed, `Confirm` and `Cancel` not;
  `escape` then a name renames the note.
- A note load held while the palette opens over the tree, released before
  `Rename` is chosen: the tree's note is renamed and the editor shows the
  landed note.
- `new no` then `enter`, then a name: the note exists. `new no` excludes `New
  folder`.
- Typing `x` gives `alphax`; the palette's `Undo` gives `alpha` again.
- Choosing `Quick open` leaves the quick-open picker open.
- `cmd-shift-p` over `cmd-p` lists the note's commands (`Undo`).
- No vault, `Quit`'s listener registered: the palette opens and lists `Quit`.
- Every shown name is unique, over `cx.all_action_names()` minus the hidden.
- Slice 2: a row's label carries its key (`New note, ⌘N`); recent order.
- The note trashed under the palette: the chosen command lands on the root, so
  `cmd-shift-p` still reaches the shell.
- The rename field focused: `Rename` shows with no key, its `enter` shadowed by
  the field's.
- `humanize` and `key_label` cases, in the palette's test file: every modifier
  in Apple order, each named key's glyph, `f1` → `F1`, a chord `cmd-k cmd-s` →
  `⌘K ⌘S` through `keys_label`; every key in `key_bindings()` of a shown action
  draws with no ASCII lowercase letter.

Render: a `command-palette` shot scene with the editor focused. Look at it.

## Slices

1. **List, match, run.** `word_starts`, the delegate, `Toggle` and its
   binding, D1, D2, D4 without recents, the name-only row. Tests for all of it
   except keys and recents. Shot scene.
2. **Keys and recents.** The key column (D5), `recent_commands`, the order in
   D3. Their tests, the scene re-rendered.

## Risks

- `available_actions` reads the last rendered frame. A view focused but not yet
  drawn contributes no actions. The palette opens on a key press, after a
  frame, so only a test that focuses and presses without `run_until_parked`
  can reach this.
- A new low-level action shows until it joins the hide list. That is decision
  1's trade: showing too much is visible and cheap to fix.
- Keys are drawn for macOS only; Inkra ships on macOS only.

## Deferred

| Item | Trigger |
|---|---|
| Persisted usage order | A second piece of app state to persist (window size, open notes). |
| Query history | Someone reruns the same query often. |
| Actions with arguments in the list | An action that carries data joins the keymap. |
| A global hide filter, as Zed's `CommandPaletteFilter` | A second consumer of the hide list. |
| Each module lists its own palette-hidden actions (Zed's owners register with `CommandPaletteFilter`) | A new move or field action leaks into the palette, or a third module's actions join the hide list. |
