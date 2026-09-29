# Command palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `⌘⇧P` lists the commands the focused view can run, finds one by word start, shows its key, and runs it on the view that was focused.

**Architecture:** A `PickerDelegate` in `crates/app/src/command_palette/` over a list built once at open from `window.available_actions`. Matching is one new public function in `inkra-index`, `word_starts`. The shell opens it through the existing modal layer and dispatches the chosen action with `window.dispatch_action` after dismissing, as Zed does.

**Tech Stack:** Rust 2024, gpui (vendored at `vendor/zed-reference/crates/gpui`), proptest.

Spec: `docs/superpowers/specs/2026-09-29-command-palette-design.md`. Read it first; D1–D6 below refer to it. It was red/blue reviewed; its reasons are not repeated here.

Rules for every task (from `CLAUDE.md`):

- Format with `cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown -p inkra-index`. **Never `cargo fmt --all`.**
- Comments: one line, a why the code cannot show. No narration.
- Tests live in their own file. Every key binding gets a `simulate_keystrokes` test.
- A colour or a size comes from `crates/app/src/theme.rs`, never a literal.
- Never run `cargo mutants --in-place`.
- The code below is the intended shape. If a signature differs from the real one, adapt to the real one and say so in the report; do not change the behaviour.

---

## File structure

| File | Change | Role |
|---|---|---|
| `crates/index/src/find/label.rs` | create | `word_starts` |
| `crates/index/src/find/label_tests.rs` | create | its unit tests and proptest |
| `crates/index/src/find.rs` | modify | `mod label;`, `#[cfg(test)] mod label_tests;`, `pub use label::word_starts;` |
| `crates/index/src/lib.rs` | modify | re-export `word_starts` |
| `crates/app/src/command_palette/mod.rs` | create | `Toggle`, `CommandChosen`, `CommandPaletteDelegate` |
| `crates/app/src/command_palette/commands.rs` | create | `Command`, `humanize`, `hidden`, `commands` (slice 2: `key_label`) |
| `crates/app/src/command_palette/tests.rs` | create | `humanize`, uniqueness, `key_label` tests |
| `crates/app/src/lib.rs` | modify | `mod command_palette;` |
| `crates/app/src/keymap.rs` | modify | `cmd-shift-p` |
| `crates/app/src/shell.rs` | modify | the `Toggle` handler; slice 2: `recent_commands`, generic `remember` |
| `crates/app/src/shell/modal.rs` | modify | `modal_is::<D>` |
| `crates/app/src/shell/tests/command_palette.rs` | create | app tests |
| `crates/app/src/shell/tests/mod.rs` | modify | `mod command_palette;`, `palette_names` |
| `crates/app/src/shot/scenes/command_palette.rs` | create | the scene |
| `crates/app/src/shot/scenes.rs` | modify | register the scene |
| `CLAUDE.md` | modify | the spec in the Docs list |

---

# Slice 1 — list, match, run

### Task 1: `word_starts` in `inkra-index`

**Files:** create `crates/index/src/find/label.rs`, `crates/index/src/find/label_tests.rs`; modify `crates/index/src/find.rs`, `crates/index/src/lib.rs`.

- [ ] **Step 1: Write the failing tests** — `crates/index/src/find/label_tests.rs`:

