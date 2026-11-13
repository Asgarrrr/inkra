//! The shell's side of the feature registry: launching features, running their commands, and switching them.

use std::any::TypeId;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Action, AppContext as _, Context, Entity, KeyContext, Window};

use super::AppShell;
use crate::feature::{FeatureHandle, FeatureId, Registry, Slot};
use crate::settings::Switches;
use crate::timeline::{Step, Timeline};
use crate::vault::{ConfigChanged, NoteStore};

/// A feature's activation or call past this logs a line (spec D11).
pub(super) fn over_budget(took: Duration) -> bool {
    took > Duration::from_millis(2)
}

/// What a feature reaches while it activates or runs a command.
pub(crate) struct FeatureCx<'a, 'b> {
    shell: &'a mut AppShell,
    #[expect(dead_code, reason = "no feature draws in the window before quick open")]
    window: &'a mut Window,
    cx: &'a mut Context<'b, AppShell>,
    feature: FeatureId,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "BUILTIN is empty until quick open joins it")
)]
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

    fn handle(&mut self) -> &mut FeatureHandle {
        match self.shell.features.slot_mut(self.feature) {
            Slot::Active(handle) => handle,
            Slot::Off | Slot::Dormant => {
                unreachable!("a FeatureCx exists only while its feature is active")
            }
        }
    }
}

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
        self.activate_due(window, cx);
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

    /// A feature switched on after the launch activates now if it waits for the first frame.
    pub(super) fn apply_switches(
        &mut self,
        switches: &Switches,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.features.apply(switches) {
            self.activate_due(window, cx);
            cx.notify();
        }
    }
}
