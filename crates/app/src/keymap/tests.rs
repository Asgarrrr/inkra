use std::collections::HashSet;

use gpui::{KeyBinding, KeyBindingContextPredicate, KeyContext, KeybindingKeystroke};

use super::{core_bindings, key_bindings};
use crate::feature::fakes::{CHORD_FAKE, CLASH_FAKE, PING_FAKE, TWIN_FAKE, UNDO_FAKE};
use crate::feature::{self, BUILTIN, Manifest};
use crate::text_input::{CONTEXT, HOST_CONTEXTS};

fn keystrokes_in(context: &str) -> HashSet<Vec<KeybindingKeystroke>> {
    let predicate = KeyBindingContextPredicate::parse(context).unwrap();
    key_bindings()
        .iter()
        .filter(|binding| binding.predicate().as_deref() == Some(&predicate))
        .map(|binding| binding.keystrokes().to_vec())
        .collect()
}

/// Both would match on the host's element, so the pair would dispatch by
/// binding order: a duplicate is drift waiting to happen.
#[test]
fn no_key_is_bound_both_shared_and_per_host() {
    let shared = keystrokes_in(CONTEXT);
    assert!(!shared.is_empty(), "no key binds in `{CONTEXT}`");

    for host in HOST_CONTEXTS {
        let twice: Vec<_> = keystrokes_in(host).intersection(&shared).cloned().collect();
        assert!(twice.is_empty(), "{host} rebinds {twice:?}");
    }
}

/// Each focus a key can reach: the shell alone, then a host, a list or a menu under it.
fn focus_stacks(features: &[&Manifest]) -> Vec<Vec<KeyContext>> {
    let mut shell = KeyContext::default();
    shell.add("AppShell");
    for manifest in features {
        shell.add(manifest.id.name());
    }
    let below = [
        vec!["EditorView TextInput"],
        vec!["NameField TextInput"],
        vec!["Sidebar"],
        vec!["Picker", "NameField TextInput"],
        vec!["ContextMenu"],
    ];
    let mut stacks = vec![vec![shell.clone()]];
    for contexts in below {
        let mut stack = vec![shell.clone()];
        stack.extend(contexts.into_iter().map(|c| KeyContext::parse(c).unwrap()));
        stacks.push(stack);
    }
    stacks
}

/// Pairs that can both fire in one focus on keys that are equal or one a chord prefix of the other.
/// `features` is the list as bound, before any deduplication.
fn conflicts(core: &[KeyBinding], features: &[KeyBinding], known: &[&Manifest]) -> usize {
    let stacks = focus_stacks(known);
    let live = |binding: &KeyBinding, stack: &[KeyContext]| {
        binding
            .predicate()
            .is_none_or(|predicate| predicate.depth_of(stack).is_some())
    };
    let clash = |a: &KeyBinding, b: &KeyBinding| {
        let (a_keys, b_keys) = (a.keystrokes(), b.keystrokes());
        (a_keys.starts_with(b_keys) || b_keys.starts_with(a_keys))
            && stacks.iter().any(|stack| live(a, stack) && live(b, stack))
    };
    let with_core = core
        .iter()
        .map(|core| {
            features
                .iter()
                .filter(|feature| clash(core, feature))
                .count()
        })
        .sum::<usize>();
    let among_features = features
        .iter()
        .enumerate()
        .map(|(ix, a)| features[ix + 1..].iter().filter(|b| clash(a, b)).count())
        .sum::<usize>();
    with_core + among_features
}

fn known() -> Vec<&'static Manifest> {
    let fakes = [&PING_FAKE, &CLASH_FAKE, &UNDO_FAKE, &CHORD_FAKE, &TWIN_FAKE];
    BUILTIN.iter().copied().chain(fakes).collect()
}

fn fake_conflicts(manifests: &[&Manifest]) -> usize {
    conflicts(&core_bindings(), &feature::bindings(manifests), &known())
}

#[test]
fn builtin_feature_keys_clash_with_no_core_key() {
    assert_eq!(fake_conflicts(BUILTIN), 0);
}

#[test]
fn a_clashing_fake_is_caught() {
    assert_eq!(fake_conflicts(&[&CLASH_FAKE]), 1);
}

#[test]
fn a_key_under_a_deeper_core_binding_is_caught() {
    assert_eq!(fake_conflicts(&[&UNDO_FAKE]), 1);
}

#[test]
fn a_chord_prefix_is_caught() {
    assert!(fake_conflicts(&[&CHORD_FAKE]) >= 1);
}

#[test]
fn two_features_on_one_key_are_caught() {
    assert_eq!(fake_conflicts(&[&PING_FAKE, &TWIN_FAKE]), 1);
}
