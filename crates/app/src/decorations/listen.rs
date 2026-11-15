//! A layer's listener: what it hears of each buffer, once per change (decoration layer spec, D4).

use std::any::Any;
use std::rc::Rc;
use std::time::Instant;

use gpui::{App, Entity, Subscription};
use inkra_text::{Buffer, EditMark, Splice};

use super::{Decorations, LayerId, rank};
use crate::feature::over_budget;

/// What a layer's open function hands its listener (D8).
pub(crate) type Seed = Box<dyn Any + Send>;

pub(crate) enum TextChange {
    /// The listener has not heard of the buffer before.
    Opened(Option<Seed>),
    /// The splices since it last heard.
    Edited(Vec<Splice>),
    /// The layer's marks over the buffer are gone: it publishes them all again.
    Lost,
}

/// Must not update the buffer or any `EditorView`: it runs inside the view's lease.
pub(crate) type Listener = dyn Fn(&Entity<Buffer>, TextChange, &mut App);

/// How far a layer's listener has heard a buffer.
pub(super) enum Heard {
    Unheard,
    Lost,
    At(EditMark),
}

impl Heard {
    /// What the listener has not heard of `text` yet, if anything; after, it has heard all of it.
    /// A first hearing hands over `seed` if `text` is still at its edit mark.
    pub(super) fn hear(
        &mut self,
        text: &Buffer,
        seed: Option<(EditMark, Seed)>,
    ) -> Option<TextChange> {
        let now = text.edit_mark();
        let change = match *self {
            Self::Unheard => Some(TextChange::Opened(
                seed.filter(|(mark, _)| *mark == now).map(|(_, seed)| seed),
            )),
            Self::Lost => Some(TextChange::Lost),
            Self::At(mark) if mark == now => None,
            Self::At(mark) => Some(text.edits_since(mark).map_or(TextChange::Lost, |splices| {
                TextChange::Edited(splices.collect())
            })),
        };
        *self = Self::At(now);
        change
    }
}

/// `listener` hears every buffer `layer` is synced over until the subscription drops; every window
/// redraws, so each note shown hears it now.
///
/// # Panics
/// If `layer` is not declared.
pub(crate) fn listen(layer: LayerId, listener: Rc<Listener>, cx: &mut App) -> Subscription {
    let rank = rank(layer, cx);
    let listeners = &mut cx.global_mut::<Decorations>().listeners;
    debug_assert!(
        listeners[rank]
            .as_ref()
            .is_none_or(|old| old.strong_count() == 0),
        "layer {} of feature {} already has a listener",
        layer.name,
        layer.feature.name()
    );
    listeners[rank] = Some(Rc::downgrade(&listener));
    cx.refresh_windows();
    Subscription::new(move || drop(listener))
}

/// Tells each listener what it has not heard, timing each call (D11).
pub(super) fn tell(
    buffer: &Entity<Buffer>,
    unheard: Vec<(Rc<Listener>, LayerId, TextChange)>,
    cx: &mut App,
) {
    for (listener, layer, change) in unheard {
        timed(layer, || listener(buffer, change, cx));
    }
}

/// `call`'s result; past its budget, a line names `layer` and its feature (D11).
pub(super) fn timed<T>(layer: LayerId, call: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = call();
    let took = started.elapsed();
    if over_budget(took) {
        log::warn!(
            "layer {} of feature {} took {took:.1?}, over its 2 ms budget",
            layer.name,
            layer.feature.name()
        );
    }
    result
}