```rust
use proptest::prelude::*;

use super::label::word_starts;
use crate::words::{NoteQuery, word_spans};

fn lit(query: &str, text: &str) -> Option<Vec<std::ops::Range<usize>>> {
    word_starts(&NoteQuery::new(query, None, &[]), text)
}

#[test]
fn every_term_must_start_a_word_in_any_order() {
    assert_eq!(lit("new no", "New note"), Some(vec![0..3, 4..6]));
    assert_eq!(lit("note new", "New note"), Some(vec![0..3, 4..8]));
    assert_eq!(lit("ote", "New note"), None);
    assert_eq!(lit("new x", "New note"), None);
}

#[test]
fn one_word_may_serve_two_terms() {
    assert_eq!(lit("new n", "New folder"), Some(vec![0..3]));
    assert_eq!(lit("n ne", "New note"), Some(vec![0..2, 4..5]));
}

#[test]
fn folds_case_and_lights_whole_graphemes() {
    assert_eq!(lit("SAVE", "Save"), Some(vec![0..4]));
    assert_eq!(lit("ete", "\u{c9}t\u{e9}"), Some(vec![0..5]));
    assert_eq!(lit("et", "\u{c9}t\u{e9}"), Some(vec![0..3]));
}

#[test]
fn an_empty_query_matches_with_nothing_lit() {
    assert_eq!(lit("", "Quit"), Some(vec![]));
    assert_eq!(lit("  ", "Quit"), Some(vec![]));
}

fn text() -> impl Strategy<Value = String> {
    "[a-zA-Z\u{e9}\u{c9} ]{0,24}"
}

proptest! {
    #[test]
    fn a_text_matches_itself_at_every_word(text in text()) {
        let found = lit(&text, &text);
        prop_assert!(found.is_some());
        let found = found.unwrap_or_default();
        for (at, _) in word_spans(&text) {
            prop_assert!(found.iter().any(|range| range.start == at), "{at} unlit in {found:?}");
        }
    }

    #[test]
    fn term_order_and_case_change_nothing(a in "[a-z\u{e9}]{1,4}", b in "[a-z\u{e9}]{1,4}", text in text()) {
        let ab = lit(&format!("{a} {b}"), &text);
        prop_assert_eq!(&ab, &lit(&format!("{b} {a}"), &text));
        prop_assert_eq!(&ab, &lit(&format!("{} {}", a.to_uppercase(), b.to_uppercase()), &text));
    }

    #[test]
    fn a_term_more_never_matches_more(a in "[a-z]{1,4}", b in "[a-z]{1,4}", text in text()) {
        if lit(&a, &text).is_none() {
            prop_assert!(lit(&format!("{a} {b}"), &text).is_none());
        }
    }

    #[test]
    fn ranges_ascend_apart_on_word_starts(query in "[a-z\u{e9} ]{0,8}", text in text()) {
        let starts: Vec<usize> = word_spans(&text).map(|(at, _)| at).collect();
        let mut end = None;
        for range in lit(&query, &text).into_iter().flatten() {
            prop_assert!(range.start < range.end && range.end <= text.len());
            prop_assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            prop_assert!(starts.contains(&range.start));
            prop_assert!(end.is_none_or(|end| end < range.start));
            end = Some(range.end);
        }
    }
}
```

If `lit("new no", "New note")` returns `4..6` as one grapheme run per term, the second range is `4..6`; the test expects that.

- [ ] **Step 2: See them fail.** In `crates/index/src/find.rs` add `mod label;` to the module list, `#[cfg(test)] mod label_tests;` beside the other `*_tests` declarations, and `pub use label::word_starts;` beside `pub use excerpt::{Excerpt, excerpt};`. Create `label.rs` returning `None`. Run `cargo test -p inkra-index label`. Expected: failures.

- [ ] **Step 3: Implement** — `crates/index/src/find/label.rs`:

```rust
//! Word-start matching on one short label, as `⌘P` matches a name (command palette spec, D3).

use std::ops::Range;

use super::{light, word_prefix};
use crate::words::{NoteQuery, fold, word_spans};

/// Each lit byte range of `text` when every term of `query` starts one of its words, as `⌘P`
/// matches a name; `None` when a term starts none.
#[must_use]
pub fn word_starts(query: &NoteQuery, text: &str) -> Option<Vec<Range<usize>>> {
    let terms = query.terms();
    let mut found = vec![false; terms.len()];
    let mut lit = Vec::new();
    for (at, word) in word_spans(text) {
        let folded = fold(word);
        for (term, found) in terms.iter().zip(&mut found) {
            if folded.starts_with(&**term) {
                *found = true;
                light(&mut lit, word_prefix(text, at..at + word.len(), term));
            }
        }
    }
    found.into_iter().all(|found| found).then_some(lit)
}
```

Add `word_starts` to the `pub use find::{…}` line in `crates/index/src/lib.rs`.

- [ ] **Step 4: Run.** `cargo test -p inkra-index label`. Expected: PASS. Commit any `.proptest-regressions` file written.

