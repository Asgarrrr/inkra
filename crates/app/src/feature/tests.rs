use gpui::KeyBindingContextPredicate;

use super::fakes::{CATALOG, CLASH_FAKE};
use super::{BUILTIN, Manifest};

fn every_manifest() -> impl Iterator<Item = &'static Manifest> {
    BUILTIN.iter().chain(CATALOG).copied().chain([&CLASH_FAKE])
}

#[test]
fn every_feature_id_is_a_key_context_identifier() {
    for manifest in every_manifest() {
        let name = manifest.id.name();
        assert!(
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{name}"
        );
        let context = format!("AppShell && {name}");
        assert!(
            KeyBindingContextPredicate::parse(&context).is_ok(),
            "{name}"
        );
    }
}

#[test]
fn every_feature_command_is_a_unit_action() {
    for manifest in every_manifest() {
        for decl in manifest.commands {
            let action = (decl.action)();
            assert_eq!(
                std::mem::size_of_val(&*action),
                0,
                "{}: {}",
                manifest.id.name(),
                action.name()
            );
        }
    }
}
