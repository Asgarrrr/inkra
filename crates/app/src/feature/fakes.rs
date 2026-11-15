//! Test features: one that counts its command, one that counts its activations after the
//! first frame, one with a pane holding an input, one that colours each to-do marker through a layer,
//! and key-conflict fakes the keymap tests check.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    App, Context, Entity, FocusHandle, InteractiveElement, IntoElement, Render, Styled, WeakEntity,
    Window, actions, div,
};
use inkra_layout::{Side, ViewId};
use inkra_text::{Buffer, ByteOffset};

use super::{CommandDecl, FeatureId, Manifest};
use crate::decorations::{self, LayerDecl, LayerId, MarkStyle, Opened, TextChange};
use crate::pane::{Pane, PaneDecl, PaneHeader, boxed};
use crate::shell::Activation;
use crate::theme::ColorToken;
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
    static HEARD: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static DELAY: Cell<Option<Duration>> = const { Cell::new(None) };
    static SILENT: Cell<bool> = const { Cell::new(false) };
    static UNBOUNDED: Cell<bool> = const { Cell::new(false) };
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
    layers: &[],
    activate: activate_ping,
};

pub(crate) static CLASH_FAKE: Manifest = Manifest {
    id: FeatureId::new("clash_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-o"])],
    panes: &[],
    buttons: &[],
    layers: &[],
    activate: |_| {},
};

pub(crate) static EAGER_FAKE: Manifest = Manifest {
    id: FeatureId::new("eager_fake"),
    default_on: true,
    after_first_frame: true,
    commands: &[],
    panes: &[],
    buttons: &[],
    layers: &[],
    activate: |_| EAGER.set(EAGER.get() + 1),
};

pub(crate) static UNDO_FAKE: Manifest = Manifest {
    id: FeatureId::new("undo_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-z"])],
    panes: &[],
    buttons: &[],
    layers: &[],
    activate: |_| {},
};

pub(crate) static CHORD_FAKE: Manifest = Manifest {
    id: FeatureId::new("chord_fake"),
    default_on: true,
    after_first_frame: false,
    commands: &[CommandDecl::new::<ClashFake>(&["cmd-k"])],
    panes: &[],
    buttons: &[],
    layers: &[],
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
    layers: &[],
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
    layers: &[],
    activate: activate_pane,
};

pub(crate) const TODO_LAYER: LayerId = LayerId::new(FeatureId::new("todo_fake"), "todo");

pub(crate) const TODO_DECL: LayerDecl = LayerDecl {
    id: TODO_LAYER,
    open: Some(open_todo),
};

pub(crate) static TODO_FAKE: Manifest = Manifest {
    id: FeatureId::new("todo_fake"),
    default_on: true,
    after_first_frame: true,
    commands: &[],
    panes: &[],
    buttons: &[],
    layers: &[TODO_DECL],
    activate: |fcx| fcx.on_text_change(TODO_LAYER, todo_listener),
};

pub(crate) static CATALOG: &[&Manifest] = &[&PING_FAKE];
pub(crate) static EAGER_CATALOG: &[&Manifest] = &[&EAGER_FAKE];
/// Backlinks first, so the fake pane docks right after it, as a second feature pane would.
pub(crate) static PANE_CATALOG: &[&Manifest] = &[&crate::backlinks::MANIFEST, &PANE_FAKE];
pub(crate) static TODO_CATALOG: &[&Manifest] = &[&PING_FAKE, &TODO_FAKE];

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

/// The warning colour over the to-do marker `word` at `start`.
fn todo_mark((start, word): (usize, &str)) -> (Range<ByteOffset>, MarkStyle) {
    let warning = MarkStyle {
        color: Some(ColorToken::Warning),
    };
    (
        ByteOffset::new(start)..ByteOffset::new(start + word.len()),
        warning,
    )
}

/// Marks each to-do marker of `text` before `deadline`, unless `set_unbounded` lifted it; its seed
/// is where it stopped.
pub(crate) fn open_todo(text: &str, deadline: Instant) -> Opened {
    let mut stop = text.len();
    let mut marks = Vec::new();
    for found in text.match_indices("TODO") {
        if !UNBOUNDED.get() && Instant::now() >= deadline {
            stop = found.0;
            break;
        }
        marks.push(todo_mark(found));
    }
    Opened {
        chunks: vec![(ByteOffset::new(0)..ByteOffset::new(stop), 0, marks)],
        seed: Some(Box::new(stop)),
    }
}

/// Notes what it heard, then, unless `set_silent` asked otherwise, publishes the warning colour
/// over each to-do marker of `buffer`, after the delay `set_delay` asked for, if any.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a `Listener` takes the change by value"
)]
pub(crate) fn todo_listener(buffer: &Entity<Buffer>, change: TextChange, cx: &mut App) {
    let heard = match change {
        TextChange::Opened(Some(_)) => "opened+seed",
        TextChange::Opened(None) => "opened",
        TextChange::Edited(_) => "edited",
        TextChange::Lost => "lost",
    };
    HEARD.with_borrow_mut(|all| all.push(heard));
    if SILENT.get() {
        return;
    }
    let text = buffer.read(cx);
    let marks = text.text().match_indices("TODO").map(todo_mark).collect();
    let (chunk, at, buffer) = (
        ByteOffset::new(0)..text.len(),
        text.edit_mark(),
        buffer.downgrade(),
    );
    let Some(delay) = DELAY.get() else {
        decorations::publish(
            TODO_LAYER,
            &buffer,
            Vec::new(),
            vec![(chunk, 0, marks)],
            at,
            cx,
        );
        return;
    };
    cx.spawn(async move |cx| {
        cx.background_executor().timer(delay).await;
        let chunks = vec![(chunk, 0, marks)];
        cx.update(|cx| decorations::publish(TODO_LAYER, &buffer, Vec::new(), chunks, at, cx));
    })
    .detach();
}

/// Declares `TODO_DECL` alone and switches it on, as a shell with `TODO_FAKE` on does.
pub(crate) fn declare_todo(cx: &mut App) {
    decorations::declare([&TODO_DECL], cx);
    decorations::enable([TODO_FAKE.id], cx);
}

/// Makes `todo_listener` note what it heard and publish nothing.
pub(crate) fn set_silent(silent: bool) {
    SILENT.set(silent);
}

/// Makes `open_todo` ignore its deadline, so a landing through `NoteStore`, which reads the wall
/// clock, colours every marker.
pub(crate) fn set_unbounded(unbounded: bool) {
    UNBOUNDED.set(unbounded);
}

/// Makes `todo_listener` publish `delay` after it computed, as a background task would.
pub(crate) fn set_delay(delay: Option<Duration>) {
    DELAY.set(delay);
}

/// What `todo_listener` heard on this thread since the last call, in order.
pub(crate) fn heard() -> Vec<&'static str> {
    HEARD.take()
}
