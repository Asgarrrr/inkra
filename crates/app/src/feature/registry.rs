//! Which features are on in a shell, and what each active one holds.

use std::any::TypeId;
use std::rc::Rc;

use gpui::{Action, AnyEntity};

use super::{BUILTIN, FeatureId, Manifest};
use crate::settings::Switches;
use crate::shell::FeatureCx;

pub(crate) type Handler = dyn Fn(&dyn Action, &mut FeatureCx);

pub(crate) struct Registry {
    catalog: &'static [&'static Manifest],
    /// Index-aligned with `catalog`.
    slots: Vec<Slot>,
}

pub(crate) enum Slot {
    Off,
    /// On, not yet activated: its first command activates it.
    Dormant,
    Active(FeatureHandle),
}

/// Dropped when the feature is switched off, and its entities with it.
#[derive(Default)]
pub(crate) struct FeatureHandle {
    pub(crate) handlers: Vec<(TypeId, Rc<Handler>)>,
    pub(crate) entities: Vec<AnyEntity>,
}

fn wanted(manifest: &Manifest, switches: &Switches) -> bool {
    switches
        .get(manifest.id.name())
        .unwrap_or(manifest.default_on)
}

impl Registry {
    pub(crate) fn new(catalog: &'static [&'static Manifest], switches: &Switches) -> Self {
        let slots = catalog
            .iter()
            .map(|manifest| {
                if wanted(manifest, switches) {
                    Slot::Dormant
                } else {
                    Slot::Off
                }
            })
            .collect();
        Self { catalog, slots }
    }

    pub(crate) fn disabled(catalog: &'static [&'static Manifest]) -> Self {
        Self {
            catalog,
            slots: catalog.iter().map(|_| Slot::Off).collect(),
        }
    }

    pub(crate) fn catalog(&self) -> &'static [&'static Manifest] {
        self.catalog
    }

    pub(crate) fn enabled(&self) -> impl Iterator<Item = &'static Manifest> + '_ {
        self.catalog
            .iter()
            .zip(&self.slots)
            .filter(|(_, slot)| !matches!(slot, Slot::Off))
            .map(|(&manifest, _)| manifest)
    }

    /// Whether any slot changed; a feature switched off drops its handle.
    pub(crate) fn apply(&mut self, switches: &Switches) -> bool {
        let mut changed = false;
        for (manifest, slot) in self.catalog.iter().zip(&mut self.slots) {
            let wanted = wanted(manifest, switches);
            if wanted == matches!(slot, Slot::Off) {
                *slot = if wanted { Slot::Dormant } else { Slot::Off };
                changed = true;
            }
        }
        changed
    }

    /// The manifest and slot of `id`; `None` for an id outside the catalog.
    pub(crate) fn get_mut(&mut self, id: FeatureId) -> Option<(&'static Manifest, &mut Slot)> {
        self.catalog
            .iter()
            .zip(&mut self.slots)
            .find(|(manifest, _)| manifest.id == id)
            .map(|(&manifest, slot)| (manifest, slot))
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(BUILTIN, &Switches::default())
    }
}