- [ ] **Step 5: Commit.** `git commit -m "Add word_starts: word-start matching on one label"` with the four files.

### Task 2: the command list

**Files:** create `crates/app/src/command_palette/{mod.rs,commands.rs,tests.rs}`; modify `crates/app/src/lib.rs`.

- [ ] **Step 1: Write the failing tests** — `crates/app/src/command_palette/tests.rs`:

```rust
use std::collections::HashSet;

use super::commands::{hidden_name, humanize};

#[test]
fn humanize_drops_the_namespace_and_splits_words() {
    assert_eq!(humanize("vault_tree::NewNote"), "New note");
    assert_eq!(humanize("vault_search::SearchVault"), "Search vault");
    assert_eq!(humanize("editor::FollowLink"), "Follow link");
    assert_eq!(humanize("text_input::SelectAll"), "Select all");
    assert_eq!(humanize("inkra::Quit"), "Quit");
}

/// Over every registered action, so no focus can show two commands with one name.
#[gpui::test]
fn every_shown_name_is_unique(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let shown: Vec<String> = cx
            .all_action_names()
            .iter()
            .filter(|name| !hidden_name(name))
            .map(|name| humanize(name))
            .collect();
        let unique: HashSet<&String> = shown.iter().collect();
        assert_eq!(unique.len(), shown.len(), "{shown:?}");
    });
}
```

`mod.rs` for now:

```rust
//! `⌘⇧P`: the commands the focused view can run, found by word start (command palette spec).

mod commands;
#[cfg(test)]
mod tests;
```

Add `mod command_palette;` to `crates/app/src/lib.rs` in alphabetical order. Run `cargo test -p inkra-app command_palette`. Expected: compile error.

- [ ] **Step 2: Implement** — `crates/app/src/command_palette/commands.rs`:

```rust
use gpui::{Action, App, SharedString, Window};

use crate::editor::actions as editor;
use crate::field;
use crate::picker;
use crate::sidebar;
use crate::text_input::actions as text_input;

/// Moves, deletions and field keys: a palette entry for one would do nothing a user wants.
fn hidden_names() -> [&'static str; 24] {
    [
        text_input::Backspace::name_for_type(),
        text_input::Delete::name_for_type(),
        text_input::Left::name_for_type(),
        text_input::Right::name_for_type(),
        text_input::SelectLeft::name_for_type(),
        text_input::SelectRight::name_for_type(),
        text_input::Home::name_for_type(),
        text_input::End::name_for_type(),
        text_input::LineStart::name_for_type(),
        text_input::LineEnd::name_for_type(),
        editor::Up::name_for_type(),
        editor::Down::name_for_type(),
        editor::SelectUp::name_for_type(),
        editor::SelectDown::name_for_type(),
        editor::DocStart::name_for_type(),
        editor::DocEnd::name_for_type(),
        editor::Newline::name_for_type(),
        sidebar::SelectPrev::name_for_type(),
        sidebar::SelectNext::name_for_type(),
        picker::SelectPrev::name_for_type(),
        picker::SelectNext::name_for_type(),
        field::Confirm::name_for_type(),
        field::Cancel::name_for_type(),
        super::Toggle::name_for_type(),
    ]
}
```

If the module paths differ (`crate::editor::actions` may be private), widen visibility to `pub(crate)` at the declaration, the smallest change.

```rust
pub(super) fn hidden_name(name: &str) -> bool {
    hidden_names().contains(&name)
}

pub(crate) struct Command {
    pub(crate) name: SharedString,
    pub(crate) action: Box<dyn Action>,
}

/// `vault_tree::NewNote` → `New note`: the last segment, a space before each word's capital.
pub(super) fn humanize(name: &str) -> String {
    let base = name.rsplit_once("::").map_or(name, |(_, base)| base);
    let mut out = String::with_capacity(base.len() + 4);
    let mut prev_lower = false;
    for c in base.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
        prev_lower = c.is_lowercase();
    }
    out
}

/// The commands available at the focused element, by name; built before the palette takes focus.
pub(crate) fn commands(window: &Window, cx: &App) -> Vec<Command> {
    let mut commands: Vec<Command> = window
        .available_actions(cx)
        .into_iter()
        .filter(|action| !hidden_name(action.name()))
        .map(|action| Command {
            name: humanize(action.name()).into(),
            action,
        })
        .collect();
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    commands
}
```

