//! A layer's listener: what it hears of each buffer, once per change (decoration layer spec, D4).

use std::rc::Rc;
use std::time::Instant;

use gpui::{App, Entity, Subscription};
use inkra_text::Buffer;

use super::{Decorations, LayerId, TextChange, rank};
use crate::feature::over_budget;

/// Must not update the buffer or any `EditorView`: it runs inside the view's lease.
pub(crate) type Listener = dyn Fn(&Entity<Buffer>, TextChange, &mut App);

/// `listener` hears every buffer `layer` is synced over until the subscription drops; every window
/// redraws, so each note shown hears it now.
///
/// # Panics
/// If `layer` is not declared.
pub(crate) fn listen(layer: LayerId, listener: Rc<Listener>, cx: &mut App) -> Subscription {
    let rank = rank(layer, cx);
    let held = &mut cx.global_mut::<Decorations>().declared[rank].listener;
    debug_assert!(
        held.as_ref().is_none_or(|old| old.strong_count() == 0),
        "layer {} of feature {} already has a listener",
        layer.name,
        layer.feature.name()
    );
    *held = Some(Rc::downgrade(&listener));
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
