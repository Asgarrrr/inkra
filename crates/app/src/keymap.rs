//! Every key binding in one place, readable without hunting through views.
//! Every binding names a context except `cmd-q` (see CLAUDE.md).

use gpui::KeyBinding;

use crate::Quit;
use crate::editor::actions::{
    Copy, Cut, DocEnd, DocStart, Down, FollowLink, Newline, Paste, Redo, SelectDown, SelectUp,
    Undo, Up,
};
use crate::field::{Cancel, Confirm};
use crate::picker;
use crate::quick_open::QuickOpen;
use crate::sidebar::{NewFolder, NewNote, Rename, SelectNext, SelectPrev, Trash};
use crate::text_input;
use crate::text_input::actions::{
    Backspace, Delete, End, Home, Left, LineEnd, LineStart, Right, SelectAll, SelectLeft,
    SelectRight,
};
use crate::vault::Save;
use crate::vault_search::SearchVault;

pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let input = Some(text_input::CONTEXT);
    let editor = Some(text_input::EDITOR_CONTEXT);
    let field = Some(text_input::FIELD_CONTEXT);
    vec![
        // Once, for every text input. `home` and `cmd-left` stop at the row's edge,
        // `ctrl-a` at the logical line's, as in Zed and Cocoa.
        KeyBinding::new("backspace", Backspace, input),
        KeyBinding::new("delete", Delete, input),
        KeyBinding::new("left", Left, input),
        KeyBinding::new("right", Right, input),
        KeyBinding::new("shift-left", SelectLeft, input),
        KeyBinding::new("shift-right", SelectRight, input),
        KeyBinding::new("cmd-a", SelectAll, input),
        KeyBinding::new("home", Home, input),
        KeyBinding::new("end", End, input),
        KeyBinding::new("cmd-left", Home, input),
        KeyBinding::new("cmd-right", End, input),
        KeyBinding::new("ctrl-a", LineStart, input),
        KeyBinding::new("ctrl-e", LineEnd, input),
        KeyBinding::new("up", Up, editor),
        KeyBinding::new("down", Down, editor),
        KeyBinding::new("shift-up", SelectUp, editor),
        KeyBinding::new("shift-down", SelectDown, editor),
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
        // `enter` and `escape` stay the field's: the deepest context wins, so a `Picker` binding would never fire.
        KeyBinding::new("up", picker::SelectPrev, Some("Picker")),
        KeyBinding::new("down", picker::SelectNext, Some("Picker")),
        // `enter` confirms here and inserts a newline in the editor.
        KeyBinding::new("enter", Confirm, field),
        KeyBinding::new("escape", Cancel, field),
        // Scoped to `AppShell`, not `Sidebar`, so it fires with the editor
        // focused too — making a note is window-wide, like `cmd-s`.
        KeyBinding::new("cmd-n", NewNote, Some("AppShell")),
        KeyBinding::new("cmd-shift-n", NewFolder, Some("AppShell")),
        KeyBinding::new("cmd-p", QuickOpen, Some("AppShell")),
        KeyBinding::new("cmd-shift-f", SearchVault, Some("AppShell")),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-s", Save, Some("AppShell")),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