- [ ] **Step 3: Run.** `cargo test -p inkra-app command_palette` — both tests PASS once Task 3 declares `Toggle`. Do not commit until Task 3.

### Task 3: the delegate, the binding, the shell handler

**Files:** modify `crates/app/src/command_palette/mod.rs`, `crates/app/src/keymap.rs`, `crates/app/src/shell.rs`, `crates/app/src/shell/modal.rs`, `crates/app/src/shell/tests/mod.rs`; create `crates/app/src/shell/tests/command_palette.rs`.

- [ ] **Step 1: Write the failing app tests.**

In `crates/app/src/shell/tests/mod.rs`: add `mod command_palette;` to the module list, and in its `impl AppShell` block:

```rust
    /// The open palette's row labels; `None` when it is not open.
    pub(crate) fn palette_names(&self, cx: &gpui::App) -> Option<Vec<String>> {
        let picker = self
            .modal
            .as_ref()?
            .view()
            .clone()
            .downcast::<Picker<crate::command_palette::CommandPaletteDelegate>>()
            .ok()?;
        let delegate = &picker.read(cx).delegate;
        Some(
            (0..delegate.match_count())
                .map(|ix| delegate.row_label(ix).to_string())
                .collect(),
        )
    }
```

`crates/app/src/shell/tests/command_palette.rs`:

```rust
use std::sync::Arc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, WindowHandle};
use inkra_vault::mem_fs::MemFs;

use super::{activate, editor, tree_focused, two_notes, vault_names};
use crate::Quit;
use crate::keymap::key_bindings;
use crate::shell::{AppShell, NoteStore};
use crate::sidebar::Sidebar;

/// Note `a` (`alpha`) open; `open_note` leaves the editor focused.
fn editor_focused(
    cx: &mut TestAppContext,
) -> (WindowHandle<AppShell>, VisualTestContext, Entity<Sidebar>, Entity<NoteStore>) {
    let fs = Arc::new(MemFs::new());
    let (window, sidebar, notes, a, _b) = two_notes(cx, &fs);
    let visual = tree_focused(cx, window, &sidebar);
    activate(&sidebar, cx, a);
    visual.run_until_parked();
    (window, visual, sidebar, notes)
}

fn names(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    window.update(cx, |shell, _, cx| shell.palette_names(cx)).unwrap()
}

fn listed(window: WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Vec<String> {
    names(window, cx).expect("the palette is open")
}

fn text(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> String {
    let editor = editor(window, cx);
    editor.read_with(cx, |editor, cx| editor.buffer().read(cx).text().to_string())
}

#[gpui::test]
fn cmd_shift_p_opens_the_palette_and_closes_it(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_some());
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_none());
}

#[gpui::test]
fn the_editor_lists_its_commands_not_its_caret_moves(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    for shown in ["Undo", "Save", "Select all", "New note", "Quick open"] {
        assert!(names.iter().any(|name| name == shown), "{shown} missing from {names:?}");
    }
    for hidden in ["Left", "Up", "Backspace", "Confirm", "Toggle"] {
        assert!(!names.iter().any(|name| name == hidden), "{hidden} shown in {names:?}");
    }
}

#[gpui::test]
fn the_tree_lists_its_commands_not_the_editors(cx: &mut TestAppContext) {
    let (window, _visual, sidebar, _) = editor_focused(cx);
    let mut visual = tree_focused(cx, window, &sidebar);
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(names.iter().any(|name| name == "Rename"), "{names:?}");
    assert!(!names.iter().any(|name| name == "Undo"), "{names:?}");
}

#[gpui::test]
fn typing_narrows_by_word_start(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p n e w space n o");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["New note"]);
}

#[gpui::test]
fn choosing_new_note_creates_a_note(cx: &mut TestAppContext) {
    let (window, mut visual, _, notes) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p n e w space n o enter");
    visual.run_until_parked();
    assert!(names(window, &mut visual).is_none());
    visual.simulate_keystrokes("p l a n enter");
    visual.run_until_parked();
    assert_eq!(vault_names(&notes, &mut visual), ["a.md", "b.md", "plan.md"]);
}

#[gpui::test]
fn choosing_undo_reaches_the_editor(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("x");
    visual.run_until_parked();
    assert_eq!(text(&window, &mut visual), "alphax");
    visual.simulate_keystrokes("cmd-shift-p u n d o enter");
    visual.run_until_parked();
    assert_eq!(text(&window, &mut visual), "alpha");
}

#[gpui::test]
fn choosing_quick_open_leaves_quick_open_open(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p q u i c k enter");
    visual.run_until_parked();
    let quick_open = window.update(&mut visual, |shell, _, cx| shell.quick_open_state(cx)).unwrap();
    assert!(quick_open.is_some(), "the palette's Dismissed closed quick open");
}

#[gpui::test]
fn over_quick_open_it_lists_the_notes_commands(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-p");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(names.iter().any(|name| name == "Undo"), "{names:?}");
}

#[gpui::test]
fn opens_with_no_vault(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, _| {});
    });
    let notes = cx.update(|cx| {
        gpui::AppContext::new(cx, |cx| NoteStore::new(Arc::new(MemFs::new()), None, cx))
    });
    let window = cx.add_window(move |_, cx| AppShell::new(None, notes, cx));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.run_until_parked();
    window
        .update(&mut visual, |shell, window, cx| window.focus(&shell.focus_handle(cx), cx))
        .unwrap();
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(names.iter().any(|name| name == "Quit"), "{names:?}");
}
```

