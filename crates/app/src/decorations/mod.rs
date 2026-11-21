//! Styled ranges features publish over a buffer, one layer per declared `LayerDecl`, drawn by
//! every view of that buffer (decoration layer spec).

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Weak;

use gpui::{App, BorrowAppContext, Entity, EntityId, Global, Subscription, WeakEntity};
pub(crate) use inkra_editor::TextChange;
use inkra_editor::{HeardLayer, Moved, RevisionLog};
use inkra_text::{Buffer, ByteOffset, EditMark};

use crate::feature::FeatureId;
use crate::theme::ColorToken;

mod listen;
mod open;

use listen::Listener;
pub(crate) use listen::listen;
pub(crate) use open::Chunk;
pub(crate) use open::{OpenFn, openers, run_openers, seed};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayerId {
    feature: FeatureId,
    name: &'static str,
}

impl LayerId {
    pub(crate) const fn new(feature: FeatureId, name: &'static str) -> Self {
        Self { feature, name }
    }
}

pub(crate) struct LayerDecl {
    pub(crate) id: LayerId,
    pub(crate) open: Option<OpenFn>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkStyle {
    pub(crate) color: Option<ColorToken>,
}

// Every stored mark carries one: a wider style grows each mark (decoration layer spec, deviation 17).
const _: () = assert!(size_of::<MarkStyle>() == 1);

#[derive(Default)]
struct Decorations {
    /// A layer's rank is its index here: a later layer draws over an earlier one.
    declared: Vec<Declared>,
    entries: HashMap<EntityId, Entry>,
    revisions: u64,
}

impl Global for Decorations {}

struct Declared {
    decl: &'static LayerDecl,
    /// The listener its `listen` subscription holds.
    listener: Option<Weak<Listener>>,
    /// Whether its feature is on.
    enabled: bool,
}

struct Entry {
    /// Removes the entry when its buffer is released.
    _released: Subscription,
    /// Its revision moves on every change a view must draw, and logs what moved.
    log: RevisionLog,
    /// One per declared layer, by rank.
    layers: Vec<HeardLayer<MarkStyle>>,
}

impl Entry {
    fn bump(&mut self, revisions: &mut u64, moved: Moved) {
        *revisions += 1;
        self.log.bump(*revisions, moved);
    }
}

/// Replaces the declared layers, dropping every published mark, unless they are unchanged.
pub(crate) fn declare(layers: impl IntoIterator<Item = &'static LayerDecl>, cx: &mut App) {
    let layers: Vec<_> = layers.into_iter().collect();
    let decorations = cx.default_global::<Decorations>();
    if decorations
        .declared
        .iter()
        .map(|declared| declared.decl.id)
        .eq(layers.iter().map(|decl| decl.id))
    {
        return;
    }
    let mut old = std::mem::take(&mut decorations.declared).into_iter();
    decorations.declared = layers
        .into_iter()
        .map(|decl| Declared {
            decl,
            listener: old.next().and_then(|old| old.listener),
            enabled: false,
        })
        .collect();
    decorations.entries.clear();
}

/// Switches each layer on when its feature is in `features`. A layer switched off drops its marks
/// and redraws; one switched either way is heard as lost, so a listener publishes it all again.
pub(crate) fn enable(features: impl IntoIterator<Item = FeatureId>, cx: &mut App) {
    let features: Vec<_> = features.into_iter().collect();
    let Decorations {
        declared,
        entries,
        revisions,
    } = cx.default_global::<Decorations>();
    let mut switched = false;
    for (rank, declared) in declared.iter_mut().enumerate() {
        let on = features.contains(&declared.decl.id.feature);
        if on == declared.enabled {
            continue;
        }
        declared.enabled = on;
        switched = true;
        for entry in entries.values_mut() {
            if entry.layers[rank].switch(on) {
                entry.bump(revisions, Moved::Whole);
            }
        }
    }
    if switched {
        cx.refresh_windows();
    }
}

/// Whether `layer`'s feature is on.
///
/// # Panics
/// If `layer` is not declared.
pub(crate) fn enabled(layer: LayerId, cx: &App) -> bool {
    cx.global::<Decorations>().declared[rank(layer, cx)].enabled
}

/// # Panics
/// If `layer` is not declared.
fn rank(layer: LayerId, cx: &App) -> usize {
    cx.try_global::<Decorations>()
        .and_then(|decorations| {
            decorations
                .declared
                .iter()
                .position(|declared| declared.decl.id == layer)
        })
        .unwrap_or_else(|| {
            panic!(
                "undeclared layer {} of feature {}",
                layer.name,
                layer.feature.name()
            )
        })
}

/// `buffer`'s entry, created with every layer at its edit mark, unheard; it drops at the buffer's
/// release.
fn entry<'a>(
    entries: &'a mut HashMap<EntityId, Entry>,
    declared: usize,
    buffer: &Entity<Buffer>,
    cx: &App,
) -> &'a mut Entry {
    let id = buffer.entity_id();
    entries.entry(id).or_insert_with(|| Entry {
        _released: cx.observe_release(buffer, move |_, cx| {
            cx.global_mut::<Decorations>().entries.remove(&id);
        }),
        log: RevisionLog::new(),
        layers: (0..declared)
            .map(|_| HeardLayer::new(buffer.read(cx).edit_mark()))
            .collect(),
    })
}

