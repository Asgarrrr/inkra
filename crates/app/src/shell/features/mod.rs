//! The shell's side of the feature registry: launching features, running their commands, and switching them.

use std::time::{Duration, Instant};

use gpui::{Action, Context, Entity, EventEmitter, KeyContext, Window};
use inkra_layout::ViewId;
use inkra_vault::EntryId;

use super::AppShell;
use super::group::Group;
use super::parked::Parked;
use crate::feature::{FeatureHandle, FeatureId, Registry, Slot};
use crate::pane::PaneDecl;
use crate::settings::Switches;
use crate::timeline::{Step, Timeline};
use crate::vault::{ConfigChanged, NoteStore};

mod cx;

pub(crate) use cx::FeatureCx;

/// A feature's activation or call past this logs a line (spec D11).
pub(super) fn over_budget(took: Duration) -> bool {
    took > Duration::from_millis(2)
}

/// The file `last_center` shows, emitted on each `follow_center`.
pub(crate) struct NoteShown(pub(crate) Option<EntryId>);

impl EventEmitter<NoteShown> for AppShell {}

impl AppShell {
    /// A task cannot run before the turn that opens the window ends, so it lands after the first draw.
    pub(super) fn start_launch(window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |shell, cx| {
            shell.update_in(cx, Self::launch_features).ok();
        })
        .detach();
    }

    fn launch_features(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        Timeline::mark(Step::Registry, cx);
        self.features.launch();
        for view in self.visible_views() {
            self.wake_pane(view, window, cx);
        }
        self.activate_due(window, cx);
    }

    /// Activates the dormant feature whose pane `view` is, once that pane shows (spec D6).
    pub(super) fn wake_pane(&mut self, view: ViewId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.features.dormant_owner(view) {
            self.activate(id, window, cx);
        }
    }

    fn activate_due(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for id in self.features.due() {
            self.activate(id, window, cx);
        }
    }

    pub(super) fn watch_config(
        notes: &Entity<NoteStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            notes,
            window,
            |shell, _notes, event: &ConfigChanged, window, cx| {
                shell.config_changed(&event.0, window, cx);
            },
        )
        .detach();
    }

    pub(super) fn run_command(
        &mut self,
        id: FeatureId,
        action: &dyn Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.features.slot(id) {
            // A palette opened before the switch went off.
            Slot::Off => return,
            Slot::Dormant => self.activate(id, window, cx),
            Slot::Active(_) => {}
        }
        let Some(handler) = self.features.handler(id, action.as_any().type_id()) else {
            log::warn!("feature {} has no handler for {}", id.name(), action.name());
            return;
        };
        self.with_feature(id, window, cx, |fcx| handler(action, fcx));
    }

    fn activate(&mut self, id: FeatureId, window: &mut Window, cx: &mut Context<Self>) {
        // Active before `activate` runs, so `FeatureCx` has a handle to write into.
        *self.features.slot_mut(id) = Slot::Active(FeatureHandle::default());
        let activate = self.features.manifest(id).activate;
        self.with_feature(id, window, cx, activate);
        Timeline::mark(Step::Activated(id.name()), cx);
    }

    fn with_feature<R>(
        &mut self,
        id: FeatureId,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut FeatureCx) -> R,
    ) -> Option<R> {
        if !matches!(self.features.slot(id), Slot::Active(_)) {
            return None;
        }
        let started = Instant::now();
        let result = f(&mut FeatureCx {
            shell: self,
            window,
            cx,
            feature: id,
        });
        let took = started.elapsed();
        if over_budget(took) {
            log::warn!(
                "feature {} took {took:.1?}, over its 2 ms budget",
                id.name()
            );
        }
        Some(result)
    }

    /// With no vault, no feature activates and no adapter renders.
    pub(super) fn vault_features(features: Registry, vault: bool) -> Registry {
        if vault {
            features
        } else {
            Registry::disabled(features.catalog())
        }
    }

    /// `AppShell` and every enabled feature, so a feature's keys are live only while it is on.
    pub(super) fn key_context(&self) -> KeyContext {
        let mut context = KeyContext::default();
        context.add("AppShell");
        for manifest in self.features.enabled() {
            context.add(manifest.id.name());
        }
        context
    }

    /// A feature switched off closes its modal and its panes' tabs before its handle drops; one
    /// switched on puts its panes back, inactive, and activates now if it waits for the first frame.
    pub(super) fn apply_switches(
        &mut self,
        switches: &Switches,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replaced = self.features.apply(switches);
        if replaced.is_empty() {
            return;
        }
        for (id, old) in replaced {
            let panes = self.features.manifest(id).panes;
            if matches!(old, Slot::Off) {
                self.change_layout(window, cx, |shell, _, cx| {
                    for &pane in panes {
                        shell.return_pane(pane, cx);
                    }
                });
                continue;
            }
            if self
                .modal
                .as_ref()
                .is_some_and(|modal| modal.owner == Some(id))
            {
                self.dismiss_modal(window, cx);
            }
            // Not `close_tab`: it sends a view tab back to its dock (spec D9).
            self.change_layout(window, cx, |shell, window, cx| {
                for pane in panes {
                    if let Some((group, tab)) = shell.tab_of(pane.id) {
                        drop(shell.detach(group, tab, window, cx));
                    }
                }
            });
            if let Some(vault) = &mut self.panes {
                for pane in panes {
                    vault.entries.remove(&pane.id);
                }
            }
            // The handle goes last: its entities outlive their modal and their tabs.
            drop(old);
        }
        self.activate_due(window, cx);
        cx.notify();
    }

    /// `pane` last and inactive in its side's first dock group, or alone in a new one; the dock
    /// stays shown or hidden as it was.
    fn return_pane(&mut self, pane: &PaneDecl, cx: &mut Context<Self>) {
        let Some(&group) = self.layout.dock_groups(pane.side).first() else {
            let group = self.layout.add_dock(pane.side);
            self.groups.insert(group, Group::holding(&[pane.id], 0));
            return;
        };
        let Group { tabs, shown, .. } = self.group_mut(group);
        let active = tabs.active();
        tabs.insert(tabs.count(), || Parked::park(shown, cx));
        // `shown` never left the active tab: the copy parked for it drops.
        drop(tabs.activate(active, || Parked::View(pane.id)));
    }
}
