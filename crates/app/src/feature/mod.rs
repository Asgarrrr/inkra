//! What a feature declares, and the bindings its commands get (feature registry spec).

use gpui::{Action, KeyBinding};

use crate::pane::PaneDecl;
use crate::shell::FeatureCx;
use crate::{backlinks, quick_open};

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
    /// Activates once the shell's first frame is drawn, rather than on its first command.
    pub(crate) after_first_frame: bool,
    pub(crate) commands: &'static [CommandDecl],
    /// Docked like the core panes; the feature registers each one's view when it activates.
    pub(crate) panes: &'static [&'static PaneDecl],
    pub(crate) activate: fn(&mut FeatureCx),
}

pub(crate) struct CommandDecl {
    keys: &'static [&'static str],
    pub(crate) action: fn() -> Box<dyn Action>,
    bind: fn(&'static str, Option<&str>) -> KeyBinding,
}

impl CommandDecl {
    /// `A` is a unit action: `on_boxed_action` hands the listener the registered clone, not the dispatched one.
    pub(crate) const fn new<A: Action + Default>(keys: &'static [&'static str]) -> Self {
        Self {
            keys,
            action: crate::pane::boxed::<A>,
            bind: bind::<A>,
        }
    }
}

// The pinned gpui's `KeyBinding::new` takes `&'static str` keystrokes.
fn bind<A: Action + Default>(keys: &'static str, context: Option<&str>) -> KeyBinding {
    KeyBinding::new(keys, A::default(), context)
}

pub(crate) static BUILTIN: &[&Manifest] = &[&quick_open::MANIFEST, &backlinks::MANIFEST];

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
