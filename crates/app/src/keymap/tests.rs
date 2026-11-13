use std::collections::HashSet;

use gpui::{KeyBinding, KeyBindingContextPredicate, KeyContext, KeybindingKeystroke};

use super::{core_bindings, key_bindings};
use crate::feature::fakes::CLASH_FAKE;
use crate::feature::{self, BUILTIN};
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

/// Core and feature pairs on equal keys that both fire in the shell with every feature on.
fn conflicts(core: &[KeyBinding], features: &[KeyBinding]) -> usize {
    let mut shell = KeyContext::default();
    shell.add("AppShell");
    for manifest in BUILTIN.iter().chain([&&CLASH_FAKE]) {
        shell.add(manifest.id.name());
    }
    let ctx = [shell];
    let live = |binding: &KeyBinding| {
        binding
            .predicate()
            .is_none_or(|predicate| predicate.eval(&ctx))
    };
    core.iter()
        .filter(|core| live(core))
        .map(|core| {
            features
                .iter()
                .filter(|feature| feature.keystrokes() == core.keystrokes() && live(feature))
                .count()
        })
        .sum()
}

#[test]
fn builtin_feature_keys_clash_with_no_core_key() {
    assert_eq!(conflicts(&core_bindings(), &feature::bindings(BUILTIN)), 0);
}

#[test]
fn a_clashing_fake_is_caught() {
    assert_eq!(
        conflicts(&core_bindings(), &feature::bindings(&[&CLASH_FAKE])),
        1
    );
}