Match the helper calls to their real signatures (`two_notes`, `tree_focused`, `editor`, `vault_names` take `&mut TestAppContext`; `VisualTestContext` derefs to it). `the_tree_lists_its_commands_not_the_editors` must have a note open, so `Undo` could be listed if the dispatch path were wrong.

Run `cargo test -p inkra-app command_palette`. Expected: compile error, `CommandPaletteDelegate` missing.

- [ ] **Step 2: The delegate** — `crates/app/src/command_palette/mod.rs`, after the module lines:

```rust
use gpui::{
    Action, AnyElement, App, Context, EventEmitter, IntoElement, ParentElement, Pixels,
    SharedString, Styled, Task, actions, div,
};
use inkra_index::{NoteQuery, word_starts};

use crate::picker::{Picker, PickerDelegate};
use crate::row_text::{LitText, styled};
use crate::theme;

pub(crate) use commands::{Command, commands};

actions!(command_palette, [Toggle]);

/// The chosen command's action; the shell dispatches it once the palette is gone (spec D4).
pub(crate) struct CommandChosen(pub(crate) Box<dyn Action>);

impl EventEmitter<CommandChosen> for Picker<CommandPaletteDelegate> {}

pub(crate) struct CommandPaletteDelegate {
    commands: Vec<Command>,
    /// Each match: its command's index and its name, lit.
    rows: Vec<(usize, LitText)>,
}

impl CommandPaletteDelegate {
    pub(crate) fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            rows: Vec::new(),
        }
    }
}

impl PickerDelegate for CommandPaletteDelegate {
    fn key_context(&self) -> &'static str {
        "CommandPalette"
    }

    fn label(&self) -> &'static str {
        "Command palette"
    }

    fn placeholder(&self) -> &'static str {
        "Run a command…"
    }

    fn empty_text(&self, _: &App) -> Option<SharedString> {
        Some("No matching command".into())
    }

    fn match_count(&self) -> usize {
        self.rows.len()
    }

    fn is_selectable(&self, ix: usize) -> bool {
        ix < self.rows.len()
    }

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        let query = NoteQuery::new(query, None, &[]);
        self.rows = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(ix, command)| {
                let lit = word_starts(&query, &command.name)?;
                Some((ix, LitText::line(&command.name, lit)))
            })
            .collect();
        // The picker is borrowed here; `App::defer` lands in this flush, before any draw or key.
        let picker = cx.weak_entity();
        cx.defer(move |cx| {
            picker
                .update(cx, |picker, cx| picker.matches_updated(0, cx))
                .ok();
        });
        Task::ready(())
    }

    fn confirm(&mut self, ix: usize, cx: &mut Context<Picker<Self>>) {
        if let Some(&(command, _)) = self.rows.get(ix) {
            cx.emit(CommandChosen(self.commands[command].action.boxed_clone()));
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        let Some((_, name)) = self.rows.get(ix) else {
            return div().into_any_element();
        };
        div()
            .h(self.row_height())
            .w_full()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(styled(name, theme::match_text().into()))
            .into_any_element()
    }

    fn row_label(&self, ix: usize) -> SharedString {
        self.rows
            .get(ix)
            .map(|(_, name)| name.text().clone())
            .unwrap_or_default()
    }

    fn row_height(&self) -> Pixels {
        theme::UI_LINE_HEIGHT
    }
}
```

