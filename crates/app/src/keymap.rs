//! Every key binding in one place, so the set can be read without hunting
//! through the views that handle them.
//!
//! A binding's key context decides where it fires. Window-wide actions bind at
//! `AppShell`, not `EditorView`: the editor's context only fires while the
//! editor has focus. `⌘S` is the precedent.
//!
//! Every editing binding names `EditorView`. That changes no behaviour today,
//! and the honest reason is narrower than it looks: a binding with no predicate
//! scores `contexts.len()` (`gpui/src/keymap.rs:246-252`), and a predicate
//! naming the *focused* frame scores the same (`keymap/context.rs:260-268`), so
//! the two **tie** — and the tie breaks on position in the `Vec` below, later
//! winning (`keymap.rs:188-190`). The tree's `down` already beat the editor's
//! only because it is pushed further down this list.
//!
//! Naming the context removes that dependency. With `EditorView` absent from
//! the focused stack the predicate fails and the binding is excluded from
//! matching, so reordering this list — an edit with no other consequence —
//! cannot silently move a key from one view to another. It would not fail
//! loudly either: the losing action has no handler in the tree's dispatch path,
//! so the key would simply stop doing anything.
//!
//! The cost is that a view wanting a key now has to claim it at its own
//! context, inheriting nothing.

use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Backspace, Copy, Cut, Delete, DocEnd, DocStart, Down, End, Home, Left, Newline, Paste, Redo,
    Right, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo, Up,
};
use crate::field::{Cancel, Confirm};
use crate::sidebar::{NewFolder, NewNote, SelectNext, SelectPrev, Trash};
use crate::vault::Save;

pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let editor = Some("EditorView");
    let field = Some("NameField");
    vec![
        KeyBinding::new("backspace", Backspace, editor),
        KeyBinding::new("delete", Delete, editor),
        KeyBinding::new("left", Left, editor),
        KeyBinding::new("right", Right, editor),
        KeyBinding::new("shift-left", SelectLeft, editor),
        KeyBinding::new("shift-right", SelectRight, editor),
        KeyBinding::new("up", Up, editor),
        KeyBinding::new("down", Down, editor),
        KeyBinding::new("shift-up", SelectUp, editor),
        KeyBinding::new("shift-down", SelectDown, editor),
        KeyBinding::new("cmd-a", SelectAll, editor),
        KeyBinding::new("home", Home, editor),
        KeyBinding::new("end", End, editor),
        // macOS text views put the document ends on cmd-up/cmd-down and leave
        // home/end line-relative.
        KeyBinding::new("cmd-up", DocStart, editor),
        KeyBinding::new("cmd-down", DocEnd, editor),
        KeyBinding::new("enter", Newline, editor),
        KeyBinding::new("cmd-c", Copy, editor),
        KeyBinding::new("cmd-x", Cut, editor),
        KeyBinding::new("cmd-v", Paste, editor),
        KeyBinding::new("cmd-z", Undo, editor),
        KeyBinding::new("cmd-shift-z", Redo, editor),
        // The vault tree. `cmd-backspace` is the Finder gesture for the same
        // act, and the arrows move the selection rather than a caret because
        // the editor's own bindings are excluded here — see the note above.
        KeyBinding::new("cmd-backspace", Trash, Some("Sidebar")),
        KeyBinding::new("up", SelectPrev, Some("Sidebar")),
        KeyBinding::new("down", SelectNext, Some("Sidebar")),
        // The name field repeats the editor's movement and deletion set rather
        // than inheriting it, which the paragraph above forces: with
        // `EditorView` absent from the focused stack those bindings are
        // excluded from matching, not merely outranked. A field with only
        // `enter` and `escape` could be typed into and never corrected.
        //
        // `enter` confirms here and inserts a newline there, on one keystroke
        // and two disjoint contexts, so the two never compete.
        KeyBinding::new("enter", Confirm, field),
        KeyBinding::new("escape", Cancel, field),
        KeyBinding::new("backspace", Backspace, field),
        KeyBinding::new("delete", Delete, field),
        KeyBinding::new("left", Left, field),
        KeyBinding::new("right", Right, field),
        KeyBinding::new("shift-left", SelectLeft, field),
        KeyBinding::new("shift-right", SelectRight, field),
        KeyBinding::new("home", Home, field),
        KeyBinding::new("end", End, field),
        KeyBinding::new("cmd-a", SelectAll, field),
        // Scoped to `AppShell` rather than `Sidebar`, so they fire with the
        // editor focused: making a note is a window-wide gesture, not a tree
        // one. `cmd-s` is the precedent.
        KeyBinding::new("cmd-n", NewNote, Some("AppShell")),
        KeyBinding::new("cmd-shift-n", NewFolder, Some("AppShell")),
        // The application's, not a view's: it must fire wherever focus is.
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-s", Save, Some("AppShell")),
    ]
}
