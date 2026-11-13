//! Test features: one that counts its command, one that counts its activations after the
//! first frame, one with a pane holding an input, and key-conflict fakes the keymap tests check.

use std::cell::{Cell, RefCell};

use gpui::{
    App, Context, FocusHandle, InteractiveElement, IntoElement, Render, Styled, WeakEntity, Window,
    actions, div,
};
use inkra_layout::{Side, ViewId};

use super::{CommandDecl, FeatureId, Manifest};
use crate::pane::{Pane, PaneDecl, PaneHeader, boxed};
use crate::shell::Activation;
use crate::ui::IconName;

actions!(fakes, [PingFake, ClashFake, MovePaneFakeToOtherDock]);

pub(crate) struct Ping {
    count: usize,
}

thread_local! {
    static PING: RefCell<Option<WeakEntity<Ping>>> = const { RefCell::new(None) };
    static EAGER: Cell<usize> = const { Cell::new(0) };
    static INPUT: RefCell<Option<WeakEntity<InputPane>>> = const { RefCell::new(None) };
    static PANE_ACTIVATIONS: Cell<usize> = const { Cell::new(0) };
}

/// A pane whose one element takes keyboard focus.
pub(crate) struct InputPane {
    focus: FocusHandle,
    hidden: usize,
}

impl Render for InputPane {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.focus).size_full()
    }
}

impl Pane for InputPane {
    fn header(&self, _: &App) -> PaneHeader {
        PaneHeader::Title { count: None }
    }

    fn focus(&self, _: &App) -> Option<FocusHandle> {
        Some(self.focus.clone())
    }

    fn hidden(&mut self, _: &mut Context<Self>) {
        self.hidden += 1;
    }
}

pub(crate) static PING_FAKE: Manifest = Manifest {
    id: FeatureId::new("ping_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<PingFake>(&["ctrl-alt-p"])],
    panes: &[],
    buttons: &[],
    activate: activate_ping,
};

pub(crate) static CLASH_FAKE: Manifest = Manifest {
    id: FeatureId::new("clash_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-o"])],
    panes: &[],
    buttons: &[],
    activate: |_| {},
};

pub(crate) static EAGER_FAKE: Manifest = Manifest {
    id: FeatureId::new("eager_fake"),
    default_on: true,
    after_first_frame: true,
    commands: &[],
    panes: &[],
    buttons: &[],
    activate: |_| EAGER.set(EAGER.get() + 1),
};

pub(crate) static UNDO_FAKE: Manifest = Manifest {
    id: FeatureId::new("undo_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-z"])],
    panes: &[],
    buttons: &[],
    activate: |_| {},
};

pub(crate) static CHORD_FAKE: Manifest = Manifest {
    id: FeatureId::new("chord_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-k"])],
    panes: &[],
    buttons: &[],
    activate: |_| {},
};

/// Shares `ctrl-alt-p` with `PING_FAKE`.
pub(crate) static TWIN_FAKE: Manifest = Manifest {
    id: FeatureId::new("twin_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["ctrl-alt-p"])],
    panes: &[],
    buttons: &[],
    activate: |_| {},
};

pub(crate) const PANE_FAKE_VIEW: ViewId = ViewId::new("pane_fake");

pub(crate) static PANE_FAKE_DECL: PaneDecl = PaneDecl {
    id: PANE_FAKE_VIEW,
    title: "Pane fake",
    icon: IconName::Search,
    icon_id: "dock-icon-pane_fake",
    header_id: "panel-header-pane_fake",
    side: Side::Right,
    move_action: boxed::<MovePaneFakeToOtherDock>,
};

pub(crate) static PANE_FAKE: Manifest = Manifest {
    id: FeatureId::new("pane_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[],
    panes: &[&PANE_FAKE_DECL],
    buttons: &[],
    activate: activate_pane,
};

pub(crate) static CATALOG: &[&Manifest] = &[&PING_FAKE];
pub(crate) static EAGER_CATALOG: &[&Manifest] = &[&EAGER_FAKE];
/// Backlinks first, so the fake pane docks right after it, as a second feature pane would.
pub(crate) static PANE_CATALOG: &[&Manifest] = &[&crate::backlinks::MANIFEST, &PANE_FAKE];

fn activate_ping(fcx: &mut Activation) {
    let ping = fcx.new(|_| Ping { count: 0 });
    PING.set(Some(ping.downgrade()));
    fcx.on_command::<PingFake>(move |_, fcx| {
        fcx.update(&ping, |ping, cx| {
            ping.count += 1;
            cx.notify();
        });
    });
}

/// The live `Ping`'s count; `None` once no feature holds it.
pub(crate) fn ping_count(cx: &App) -> Option<usize> {
    let ping = PING.with_borrow(|ping| ping.as_ref()?.upgrade())?;
    Some(ping.read(cx).count)
}

/// How many times `EAGER_FAKE` activated on this thread.
pub(crate) fn eager_count() -> usize {
    EAGER.get()
}

fn activate_pane(fcx: &mut Activation) {
    let input = fcx.new(|cx| InputPane {
        focus: cx.focus_handle(),
        hidden: 0,
    });
    INPUT.set(Some(input.downgrade()));
    PANE_ACTIVATIONS.set(PANE_ACTIVATIONS.get() + 1);
    fcx.register_pane(PANE_FAKE_VIEW, input);
}

/// The live `InputPane`'s focus and how many times it was hidden; `None` once no feature holds it.
pub(crate) fn pane_input(cx: &App) -> Option<(FocusHandle, usize)> {
    let input = INPUT.with_borrow(|input| input.as_ref()?.upgrade())?;
    let input = input.read(cx);
    Some((input.focus.clone(), input.hidden))
}

/// How many times `PANE_FAKE` activated on this thread.
pub(crate) fn pane_activations() -> usize {
    PANE_ACTIVATIONS.get()
}
