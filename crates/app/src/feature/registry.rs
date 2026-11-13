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
    /// The shell's first frame is drawn: an `after_first_frame` feature switched on activates at once.
    launched: bool,
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
    handlers: Vec<(TypeId, Rc<Handler>)>,
    entities: Vec<AnyEntity>,
}

impl FeatureHandle {
    pub(crate) fn on(&mut self, action: TypeId, handler: Rc<Handler>) {
        self.handlers.push((action, handler));
    }

    pub(crate) fn hold(&mut self, entity: AnyEntity) {
        self.entities.push(entity);
    }
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
        Self {
            catalog,
            slots,
            launched: false,
        }
    }

    pub(crate) fn disabled(catalog: &'static [&'static Manifest]) -> Self {
        Self {
            catalog,
            slots: catalog.iter().map(|_| Slot::Off).collect(),
            launched: false,
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

    pub(crate) fn launch(&mut self) {
        self.launched = true;
    }

    /// The dormant `after_first_frame` features, once launched; none before.
    pub(crate) fn due(&self) -> Vec<FeatureId> {
        if !self.launched {
            return Vec::new();
        }
        self.catalog
            .iter()
            .zip(&self.slots)
            .filter(|(manifest, slot)| manifest.after_first_frame && matches!(slot, Slot::Dormant))
            .map(|(manifest, _)| manifest.id)
            .collect()
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

    pub(crate) fn manifest(&self, id: FeatureId) -> &'static Manifest {
        self.catalog[self.index(id)]
    }

    pub(crate) fn slot(&self, id: FeatureId) -> &Slot {
        &self.slots[self.index(id)]
    }

    pub(crate) fn slot_mut(&mut self, id: FeatureId) -> &mut Slot {
        let index = self.index(id);
        &mut self.slots[index]
    }

    /// `None` unless the feature is active and handles `action`.
    pub(crate) fn handler(&self, id: FeatureId, action: TypeId) -> Option<Rc<Handler>> {
        let Slot::Active(handle) = self.slot(id) else {
            return None;
        };
        handle
            .handlers
            .iter()
            .find(|(type_id, _)| *type_id == action)
            .map(|(_, handler)| handler.clone())
    }

    fn index(&self, id: FeatureId) -> usize {
        let Some(index) = self.catalog.iter().position(|manifest| manifest.id == id) else {
            panic!("{} is not in this registry's catalog", id.name());
        };
        index
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new(BUILTIN, &Switches::default())
    }
}
