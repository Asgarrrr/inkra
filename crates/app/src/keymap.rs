//! Every key binding in one place, readable without hunting through views.
//! Every binding names a context except `cmd-q` (see CLAUDE.md).

use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Backspace, Copy, Cut, Delete, DocEnd, DocStart, Down, End, FollowLink, Home, Left, LineEnd,
    LineStart, Newline, Paste, Redo, Right, SelectAll, SelectDown, SelectLeft, SelectRight,
    SelectUp, Undo, Up,
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
        KeyBinding::new("cmd-left", Home, editor),
        KeyBinding::new("cmd-right", End, editor),
        KeyBinding::new("ctrl-a", LineStart, editor),
        KeyBinding::new("ctrl-e", LineEnd, editor),
        KeyBinding::new("cmd-up", DocStart, editor),
        KeyBinding::new("cmd-down", DocEnd, editor),
        KeyBinding::new("enter", Newline, editor),
        KeyBinding::new("cmd-enter", FollowLink, editor),
        KeyBinding::new("cmd-c", Copy, editor),
        KeyBinding::new("cmd-x", Cut, editor),
        KeyBinding::new("cmd-v", Paste, editor),
        KeyBinding::new("cmd-z", Undo, editor),
        KeyBinding::new("cmd-shift-z", Redo, editor),
        KeyBinding::new("cmd-backspace", Trash, Some("Sidebar")),
        KeyBinding::new("up", SelectPrev, Some("Sidebar")),
        KeyBinding::new("down", SelectNext, Some("Sidebar")),
        KeyBinding::new("enter", Rename, Some("Sidebar")),
        // NameField repeats the editor's movement/deletion bindings since it
        // can't inherit them; `enter` confirms here, inserts a newline in the editor.
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
        KeyBinding::new("cmd-left", Home, field),
        KeyBinding::new("cmd-right", End, field),
        KeyBinding::new("ctrl-a", Home, field),
        KeyBinding::new("ctrl-e", End, field),
        KeyBinding::new("cmd-a", SelectAll, field),
        // Scoped to `AppShell`, not `Sidebar`, so it fires with the editor
        // focused too — making a note is window-wide, like `cmd-s`.
        KeyBinding::new("cmd-n", NewNote, Some("AppShell")),
        KeyBinding::new("cmd-shift-n", NewFolder, Some("AppShell")),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-s", Save, Some("AppShell")),
    ]
}