Check `NoteQuery` is exported from `inkra_index` (it is, `lib.rs` re-exports `words::NoteQuery`). If `.ok()` trips `clippy::let_underscore` or `unused_must_use` policy, follow how the repo already discards `WeakEntity::update` results (`let _ = …` in `picker/letters.rs`).

- [ ] **Step 3: The modal layer** — `crates/app/src/shell/modal.rs`:

1. `toggle_picker_with` does not change. It stays on `cx.subscribe`: quick open and vault search reach `show_note`, whose `cx.with_window` fails silently inside a `subscribe_in` callback (spec D4).
2. Add:

```rust
    pub(super) fn modal_is<D: PickerDelegate>(&self) -> bool {
        self.modal
            .as_ref()
            .is_some_and(|modal| modal.view.clone().downcast::<Picker<D>>().is_ok())
    }
```

and use it in `toggle_picker`'s "same kind" check so the two cannot disagree.

- [ ] **Step 4: The shell handler and the binding** — `crates/app/src/shell.rs`:

```rust
use crate::command_palette::{self, CommandChosen, CommandPaletteDelegate};
```

Add `.on_action(cx.listener(Self::command_palette))` after `search_vault` in `render`, and:

```rust
    /// Closes any other picker first, so the list, the keys and the dispatch answer for one handle (spec D1).
    fn command_palette(
        &mut self,
        _: &command_palette::Toggle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_is::<CommandPaletteDelegate>() {
            self.dismiss_modal(window, cx);
            return;
        }
        self.dismiss_modal(window, cx);
        let commands = command_palette::commands(window, cx);
        let Some(picker) =
            self.toggle_picker(window, cx, |_| CommandPaletteDelegate::new(commands))
        else {
            return;
        };
        // With a window, unlike `toggle_picker_with`: dismissing and dispatching both need one.
        let chosen = cx.subscribe_in(&picker, window, |shell, _, event: &CommandChosen, window, cx| {
            shell.dismiss_modal(window, cx);
            window.dispatch_action(event.0.boxed_clone(), cx);
        });
        if let Some(modal) = &mut self.modal {
            modal.hold(chosen);
        }
    }
```

`dismiss_modal` with no modal does nothing, so the unconditional call is the rule, not a special case. `crates/app/src/keymap.rs`: import `crate::command_palette` and add after `cmd-shift-f`:

```rust
        KeyBinding::new("cmd-shift-p", command_palette::Toggle, Some("AppShell")),
```

- [ ] **Step 5: Run.** `cargo test -p inkra-app command_palette`, then `cargo test -p inkra-app` (the modal callback changed). Expected: PASS.

- [ ] **Step 6: Commit.** `git add crates/app/src && git commit -m "Add the cmd-shift-p command palette"`.

### Task 4: the scene, the docs, the gate

**Files:** create `crates/app/src/shot/scenes/command_palette.rs`; modify `crates/app/src/shot/scenes.rs`, `CLAUDE.md`, the spec's Status.

- [ ] **Step 1: The scene**

