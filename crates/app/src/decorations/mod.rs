//! Styled ranges features publish over a buffer, one layer per declared `LayerDecl`, drawn by
//! every view of that buffer (decoration layer spec).

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Weak;

use gpui::{App, BorrowAppContext, Entity, EntityId, Global, Subscription, WeakEntity};
use inkra_editor::Layer;
use inkra_text::{Buffer, ByteOffset, EditMark};

use crate::feature::FeatureId;
use crate::theme::ColorToken;

mod listen;
mod open;

use listen::{Heard, Listener, Seed};
pub(crate) use listen::{TextChange, listen};
pub(crate) use open::Chunk;
pub(crate) use open::Opened;
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

#[derive(Default)]
struct Decorations {
    /// A layer's rank is its index here: a later layer draws over an earlier one.
    declared: Vec<&'static LayerDecl>,
    entries: HashMap<EntityId, Entry>,
    revisions: u64,
    /// One per declared layer, by rank: the listener its `listen` subscription holds.
    listeners: Vec<Option<Weak<Listener>>>,
    /// One per declared layer, by rank: whether its feature is on.
    enabled: Vec<bool>,
}

impl Global for Decorations {}

struct Entry {
    /// Removes the entry when its buffer is released.
    _released: Subscription,
    /// Moves on every change a view must draw.
    revision: u64,
    /// One per declared layer, by rank.
    layers: Vec<Layer<MarkStyle>>,
    /// One per declared layer, by rank.
    heard: Vec<Heard>,
    /// One per declared layer, by rank: its open function's seed, valid only at its edit mark.
    seeds: Vec<Option<(EditMark, Seed)>>,
}

/// Replaces the declared layers, dropping every published mark, unless they are unchanged.
pub(crate) fn declare(layers: impl IntoIterator<Item = &'static LayerDecl>, cx: &mut App) {
    let layers: Vec<_> = layers.into_iter().collect();
    let decorations = cx.default_global::<Decorations>();
    if decorations
        .declared
        .iter()
        .map(|decl| decl.id)
        .eq(layers.iter().map(|decl| decl.id))
    {
        return;
    }
    decorations.listeners.resize(layers.len(), None);
    decorations.enabled = vec![false; layers.len()];
    decorations.declared = layers;
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
        enabled,
        ..
    } = cx.default_global::<Decorations>();
    let now: Vec<_> = declared
        .iter()
        .map(|decl| features.contains(&decl.id.feature))
        .collect();
    if now == *enabled {
        return;
    }
    for rank in (0..now.len()).filter(|&rank| now[rank] != enabled[rank]) {
        for entry in entries.values_mut() {
            entry.heard[rank] = Heard::Lost;
            entry.seeds[rank] = None;
            if !now[rank] && !entry.layers[rank].is_empty() {
                entry.layers[rank].clear();
                *revisions += 1;
                entry.revision = *revisions;
            }
        }
    }
    *enabled = now;
    cx.refresh_windows();
}

/// # Panics
/// If `layer` is not declared.
fn rank(layer: LayerId, cx: &App) -> usize {
    cx.try_global::<Decorations>()
        .and_then(|decorations| {
            decorations
                .declared
                .iter()
                .position(|decl| decl.id == layer)
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
        revision: 0,
        layers: (0..declared)
            .map(|_| Layer::new(buffer.read(cx).edit_mark()))
            .collect(),
        heard: (0..declared).map(|_| Heard::Unheard).collect(),
        seeds: (0..declared).map(|_| None).collect(),
    })
}

/// As `Layer::publish`, on `layer` over `buffer`; redraws every window once when the marks change.
/// A lost layer's listener hears `Lost` at the next sync, and publishes it all again.
///
/// # Panics
/// If `layer` is not declared, or as `Layer::publish`.
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
    if !cx.global::<Decorations>().enabled[rank] {
        return;
    }
    let changed = cx.update_global(|decorations: &mut Decorations, cx| {
        let declared = decorations.declared.len();
        let entry = entry(&mut decorations.entries, declared, &buffer, cx);
        let text = buffer.read(cx);
        let published = entry.layers[rank].publish(text, removed, chunks, at);
        if published.is_err() && matches!(entry.heard[rank], Heard::At(_)) {
            entry.heard[rank] = Heard::Lost;
        }
        // A lost layer was cleared: what it drew is gone too.
        let changed = published != Ok(false);
        if changed {
            decorations.revisions += 1;
            entry.revision = decorations.revisions;
        }
        changed
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
    let layer = &entry.layers[rank];
    // Offsets behind the buffer's would mis-plan without a sign.
    debug_assert!(layer.is_empty() || layer.at() == buffer.read(cx).edit_mark());
    layer.chunks().collect()
}

/// Brings every layer of `buffer` to its edit mark. A non-empty layer behind it draws something
/// else after, moved or lost, so the entry's revision moves: undo can bring back a shaped version.
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
        let mut moved = false;
        let mut unheard = Vec::new();
        let ranks = entry
            .layers
            .iter_mut()
            .zip(entry.heard.iter_mut().zip(&mut entry.seeds))
            .zip(decorations.declared.iter().zip(&decorations.listeners))
            .zip(&decorations.enabled)
            .filter_map(|(rank, &enabled)| enabled.then_some(rank));
        for ((layer, (heard, seed)), (decl, listener)) in ranks {
            moved |= !layer.is_empty() && layer.at() != text.edit_mark();
            // `Lost` clears the layer, a change `moved` already counts.
            if layer.catch_up(text).is_err() && matches!(heard, Heard::At(_)) {
                *heard = Heard::Lost;
            }
            if let Some(listener) = listener.as_ref().and_then(Weak::upgrade)
                && let Some(change) = heard.hear(text, seed.take())
            {
                unheard.push((listener, decl.id, change));
            }
        }
        if moved {
            decorations.revisions += 1;
            entry.revision = decorations.revisions;
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

/// What a view draws of a buffer's layers.
#[derive(Clone, Copy, Default)]
pub(crate) struct Drawn<'a> {
    pub(crate) revision: u64,
    layers: &'a [Layer<MarkStyle>],
    /// The buffer's edit mark: a layer at another one is stale and not drawn.
    at: EditMark,
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
        revision: entry.revision,
        layers: &entry.layers,
        at: buffer.read(cx).edit_mark(),
    }
}

#[cfg(test)]
impl<'a> Drawn<'a> {
    pub(crate) fn for_tests(layers: &'a [Layer<MarkStyle>], at: EditMark) -> Self {
        Self {
            revision: 0,
            layers,
            at,
        }
    }
}

impl Drawn<'_> {
    /// The fresh layers' colours over `line`, clipped to it, by rank: a later one paints over.
    pub(crate) fn colors_in(
        &self,
        line: Range<ByteOffset>,
    ) -> Vec<(Range<ByteOffset>, ColorToken)> {
        self.layers
            .iter()
            .filter(|layer| layer.at() == self.at)
            .flat_map(|layer| layer.marks_in(line.clone()))
            .filter_map(|(mark, style)| Some((mark, style.color?)))
            .collect()
    }
}
