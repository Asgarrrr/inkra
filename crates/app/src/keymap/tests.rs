use std::collections::HashSet;

use gpui::{KeyBindingContextPredicate, KeybindingKeystroke};

use super::key_bindings;
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
