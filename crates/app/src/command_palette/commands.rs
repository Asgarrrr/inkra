use gpui::{Action, App, FocusHandle, KeybindingKeystroke, SharedString, Window};

use crate::editor::actions as editor;
use crate::field;
use crate::picker;
use crate::sidebar;
use crate::text_input::actions as text_input;

/// Moves, deletions and field keys: a palette entry for one would do nothing a user wants.
pub(super) fn is_hidden(name: &str) -> bool {
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
        field::Confirm::name_for_type(),
        picker::SecondaryConfirm::name_for_type(),
        field::Cancel::name_for_type(),
        super::Toggle::name_for_type(),
    ]
    .contains(&name)
}

pub(crate) struct Command {
    pub(super) name: SharedString,
    /// Run from the palette before: keeps its place above every other match.
    pub(super) recent: bool,
    pub(super) key: Option<SharedString>,
    pub(super) action: Box<dyn Action>,
}

/// `vault_tree::NewNote` → `New note`: the last segment, a space before each word's capital.
pub(super) fn humanize(name: &str) -> String {
    let base = name.rsplit_once("::").map_or(name, |(_, base)| base);
    let mut out =
        String::with_capacity(base.len() + base.chars().filter(char::is_ascii_uppercase).count());
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

/// A symbol macOS reports with shift cleared (`gpui_macos/src/events.rs:467-485`), and the key pressed for it (US layout).
const SHIFTED: &[(&str, &str)] = &[("|", "\\"), ("}", "]"), ("{", "[")];

/// Apple order, `⌃⌥⇧⌘`, then the key's glyph (spec D5); gpui's `Display` orders `⌘⇧` and prints `enter`.
pub(super) fn key_label(keystroke: &KeybindingKeystroke) -> String {
    let modifiers = keystroke.modifiers();
    let shifted = SHIFTED
        .iter()
        .find(|(symbol, _)| *symbol == keystroke.key())
        .map(|(_, key)| *key);
    let mut out = String::new();
    for (on, glyph) in [
        (modifiers.control, '⌃'),
        (modifiers.alt, '⌥'),
        (modifiers.shift || shifted.is_some(), '⇧'),
        (modifiers.platform, '⌘'),
    ] {
        if on {
            out.push(glyph);
        }
    }
    match shifted.unwrap_or(keystroke.key()) {
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

/// A chord's keystrokes, each drawn by `key_label`, joined with a space.
pub(crate) fn keys_label(keystrokes: &[KeybindingKeystroke]) -> SharedString {
    keystrokes
        .iter()
        .map(key_label)
        .collect::<Vec<_>>()
        .join(" ")
        .into()
}

/// The commands at `target`, each with its key there: `recent` first, newest first, then by name.
/// A query moves the rest whose match starts later than their first word down (`update_matches`).
pub(crate) fn commands(
    window: &Window,
    cx: &App,
    target: &FocusHandle,
    recent: &[&str],
) -> Vec<Command> {
    let mut commands: Vec<Command> = window
        .available_actions(cx)
        .into_iter()
        .filter(|action| !is_hidden(action.name()))
        .map(|action| {
            let key = window
                .highest_precedence_binding_for_action_in(&*action, target)
                .map(|binding| keys_label(binding.keystrokes()));
            Command {
                name: humanize(action.name()).into(),
                recent: recent.contains(&action.name()),
                key,
                action,
            }
        })
        .collect();
    commands.sort_by_cached_key(|command| {
        let rank = recent
            .iter()
            .position(|&name| name == command.action.name())
            .unwrap_or(usize::MAX);
        (rank, command.name.clone())
    });
    commands
}
