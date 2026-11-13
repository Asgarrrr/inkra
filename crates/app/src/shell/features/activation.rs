//! `Activation`: what a feature wires up once, while it activates.

use std::any::TypeId;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use gpui::{Action, AppContext as _, Context, Entity, EventEmitter};
use inkra_layout::ViewId;
use inkra_vault::EntryId;

use super::{FeatureCx, NoteShown};
use crate::feature::{FeatureHandle, Slot};
use crate::pane::{Pane, PaneEntry};

/// Commands, owned entities, subscriptions and panes are registered here and nowhere else, so all
/// of them land in the handle that switching the feature off drops. Derefs to `FeatureCx`, as
/// gpui's `Context` derefs to `App`.
pub(crate) struct Activation<'a, 'b>(pub(super) FeatureCx<'a, 'b>);

impl<'a, 'b> Deref for Activation<'a, 'b> {
    type Target = FeatureCx<'a, 'b>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Activation<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Activation<'_, '_> {
    pub(crate) fn on_command<A: Action>(&mut self, f: impl Fn(&A, &mut FeatureCx) + 'static) {
        let handler = move |action: &dyn Action, fcx: &mut FeatureCx| {
            if let Some(action) = action.as_any().downcast_ref::<A>() {
                f(action, fcx);
            }
        };
        self.handle().on(TypeId::of::<A>(), Rc::new(handler));
    }

    /// Held by the feature: switching it off releases the entity.
    #[expect(
        clippy::new_ret_no_self,
        clippy::wrong_self_convention,
        reason = "named after gpui's cx.new, which it wraps"
    )]
    pub(crate) fn new<T: 'static>(
        &mut self,
        build: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        let entity = self.0.cx.new(build);
        self.handle().hold(entity.clone().into_any());
        entity
    }

    /// `entity` draws `view`, one of this feature's panes, until the feature switches off.
    pub(crate) fn register_pane<T: Pane>(&mut self, view: ViewId, entity: Entity<T>) {
        debug_assert!(
            self.0
                .shell
                .features
                .manifest(self.0.feature)
                .panes
                .iter()
                .any(|pane| pane.id == view),
            "{view:?} is not a pane of {}",
            self.0.feature.name()
        );
        let entry = PaneEntry::new(entity);
        if !self.0.shell.is_visible(view) {
            entry.hidden(self.0.cx);
        }
        if let Some(panes) = &mut self.0.shell.panes {
            panes.entries.insert(view, entry);
        }
        // Its dock drew an empty body until now, and a settled vault notifies nothing else.
        self.0.cx.notify();
    }

    /// Each `E` from `entity` reaches `f` while the feature is on.
    pub(crate) fn subscribe<T: 'static + EventEmitter<E>, E: 'static>(
        &mut self,
        entity: &Entity<T>,
        f: impl Fn(&E, &mut FeatureCx) + 'static,
    ) {
        let id = self.0.feature;
        let subscription = self.0.cx.subscribe_in(
            entity,
            self.0.window,
            move |shell, _, event: &E, window, cx| {
                shell.with_feature(id, window, cx, |mut fcx| f(event, &mut fcx));
            },
        );
        self.handle().keep(subscription);
    }

    /// Each change of the file shown reaches `f` while the feature is on; `shown()` reads it now.
    pub(crate) fn on_note_shown(&mut self, f: impl Fn(Option<EntryId>, &mut FeatureCx) + 'static) {
        let id = self.0.feature;
        let shell = self.0.cx.entity();
        let subscription = self.0.cx.subscribe_in(
            &shell,
            self.0.window,
            move |shell, _, event: &NoteShown, window, cx| {
                shell.with_feature(id, window, cx, |mut fcx| f(event.0, &mut fcx));
            },
        );
        self.handle().keep(subscription);
    }

    fn handle(&mut self) -> &mut FeatureHandle {
        match self.0.shell.features.slot_mut(self.0.feature) {
            Slot::Active(handle) => handle,
            Slot::Off | Slot::Dormant => {
                unreachable!("an Activation exists only while its feature is active")
            }
        }
    }
}
