//! Test features: one that counts its command, one whose key clashes with a core key.

use std::cell::RefCell;

use gpui::{App, WeakEntity, actions};

use super::{CommandDecl, FeatureId, Manifest};
use crate::shell::FeatureCx;

actions!(fakes, [PingFake, ClashFake]);

pub(crate) struct Ping {
    count: usize,
}

thread_local! {
    static PING: RefCell<Option<WeakEntity<Ping>>> = const { RefCell::new(None) };
}

pub(crate) static PING_FAKE: Manifest = Manifest {
    id: FeatureId::new("ping_fake"),
    default_on: true,
    commands: &[CommandDecl::new::<PingFake>(&["ctrl-alt-p"])],
    activate: activate_ping,
};

pub(crate) static CLASH_FAKE: Manifest = Manifest {
    id: FeatureId::new("clash_fake"),
    default_on: true,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-o"])],
    activate: |_| {},
};

pub(crate) static UNDO_FAKE: Manifest = Manifest {
    id: FeatureId::new("undo_fake"),
    default_on: true,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-z"])],
    activate: |_| {},
};

pub(crate) static CHORD_FAKE: Manifest = Manifest {
    id: FeatureId::new("chord_fake"),
    default_on: true,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-k"])],
    activate: |_| {},
};

/// Shares `ctrl-alt-p` with `PING_FAKE`.
pub(crate) static TWIN_FAKE: Manifest = Manifest {
    id: FeatureId::new("twin_fake"),
    default_on: true,
    commands: &[CommandDecl::new::<ClashFake>(&["ctrl-alt-p"])],
    activate: |_| {},
};

pub(crate) static CATALOG: &[&Manifest] = &[&PING_FAKE];

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
