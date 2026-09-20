use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Backspace, Delete, DocEnd, DocStart, Down, End, Home, Left, Newline, Redo, Right, SelectAll,
    SelectDown, SelectLeft, SelectRight, SelectUp, Undo, Up,
};

pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        // macOS text views put the document ends on cmd-up/cmd-down and leave
        // home/end line-relative.
        KeyBinding::new("cmd-up", DocStart, None),
        KeyBinding::new("cmd-down", DocEnd, None),
        KeyBinding::new("enter", Newline, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]
}
