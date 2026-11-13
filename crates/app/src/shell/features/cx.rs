//! `FeatureCx`: what a feature reaches once active, in a command, a pick or an event.

use std::ops::Range;

use gpui::{Context, Entity, EventEmitter, Window};
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, Vault};

use crate::feature::FeatureId;
use crate::index::IndexSync;
use crate::open::Open;
use crate::picker::{Picker, PickerDelegate};
use crate::shell::AppShell;

/// Reads and acts; nothing here adds to the feature's handle.
pub(crate) struct FeatureCx<'a, 'b> {
    pub(super) shell: &'a mut AppShell,
    pub(super) window: &'a mut Window,
    pub(super) cx: &'a mut Context<'b, AppShell>,
    pub(super) feature: FeatureId,
}

impl FeatureCx<'_, '_> {
    pub(crate) fn update<T: 'static, R>(
        &mut self,
        entity: &Entity<T>,
        f: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        entity.update(self.cx, f)
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
                shell.with_feature(id, window, cx, |mut fcx| on_pick(event, &mut fcx));
            },
        );
        if let Some(modal) = &mut self.shell.modal {
            modal.hold(subscription);
            modal.owner = Some(id);
        }
    }
}