```rust
use gpui::{WindowHandle, point};

use super::SIDEBAR_X;
use super::quick_open::PICKER_SETTLE;
use crate::shell::AppShell;
use crate::shot::stage::Stage;
use crate::sidebar::row_center;

pub(super) fn open(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_vault(&[("index.md", "# Index\n\nThe front page.\n")]);
    stage.click(window, point(SIDEBAR_X, row_center(0.)));
    stage.focus(window);
    stage.keys(window, "cmd-shift-p");
    stage.keys(window, "n");
    stage.wait(PICKER_SETTLE);
    window
}
```

Register it in `scenes.rs`: `mod command_palette;` and a `Scene { name: "command-palette", shows: "cmd-shift-p over the editor, n typed: every command with a word starting n, lit", open: command_palette::open }`. Check `stage.focus` focuses the editor, as `quick_open::results` relies on.

- [ ] **Step 2: Render and look.** `cargo run -p inkra-app --features shot --example shot command-palette`, open `target/shots/command-palette.png`. Check: one row per command, `n` lit at word starts, rows as tall as vault-search rows, nothing clipped.

- [ ] **Step 3: Docs.** In `CLAUDE.md`'s Docs list, after the query-run spec:

```markdown
- `docs/superpowers/specs/2026-09-29-command-palette-design.md` — `⌘⇧P`: the
  listed actions, the hide list, `word_starts`, the dismiss-then-dispatch order
```

Spec Status: `slice 1 built 2026-09-29`.

