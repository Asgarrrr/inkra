//! Every key binding in one place, readable without hunting through views.
//! Every binding names a context except `cmd-q` (see CLAUDE.md).

use gpui::KeyBinding;

use crate::Quit;
use crate::command_palette;
use crate::editor::actions::{
    Copy, Cut, DocEnd, DocStart, Down, FollowLink, FollowLinkInNewTab, FollowLinkInSplit, Newline,
    Paste, Redo, SelectDown, SelectUp, Undo, Up,
};
use crate::feature;
use crate::field::{Cancel, Confirm};
use crate::picker;
use crate::shell::{
    ActivateLastTab, ActivatePaneDown, ActivatePaneLeft, ActivatePaneRight, ActivatePaneUp,
    ActivateTab, CloseTab, GoBack, GoForward, NewTab, NextTab, OpenVault, PreviousTab,
    ReopenClosedTab, SplitDown, SplitLeft, SplitRight, SplitUp, ToggleLeftDock, ToggleRightDock,
};
use crate::sidebar::{NewFolder, NewNote, Rename, SelectNext, SelectPrev, Trash};
use crate::text_input;
use crate::text_input::actions::{
    Backspace, Delete, End, Home, Left, LineEnd, LineStart, Right, SelectAll, SelectLeft,
    SelectRight,
};
use crate::vault::Save;
use crate::vault_search::SearchVault;

pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let mut bindings = core_bindings();
    bindings.extend(feature::bindings(feature::BUILTIN));
    bindings
}

fn core_bindings() -> Vec<KeyBinding> {
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
        // `enter` keeps shift on macOS, unlike a character key (gpui_macos `events.rs`).
        KeyBinding::new("cmd-shift-enter", FollowLinkInNewTab, editor),
        KeyBinding::new("cmd-alt-enter", FollowLinkInSplit, editor),
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
        // The field binds no `cmd-enter`, so this one fires; the editor's is out of reach.
        KeyBinding::new("cmd-enter", picker::SecondaryConfirm, Some("Picker")),
        // The menu reuses the picker's and the field's actions, so the palette lists nothing new.
        KeyBinding::new("up", picker::SelectPrev, Some("ContextMenu")),
        KeyBinding::new("down", picker::SelectNext, Some("ContextMenu")),
        KeyBinding::new("enter", Confirm, Some("ContextMenu")),
        KeyBinding::new("escape", Cancel, Some("ContextMenu")),
        // `enter` confirms here and inserts a newline in the editor.
        KeyBinding::new("enter", Confirm, field),
        KeyBinding::new("escape", Cancel, field),
        // Scoped to `AppShell`, not `Sidebar`, so it fires with the editor
        // focused too — making a note is window-wide, like `cmd-s`.
        KeyBinding::new("cmd-n", NewNote, Some("AppShell")),
        KeyBinding::new("cmd-shift-n", NewFolder, Some("AppShell")),
        KeyBinding::new("cmd-o", OpenVault, Some("AppShell")),
        KeyBinding::new("cmd-shift-f", SearchVault, Some("AppShell")),
        KeyBinding::new("cmd-shift-p", command_palette::Toggle, Some("AppShell")),
        KeyBinding::new("cmd-t", NewTab, Some("AppShell")),
        KeyBinding::new("cmd-shift-t", ReopenClosedTab, Some("AppShell")),
        KeyBinding::new("cmd-w", CloseTab, Some("AppShell")),
        KeyBinding::new("ctrl-tab", NextTab, Some("AppShell")),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some("AppShell")),
        // ⌘⇧] as macOS reports it (gpui_macos events.rs:470-479).
        KeyBinding::new("cmd-}", NextTab, Some("AppShell")),
        KeyBinding::new("cmd-{", PreviousTab, Some("AppShell")),
        // `[` is no letter, so macOS keeps it with `cmd` (gpui_macos events.rs:440-481).
        KeyBinding::new("cmd-[", GoBack, Some("AppShell")),
        KeyBinding::new("cmd-]", GoForward, Some("AppShell")),
        KeyBinding::new("cmd-1", ActivateTab(0), Some("AppShell")),
        KeyBinding::new("cmd-2", ActivateTab(1), Some("AppShell")),
        KeyBinding::new("cmd-3", ActivateTab(2), Some("AppShell")),
        KeyBinding::new("cmd-4", ActivateTab(3), Some("AppShell")),
        KeyBinding::new("cmd-5", ActivateTab(4), Some("AppShell")),
        KeyBinding::new("cmd-6", ActivateTab(5), Some("AppShell")),
        KeyBinding::new("cmd-7", ActivateTab(6), Some("AppShell")),
        KeyBinding::new("cmd-8", ActivateTab(7), Some("AppShell")),
        // The last tab, not the ninth, as in Zed.
        KeyBinding::new("cmd-9", ActivateLastTab, Some("AppShell")),
        KeyBinding::new("cmd-\\", ToggleLeftDock, Some("AppShell")),
        // ⌘⇧\ as macOS reports it, as `cmd-}` above.
        KeyBinding::new("cmd-|", ToggleRightDock, Some("AppShell")),
        KeyBinding::new("cmd-k left", SplitLeft, Some("AppShell")),
        KeyBinding::new("cmd-k right", SplitRight, Some("AppShell")),
        KeyBinding::new("cmd-k up", SplitUp, Some("AppShell")),
        KeyBinding::new("cmd-k down", SplitDown, Some("AppShell")),
        KeyBinding::new("cmd-k cmd-left", ActivatePaneLeft, Some("AppShell")),
        KeyBinding::new("cmd-k cmd-right", ActivatePaneRight, Some("AppShell")),
        KeyBinding::new("cmd-k cmd-up", ActivatePaneUp, Some("AppShell")),
        KeyBinding::new("cmd-k cmd-down", ActivatePaneDown, Some("AppShell")),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-s", Save, Some("AppShell")),
    ]
}

#[cfg(test)]
mod tests;
