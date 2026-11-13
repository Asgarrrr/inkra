//! `FeatureCx`: the one door a feature gets to the shell, the index and the vault.

use std::any::TypeId;
use std::ops::Range;
use std::rc::Rc;

use gpui::{Action, AppContext as _, Context, Entity, EventEmitter, Window};
use inkra_layout::ViewId;
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, Vault};

use super::NoteShown;
use crate::feature::{FeatureHandle, FeatureId, Slot};
use crate::index::IndexSync;
use crate::open::Open;
use crate::pane::{Pane, PaneEntry};
use crate::picker::{Picker, PickerDelegate};
use crate::shell::AppShell;

/// What a feature reaches while it activates or runs a command.
pub(crate) struct FeatureCx<'a, 'b> {
    pub(super) shell: &'a mut AppShell,
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut Context<'b, AppShell>,
    pub(super) feature: FeatureId,
}

impl FeatureCx<'_, '_> {
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
        let entity = self.cx.new(build);
        self.handle().hold(entity.clone().into_any());
        entity
    }

    pub(crate) fn update<T: 'static, R>(
        &mut self,
        entity: &Entity<T>,
        f: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        entity.update(self.cx, f)
    }

    /// `entity` draws `view`, one of this feature's panes, until the feature switches off.
    pub(crate) fn register_pane<T: Pane>(&mut self, view: ViewId, entity: Entity<T>) {
        debug_assert!(
            self.shell
                .features
                .manifest(self.feature)
                .panes
                .iter()
                .any(|pane| pane.id == view),
            "{view:?} is not a pane of {}",
            self.feature.name()
        );
        let entry = PaneEntry::new(entity);
        if !self.shell.is_visible(view) {
            entry.hidden(self.cx);
        }
        if let Some(panes) = &mut self.shell.panes {
            panes.entries.insert(view, entry);
        }
        // Its dock drew an empty body until now, and a settled vault notifies nothing else.
        self.cx.notify();
    }

    /// `None` with no vault: `expect_used` is denied, so the type says it.
    pub(crate) fn index(&self) -> Option<Entity<IndexSync>> {
        self.shell.panes.as_ref().map(|panes| panes.links.clone())
    }

    pub(crate) fn vault(&self) -> Option<Entity<Vault>> {
        self.shell.panes.as_ref().map(|panes| panes.vault.clone())
    }

    pub(crate) fn shown(&self) -> Option<EntryId> {
        self.shell.showing()
    }

    pub(crate) fn recent(&self) -> Vec<EntryId> {
        self.shell.recent.clone()
    }

    /// Deferred out of the window's lease, so opening the note showing can select `at` and focus it (`show_file`).
    pub(crate) fn open(&mut self, file: EntryId, at: Option<Range<ByteOffset>>, open: Open) {
        let shell = self.cx.weak_entity();
        self.cx.defer(move |cx| {
            shell
                .update(cx, |shell, cx| shell.open(file, at, open, cx))
                .ok();
        });
    }

    /// A modal owned by the feature: switching it off closes the modal. Each `E` reaches `on_pick` while it is on.
    pub(crate) fn open_picker<D, E: 'static>(
        &mut self,
        delegate: impl FnOnce(&mut Context<Picker<D>>) -> D,
        on_pick: impl Fn(&E, &mut FeatureCx) + 'static,
    ) where
        D: PickerDelegate,
        Picker<D>: EventEmitter<E>,
    {
        let Some(picker) = self.shell.toggle_picker(self.window, self.cx, delegate) else {
            return;
        };
        let id = self.feature;
        let subscription = self.cx.subscribe_in(
            &picker,
            self.window,
            move |shell, _, event: &E, window, cx| {
                shell.with_feature(id, window, cx, |fcx| on_pick(event, fcx));
            },
        );
        if let Some(modal) = &mut self.shell.modal {
            modal.hold(subscription);
            modal.owner = Some(id);
        }
    }

    /// Each `E` from `entity` reaches `f` while the feature is on.
    pub(crate) fn subscribe<T: 'static + EventEmitter<E>, E: 'static>(
        &mut self,
        entity: &Entity<T>,
        f: impl Fn(&E, &mut FeatureCx) + 'static,
    ) {
        let id = self.feature;
        let subscription = self.cx.subscribe_in(
            entity,
            self.window,
            move |shell, _, event: &E, window, cx| {
                shell.with_feature(id, window, cx, |fcx| f(event, fcx));
            },
        );
        self.handle().keep(subscription);
    }

    /// Each change of the file shown reaches `f` while the feature is on; `shown()` reads it now.
    pub(crate) fn on_note_shown(&mut self, f: impl Fn(Option<EntryId>, &mut FeatureCx) + 'static) {
        let id = self.feature;
        let shell = self.cx.entity();
        let subscription = self.cx.subscribe_in(
            &shell,
            self.window,
            move |shell, _, event: &NoteShown, window, cx| {
                shell.with_feature(id, window, cx, |fcx| f(event.0, fcx));
            },
        );
        self.handle().keep(subscription);
    }

    fn handle(&mut self) -> &mut FeatureHandle {
        match self.shell.features.slot_mut(self.feature) {
            Slot::Active(handle) => handle,
            Slot::Off | Slot::Dormant => {
                unreachable!("a FeatureCx exists only while its feature is active")
            }
        }
    }
}
