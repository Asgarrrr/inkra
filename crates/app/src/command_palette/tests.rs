#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use gpui::{KeybindingKeystroke, Keystroke};

use super::commands::{humanize, is_hidden, key_label, keys_label};

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
            .filter(|name| !is_hidden(name))
            .map(|name| humanize(name))
            .collect();
        let unique: HashSet<&String> = shown.iter().collect();
        assert_eq!(unique.len(), shown.len(), "{shown:?}");
    });
}

fn keystroke(source: &str) -> KeybindingKeystroke {
    KeybindingKeystroke::from_keystroke(Keystroke::parse(source).expect(source))
}

fn label(source: &str) -> String {
    key_label(&keystroke(source))
}

#[test]
fn keys_draw_in_apple_order_with_glyphs() {
    for (source, want) in [
        ("cmd-shift-n", "⇧⌘N"),
        ("cmd-enter", "⌘↩"),
        ("enter", "↩"),
        ("cmd-backspace", "⌘⌫"),
        ("ctrl-alt-a", "⌃⌥A"),
        ("ctrl-alt-shift-cmd-a", "⌃⌥⇧⌘A"),
        ("delete", "⌦"),
        ("escape", "⎋"),
        ("tab", "⇥"),
        ("left", "←"),
        ("right", "→"),
        ("up", "↑"),
        ("down", "↓"),
        ("space", "Space"),
        ("f1", "F1"),
    ] {
        assert_eq!(label(source), want, "{source}");
    }
}

/// macOS reports ⌘⇧\ as `cmd-|`; the label names the keys pressed, in Apple order.
#[test]
fn a_shifted_symbol_draws_as_shift_and_its_key() {
    for (source, want) in [("cmd-|", "⇧⌘\\"), ("cmd-}", "⇧⌘]"), ("cmd-{", "⇧⌘[")] {
        assert_eq!(label(source), want, "{source}");
    }
}

#[test]
fn a_chord_joins_its_keystrokes_with_a_space() {
    let chord = [keystroke("cmd-k"), keystroke("cmd-s")];
    assert_eq!(keys_label(&chord), "⌘K ⌘S");
}

/// A key gpui names in words (`pageup`, `home`) would draw as a word beside the glyphs.
#[test]
fn every_listed_key_draws_as_glyphs_and_capitals() {
    for binding in crate::keymap::key_bindings() {
        if is_hidden(binding.action().name()) {
            continue;
        }
        for keystroke in binding.keystrokes() {
            let label = key_label(keystroke);
            assert!(
                !label.chars().any(|c| c.is_ascii_lowercase()),
                "{} draws as {label}",
                binding.action().name()
            );
        }
    }
}
