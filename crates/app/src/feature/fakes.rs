//! Test features: one that counts its command, one that counts its activations after the
//! first frame, and key-conflict fakes the keymap tests check.

use std::cell::{Cell, RefCell};

use gpui::{App, WeakEntity, actions};

use super::{CommandDecl, FeatureId, Manifest};
use crate::shell::FeatureCx;

actions!(fakes, [PingFake, ClashFake]);

pub(crate) struct Ping {
    count: usize,
}

thread_local! {
    static PING: RefCell<Option<WeakEntity<Ping>>> = const { RefCell::new(None) };
    static EAGER: Cell<usize> = const { Cell::new(0) };
}

pub(crate) static PING_FAKE: Manifest = Manifest {
    id: FeatureId::new("ping_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<PingFake>(&["ctrl-alt-p"])],
    activate: activate_ping,
};

pub(crate) static CLASH_FAKE: Manifest = Manifest {
    id: FeatureId::new("clash_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-o"])],
    activate: |_| {},
};

pub(crate) static EAGER_FAKE: Manifest = Manifest {
    id: FeatureId::new("eager_fake"),
    default_on: true,
    after_first_frame: true,
    commands: &[],
    activate: |_| EAGER.set(EAGER.get() + 1),
};

pub(crate) static UNDO_FAKE: Manifest = Manifest {
    id: FeatureId::new("undo_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-z"])],
    activate: |_| {},
};

pub(crate) static CHORD_FAKE: Manifest = Manifest {
    id: FeatureId::new("chord_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-k"])],
    activate: |_| {},
};

/// Shares `ctrl-alt-p` with `PING_FAKE`.
pub(crate) static TWIN_FAKE: Manifest = Manifest {
    id: FeatureId::new("twin_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["ctrl-alt-p"])],
    activate: |_| {},
};

pub(crate) static CATALOG: &[&Manifest] = &[&PING_FAKE];
pub(crate) static EAGER_CATALOG: &[&Manifest] = &[&EAGER_FAKE];

fn activate_ping(fcx: &mut FeatureCx) {
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
