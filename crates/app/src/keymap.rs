//! Every key binding in one place, so the set can be read without hunting
//! through the views that handle them.
//!
//! A binding's key context decides where it fires. Window-wide actions bind at
//! `AppShell`, not `EditorView`: the editor's context only fires while the
//! editor has focus. `⌘S` is the precedent.
//!
//! An editing binding names `EditorView` rather than leaving the context open,
//! and that is load-bearing now that a second view edits text. A binding with
//! no predicate scores `contexts.len()` (`gpui/src/keymap.rs:246-252`), and a
//! predicate naming the *focused* frame scores the same
//! (`keymap/context.rs:260-268`), so the two **tie** and the winner is whichever
//! was pushed later into the `Vec` below (`keymap.rs:188-190`). Naming the
//! context removes the tie instead of betting on it: with `EditorView` absent
//! from the focused stack, the predicate fails and the binding is excluded from
//! matching rather than merely outranked.
//!
//! The consequence is that a view wanting a key must claim it at its own
//! context. `NameField` therefore repeats the movement and deletion set below
//! rather than inheriting it — see its own bindings.

use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Backspace, Copy, Cut, Delete, DocEnd, DocStart, Down, End, Home, Left, Newline, Paste, Redo,
    Right, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo, Up,
};
use crate::vault::Save;

pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let editor = Some("EditorView");
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
        // The application's, not a view's: it must fire wherever focus is.
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-s", Save, Some("AppShell")),
    ]
}
