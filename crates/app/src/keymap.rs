//! Every key binding in one place, so the set can be read without hunting
//! through the views that handle them.
//!
//! Every binding names a context except `cmd-q` (see CLAUDE.md, "Every
//! binding names a context" and "A binding fires where its handler can be
//! reached"). A predicate-less binding and one naming the focused frame tie
//! in score (`gpui/src/keymap.rs:246-252`, `keymap/context.rs:260-268`); ties
//! break on position in the `Vec` below (`keymap.rs:188-190`).

use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Backspace, Copy, Cut, Delete, DocEnd, DocStart, Down, End, Home, Left, Newline, Paste, Redo,
    Right, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp, Undo, Up,
};
use crate::field::{Cancel, Confirm};
use crate::sidebar::{NewFolder, NewNote, Rename, SelectNext, SelectPrev, Trash};
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
        // The Finder gesture. `enter` also inserts a newline in the editor and
        // confirms in a field; all three name a context, so none competes.
        KeyBinding::new("enter", Rename, Some("Sidebar")),
        // NameField repeats the editor's movement/deletion bindings since it
        // can't inherit them (see module doc); otherwise it could be typed
        // into and never corrected.
        //
        // `enter` confirms here, inserts a newline in the editor: disjoint
        // contexts, no competition.
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