/// As `HeardLayer::publish`, on `layer` over `buffer`; redraws every window once when the marks
/// change. A lost layer's listener hears `Lost` at the next sync, and publishes it all again.
///
/// # Panics
/// If `layer` is not declared, or as `HeardLayer::publish`.
pub(crate) fn publish(
    layer: LayerId,
    buffer: &WeakEntity<Buffer>,
    removed: Vec<Range<ByteOffset>>,
    chunks: Vec<Chunk>,
    at: EditMark,
    cx: &mut App,
) {
    let Some(buffer) = buffer.upgrade() else {
        return;
    };
    let rank = rank(layer, cx);
    // A publish computed before its feature went off.
    if !cx.global::<Decorations>().declared[rank].enabled {
        return;
    }
    let changed = cx.update_global(|decorations: &mut Decorations, cx| {
        let declared = decorations.declared.len();
        let entry = entry(&mut decorations.entries, declared, &buffer, cx);
        let text = buffer.read(cx);
        let Some(moved) = entry.layers[rank].publish(text, removed, chunks, at) else {
            return false;
        };
        entry.bump(&mut decorations.revisions, moved);
        true
    });
    if changed {
        cx.refresh_windows();
    }
}

/// The chunks `layer` holds over `buffer`, each with its tag, sorted (code highlighting spec, D9
/// and deviations 6 and 7).
///
/// # Panics
/// If `layer` is not declared.
pub(crate) fn chunks(
    layer: LayerId,
    buffer: &Entity<Buffer>,
    cx: &App,
) -> Vec<(Range<ByteOffset>, Option<u64>)> {
    let rank = rank(layer, cx);
    let Some(entry) = cx.global::<Decorations>().entries.get(&buffer.entity_id()) else {
        return Vec::new();
    };
    let layer = entry.layers[rank].layer();
    // Offsets behind the buffer's would mis-plan without a sign.
    debug_assert!(!layer.trails(buffer.read(cx).edit_mark()));
    layer.chunks().collect()
}

