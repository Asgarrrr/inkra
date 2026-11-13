//! Styled ranges features publish over a buffer, one layer per declared `LayerDecl`, drawn by
//! every view of that buffer (decoration layer spec).
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only tests declare and publish until a feature does"
    )
)]

use std::collections::HashMap;
use std::ops::Range;

use gpui::{App, BorrowAppContext, Entity, EntityId, Global, WeakEntity};
use inkra_editor::Layer;
use inkra_text::{Buffer, ByteOffset, EditMark};

use crate::feature::FeatureId;
use crate::theme::ColorToken;

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
}

impl Global for Decorations {}

struct Entry {
    buffer: WeakEntity<Buffer>,
    /// Moves on every change a view must draw.
    revision: u64,
    /// One per declared layer, by rank.
    layers: Vec<Layer<MarkStyle>>,
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
    decorations.declared = layers;
    decorations.entries.clear();
}

/// As `Layer::publish`, on `layer` over `buffer`; redraws every window when the marks change.
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
    let rank = cx
        .try_global::<Decorations>()
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
        });
    let changed = cx.update_global(|decorations: &mut Decorations, cx| {
        let text = buffer.read(cx);
        let id = buffer.entity_id();
        if !decorations.entries.contains_key(&id) {
            decorations
                .entries
                .retain(|_, entry| entry.buffer.is_upgradable());
        }
        let declared = decorations.declared.len();
        let entry = decorations.entries.entry(id).or_insert_with(|| Entry {
            buffer: buffer.downgrade(),
            revision: 0,
            layers: (0..declared)
                .map(|_| Layer::new(text.edit_mark()))
                .collect(),
        });
        // A lost layer was cleared: what it drew is gone too.
        let changed = entry.layers[rank].publish(text, chunk, marks, at) != Ok(false);
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
pub(crate) fn sync(buffer: &Entity<Buffer>, cx: &mut App) {
    if cx
        .try_global::<Decorations>()
        .is_none_or(|decorations| decorations.declared.is_empty())
    {
        return;
    }
    cx.update_global(|decorations: &mut Decorations, cx| {
        let Some(entry) = decorations.entries.get_mut(&buffer.entity_id()) else {
            return;
        };
        let text = buffer.read(cx);
        let mut moved = false;
        for layer in &mut entry.layers {
            moved |= !layer.is_empty() && layer.at() != text.edit_mark();
            // `Lost` clears the layer, a change `moved` already counts.
            let _ = layer.catch_up(text);
        }
        if moved {
            decorations.revisions += 1;
            entry.revision = decorations.revisions;
        }
    });
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
