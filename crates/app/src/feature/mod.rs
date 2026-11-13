//! What a feature declares, and the bindings its commands get (feature registry spec).

use gpui::{Action, KeyBinding};

use crate::shell::FeatureCx;

#[cfg(test)]
pub(crate) mod fakes;
mod registry;
#[cfg(test)]
mod tests;

pub(crate) use registry::{FeatureHandle, Registry, Slot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FeatureId(&'static str);

impl FeatureId {
    /// `name` matches `[a-z_]+`: it is also the feature's key context.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "BUILTIN is empty until quick open joins it")
    )]
    pub(crate) const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub(crate) const fn name(self) -> &'static str {
        self.0
    }
}

pub(crate) struct Manifest {
    pub(crate) id: FeatureId,
    pub(crate) default_on: bool,
    pub(crate) commands: &'static [CommandDecl],
    pub(crate) activate: fn(&mut FeatureCx),
}

pub(crate) struct CommandDecl {
    keys: &'static [&'static str],
    pub(crate) action: fn() -> Box<dyn Action>,
    bind: fn(&'static str, Option<&str>) -> KeyBinding,
}

impl CommandDecl {
    /// `A` is a unit action: `on_boxed_action` hands the listener the registered clone, not the dispatched one.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "BUILTIN is empty until quick open joins it")
    )]
    pub(crate) const fn new<A: Action + Default>(keys: &'static [&'static str]) -> Self {
        Self {
            keys,
            action: crate::pane::boxed::<A>,
            bind: bind::<A>,
        }
    }
}

fn bind<A: Action + Default>(keys: &'static str, context: Option<&str>) -> KeyBinding {
    KeyBinding::new(keys, A::default(), context)
}

pub(crate) static BUILTIN: &[&Manifest] = &[];

/// Each feature's keys, live only while the shell's key context names the feature.
pub(crate) fn bindings(catalog: &[&Manifest]) -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    for manifest in catalog {
        let context = format!("AppShell && {}", manifest.id.name());
        for decl in manifest.commands {
            for &keys in decl.keys {
                bindings.push((decl.bind)(keys, Some(&context)));
            }
        }
    }
    bindings
}