/// Brings every layer of `buffer` to its edit mark. A non-empty layer behind it draws something
/// else after, moved or lost, so the entry's revision moves: undo can bring back a shaped version.
/// A lost layer logs `Whole`, an empty one too, which is rare; a shifted one logs `Shift`.
/// Then each listener hears what it has not, once the global is released.
pub(crate) fn sync(buffer: &Entity<Buffer>, cx: &mut App) {
    if cx
        .try_global::<Decorations>()
        .is_none_or(|decorations| decorations.declared.is_empty())
    {
        return;
    }
    let unheard = cx.update_global(|decorations: &mut Decorations, cx| {
        let entry = entry(
            &mut decorations.entries,
            decorations.declared.len(),
            buffer,
            cx,
        );
        let text = buffer.read(cx);
        let mut moved = None;
        let mut unheard = Vec::new();
        let enabled = entry
            .layers
            .iter_mut()
            .zip(&decorations.declared)
            .filter(|(_, declared)| declared.enabled);
        for (layer, declared) in enabled {
            match layer.catch_up(text) {
                Some(Moved::Whole) => moved = Some(Moved::Whole),
                caught => moved = moved.or(caught),
            }
            if let Some(listener) = declared.listener.as_ref().and_then(Weak::upgrade)
                && let Some(change) = layer.hear(text)
            {
                unheard.push((listener, declared.decl.id, change));
            }
        }
        if let Some(moved) = moved {
            entry.bump(&mut decorations.revisions, moved);
        }
        unheard
    });
    listen::tell(buffer, unheard, cx);
}

#[cfg(test)]
pub(crate) fn tracked(cx: &App) -> usize {
    cx.try_global::<Decorations>()
        .map_or(0, |decorations| decorations.entries.len())
}

/// The log of a buffer no feature published to.
static NO_LOG: RevisionLog = RevisionLog::new();

/// What a view draws of a buffer's layers.
#[derive(Clone, Copy)]
pub(crate) struct Drawn<'a> {
    log: &'a RevisionLog,
    layers: &'a [HeardLayer<MarkStyle>],
    /// The buffer's edit mark: a layer at another one is stale and not drawn.
    at: EditMark,
}

impl Default for Drawn<'_> {
    fn default() -> Self {
        Self {
            log: &NO_LOG,
            layers: &[],
            at: EditMark::default(),
        }
    }
}

/// Allocates nothing: a buffer no feature published to draws `Drawn::default()`.
pub(crate) fn drawn<'a>(buffer: &Entity<Buffer>, cx: &'a App) -> Drawn<'a> {
    let Some(entry) = cx
        .try_global::<Decorations>()
        .and_then(|decorations| decorations.entries.get(&buffer.entity_id()))
    else {
        return Drawn::default();
    };
    Drawn {
        log: &entry.log,
        layers: &entry.layers,
        at: buffer.read(cx).edit_mark(),
    }
}

#[cfg(any(test, feature = "bench"))]
impl<'a> Drawn<'a> {
    pub(crate) fn for_tests(
        layers: &'a [HeardLayer<MarkStyle>],
        log: &'a RevisionLog,
        at: EditMark,
    ) -> Self {
        Self { log, layers, at }
    }
}

impl Drawn<'_> {
    /// What a layout records, `None` while a layer with marks trails the buffer: it draws none
    /// of them, and the `sync` that brings it back logs only a `Shift`.
    pub(crate) fn revision(&self) -> Option<u64> {
        let stale = self
            .layers
            .iter()
            .map(HeardLayer::layer)
            .any(|layer| layer.trails(self.at));
        (!stale).then(|| self.log.revision())
    }

    /// The ranges of `buffer`'s text whose colours may differ from those drawn at `revision`, as
    /// `RevisionLog::changed_since`; `None` too when either revision is `None`.
    pub(crate) fn changed_since(
        &self,
        revision: Option<u64>,
        buffer: &Buffer,
    ) -> Option<Vec<Range<ByteOffset>>> {
        self.revision()?;
        self.log.changed_since(revision?, buffer)
    }

    /// The fresh layers' colours over `line`, clipped to it, by rank: a later one paints over.
    pub(crate) fn colors_in(
        &self,
        line: Range<ByteOffset>,
    ) -> Vec<(Range<ByteOffset>, ColorToken)> {
        self.layers
            .iter()
            .map(HeardLayer::layer)
            .filter(|layer| layer.at() == self.at)
            .flat_map(|layer| layer.marks_in(line.clone()))
            .filter_map(|(mark, style)| Some((mark, style.color?)))
            .collect()
    }
}
