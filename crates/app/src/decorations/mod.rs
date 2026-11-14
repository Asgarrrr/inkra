//! Styled ranges features publish over a buffer, one layer per declared `LayerDecl`, drawn by
//! every view of that buffer (decoration layer spec).
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the test feature listens and publishes until a feature does"
    )
)]

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Weak;

use gpui::{App, BorrowAppContext, Entity, EntityId, Global, WeakEntity};
use inkra_editor::Layer;
use inkra_text::{Buffer, ByteOffset, EditMark};

use crate::feature::FeatureId;
use crate::theme::ColorToken;

mod listen;

use listen::{Heard, Listener};
pub(crate) use listen::{TextChange, listen};

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
}

impl Global for Decorations {}

struct Entry {
    buffer: WeakEntity<Buffer>,
    /// Moves on every change a view must draw.
    revision: u64,
    /// One per declared layer, by rank.
    layers: Vec<Layer<MarkStyle>>,
    /// One per declared layer, by rank.
    heard: Vec<Heard>,
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
    decorations.declared = layers;
    decorations.entries.clear();
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

/// `buffer`'s entry, created with every layer at its edit mark, unheard; creating one drops the
/// entries of released buffers.
fn entry<'a>(
    entries: &'a mut HashMap<EntityId, Entry>,
    declared: usize,
    buffer: &Entity<Buffer>,
    text: &Buffer,
) -> &'a mut Entry {
    let id = buffer.entity_id();
    if !entries.contains_key(&id) {
        entries.retain(|_, entry| entry.buffer.is_upgradable());
    }
    entries.entry(id).or_insert_with(|| Entry {
        buffer: buffer.downgrade(),
        revision: 0,
        layers: (0..declared)
            .map(|_| Layer::new(text.edit_mark()))
            .collect(),
        heard: (0..declared).map(|_| Heard::Unheard).collect(),
    })
}

/// As `Layer::publish`, on `layer` over `buffer`; redraws every window when the marks change. A
/// lost layer's listener hears `Lost` at the next sync, and publishes it all again.
///
/// # Panics
/// If `layer` is not declared, or as `Layer::publish`.
pub(crate) fn publish(
    layer: LayerId,
    buffer: &WeakEntity<Buffer>,
    chunk: Range<ByteOffset>,
    marks: Vec<(Range<ByteOffset>, MarkStyle)>,
    at: EditMark,
    cx: &mut App,
) {
    let Some(buffer) = buffer.upgrade() else {
        return;
    };
    let rank = rank(layer, cx);
    let changed = cx.update_global(|decorations: &mut Decorations, cx| {
        let text = buffer.read(cx);
        let declared = decorations.declared.len();
        let entry = entry(&mut decorations.entries, declared, &buffer, text);
        let published = entry.layers[rank].publish(text, chunk, marks, at);
        if published.is_err() {
            entry.heard[rank].lose();
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
        let text = buffer.read(cx);
        let entry = entry(
            &mut decorations.entries,
            decorations.declared.len(),
            buffer,
            text,
        );
        let mut moved = false;
        let mut unheard = Vec::new();
        let ranks = entry
            .layers
            .iter_mut()
            .zip(&mut entry.heard)
            .zip(decorations.declared.iter().zip(&decorations.listeners));
        for ((layer, heard), (decl, listener)) in ranks {
            moved |= !layer.is_empty() && layer.at() != text.edit_mark();
            // `Lost` clears the layer, a change `moved` already counts.
            if layer.catch_up(text).is_err() {
                heard.lose();
            }
            if let Some(listener) = listener.as_ref().and_then(Weak::upgrade)
                && let Some(change) = heard.hear(text)
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