- [ ] **Step 4: The gate.** `./scripts/check.sh` → exit 0. `cargo mutants -p inkra-index --re 'crates/index/src/find/label\.rs'` (not in place; `-f` adds to the config's globs) → zero missed.

- [ ] **Step 5: Commit.** `git commit -m "Add the command palette scene and record the spec"`.

# Slice 2 — keys and recents

### Task 5: the key column

**Files:** modify `crates/app/src/command_palette/{commands.rs,mod.rs,tests.rs}`, `crates/app/src/shell.rs`, `crates/app/src/shell/tests/command_palette.rs`.

- [ ] **Step 1: Failing tests.** In `command_palette/tests.rs`:

```rust
use gpui::KeybindingKeystroke;

use super::commands::key_label;

fn label(source: &str) -> String {
    key_label(&KeybindingKeystroke::from_keystroke(gpui::Keystroke::parse(source).unwrap()))
}

#[test]
fn keys_draw_in_apple_order_with_glyphs() {
    assert_eq!(label("cmd-shift-n"), "⇧⌘N");
    assert_eq!(label("cmd-enter"), "⌘↩");
    assert_eq!(label("enter"), "↩");
    assert_eq!(label("cmd-backspace"), "⌘⌫");
    assert_eq!(label("ctrl-alt-a"), "⌃⌥A");
}
```

Check how `KeybindingKeystroke` is built from a `Keystroke` in gpui (`platform/keystroke.rs`; `from_keystroke` or a `From` impl) and use the real constructor.

In `shell/tests/command_palette.rs`, change `typing_narrows_by_word_start`'s expectation to `["New note, ⌘N"]`, and add:

```rust
#[gpui::test]
fn a_row_shows_its_key(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p s a v e");
    visual.run_until_parked();
    assert_eq!(listed(window, &mut visual), ["Save, ⌘S"]);
}
```

Run. Expected: FAIL.

- [ ] **Step 2: Implement.**

`commands.rs`:

```rust
/// Apple order, `⌃⌥⇧⌘`, then the key's glyph (spec D5); gpui's `Display` orders `⌘⇧` and prints `enter`.
pub(super) fn key_label(keystroke: &KeybindingKeystroke) -> String {
    let modifiers = keystroke.modifiers();
    let mut out = String::new();
    for (on, glyph) in [
        (modifiers.control, '⌃'),
        (modifiers.alt, '⌥'),
        (modifiers.shift, '⇧'),
        (modifiers.platform, '⌘'),
    ] {
        if on {
            out.push(glyph);
        }
    }
    match keystroke.key() {
        "enter" => out.push('↩'),
        "backspace" => out.push('⌫'),
        "delete" => out.push('⌦'),
        "escape" => out.push('⎋'),
        "tab" => out.push('⇥'),
        "up" => out.push('↑'),
        "down" => out.push('↓'),
        "left" => out.push('←'),
        "right" => out.push('→'),
        "space" => out.push_str("Space"),
        key => {
            let mut chars = key.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}
```

`Command` gains `key: Option<SharedString>`. `commands(window, cx, target: &FocusHandle)`:

```rust
        .map(|action| {
            let key = window
                .highest_precedence_binding_for_action_in(&*action, target)
                .map(|binding| {
                    binding
                        .keystrokes()
                        .iter()
                        .map(key_label)
                        .collect::<Vec<_>>()
                        .join(" ")
                        .into()
                });
            Command {
                name: humanize(action.name()).into(),
                key,
                action,
            }
        })
```

In the shell handler, after the second `dismiss_modal`:

```rust
        // The root, not `Focusable::focus_handle`, which answers the editor's handle.
        let target = window.focused(cx).unwrap_or_else(|| self.focus_handle.clone());
        let commands = command_palette::commands(window, cx, &target);
```

`mod.rs` `render_row`: a flex row,

```rust
        let key = self.commands[command].key.clone();
        div()
            .h(self.row_height())
            .w_full()
            .flex()
            .justify_between()
            .gap_2()
            .whitespace_nowrap()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(styled(name, theme::match_text().into())),
            )
            .children(key.map(|key| {
                div()
                    .flex_none()
                    .text_color(theme::placeholder_text())
                    .child(key)
            }))
            .into_any_element()
```

(`let Some(&(command, ref name)) = self.rows.get(ix)` to have both.) Use a spacing from `theme.rs` if one exists for this gap instead of `gap_2`. `row_label`: `name`, then `", {key}"` when `key` is `Some`.

- [ ] **Step 3: Run.** `cargo test -p inkra-app command_palette`. Expected: PASS.

- [ ] **Step 4: Commit.** `git commit -m "Show each command's key in the palette"`.

### Task 6: recent commands first

**Files:** modify `crates/app/src/shell.rs`, `crates/app/src/command_palette/commands.rs`, `crates/app/src/shell/tests/command_palette.rs`.

- [ ] **Step 1: Failing tests.**

```rust
#[gpui::test]
fn the_newest_recent_command_lists_first(cx: &mut TestAppContext) {
    let (window, mut visual, _, _) = editor_focused(cx);
    visual.simulate_keystrokes("cmd-shift-p s a v e enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("x cmd-shift-p u n d o enter");
    visual.run_until_parked();
    visual.simulate_keystrokes("cmd-shift-p");
    visual.run_until_parked();
    let names = listed(window, &mut visual);
    assert!(names[0].starts_with("Undo") && names[1].starts_with("Save"), "{names:?}");
    assert!(names[2..].is_sorted(), "the rest stays alphabetical: {names:?}");
}
```

Run. Expected: FAIL.

- [ ] **Step 2: Implement.** `shell.rs`:

```rust
/// Newest first, no repeat, at most `RECENT`.
fn remember<T: PartialEq>(recent: &mut Vec<T>, item: T) {
    recent.retain(|seen| *seen != item);
    recent.insert(0, item);
    recent.truncate(RECENT);
}
```

`AppShell` gains `recent_commands: Vec<&'static str>` (`/// The commands run from the palette, by action name, newest first.`), initialised empty. The `CommandChosen` callback starts with `remember(&mut shell.recent_commands, event.0.name());`. The handler passes `&self.recent_commands` to `commands`.

`commands(..., recent: &[&'static str])` sorts once:

```rust
    commands.sort_by_cached_key(|command| {
        let rank = recent
            .iter()
            .position(|&name| name == command.action.name())
            .unwrap_or(usize::MAX);
        (rank, command.name.clone())
    });
```

- [ ] **Step 3: Run.** `cargo test -p inkra-app`. Expected: PASS.

- [ ] **Step 4: Render, gate, commit.** Re-render `command-palette` and look at the key column: right-aligned, dimmed, not clipped. `./scripts/check.sh` → exit 0. Spec Status: `built 2026-09-29`. `git commit -m "List the palette's recent commands first"`.
