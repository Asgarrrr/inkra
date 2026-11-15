//! A layer and how far its listener has heard the buffer (decoration layer spec, D4 and D8).

use std::ops::Range;

use inkra_text::{Buffer, ByteOffset, EditMark, Splice};

use super::{Layer, Lost, Published};
use crate::Moved;

/// What a layer's listener has not heard of a buffer yet.
#[derive(Debug, PartialEq, Eq)]
pub enum TextChange {
    /// The listener has not heard of the buffer before; `fresh` when the buffer is still at the edit
    /// mark an open landed its chunks at (D8).
    Opened { fresh: bool },
    /// The splices since it last heard.
    Edited(Vec<Splice>),
    /// The layer's marks over the buffer are gone: it publishes them all again.
    Lost,
}

enum Heard {
    /// `opened`: the edit mark an open landed its chunks at, while they are whole.
    Unheard {
        opened: Option<EditMark>,
    },
    Lost,
    At(EditMark),
}

/// A feature's layer over one buffer, with how far its listener has heard that buffer.
pub struct HeardLayer<S> {
    layer: Layer<S>,
    heard: Heard,
}

impl<S: Clone + PartialEq> HeardLayer<S> {
    #[must_use]
    pub fn new(at: EditMark) -> Self {
        Self {
            layer: Layer::new(at),
            heard: Heard::Unheard { opened: None },
        }
    }

    #[must_use]
    pub fn layer(&self) -> &Layer<S> {
        &self.layer
    }

    /// As `Layer::catch_up`; `Shift` when the layer held marks behind `text`, which now draw
    /// elsewhere, `Whole` when it was lost.
    #[must_use]
    pub fn catch_up(&mut self, text: &Buffer) -> Option<Moved> {
        let shifts = self.layer.trails(text.edit_mark());
        let caught = self.layer.catch_up(text);
        self.track(&caught);
        match caught {
            Ok(()) => shifts.then_some(Moved::Shift),
            Err(Lost) => Some(Moved::Whole),
        }
    }

    /// As `Layer::publish`; its range at `text`'s edit mark, where the layer stands after, or
    /// `Whole` when it was lost.
    ///
    /// # Panics
    /// As `Layer::publish`.
    #[must_use]
    pub fn publish(
        &mut self,
        text: &Buffer,
        removed: Vec<Range<ByteOffset>>,
        chunks: Vec<Published<S>>,
        at: EditMark,
    ) -> Option<Moved> {
        let published = self.layer.publish(text, removed, chunks, at);
        self.track(&published);
        match published {
            Ok(range) => range.map(|range| Moved::Range(text.edit_mark(), range)),
            Err(Lost) => Some(Moved::Whole),
        }
    }

    /// Publishes an open's `chunks` at `text`'s edit mark, which a first hearing then compares.
    ///
    /// # Panics
    /// As `Layer::publish`.
    pub fn land(&mut self, text: &Buffer, chunks: Vec<Published<S>>) -> bool {
        let at = text.edit_mark();
        let landed = self.layer.publish(text, Vec::new(), chunks, at);
        if landed.is_ok()
            && let Heard::Unheard { opened } = &mut self.heard
        {
            *opened = Some(at);
        }
        self.track(&landed);
        landed != Ok(None)
    }

    /// What the listener has not heard of `text` yet, if anything; after, it has heard all of it.
    pub fn hear(&mut self, text: &Buffer) -> Option<TextChange> {
        let now = text.edit_mark();
        let change = match self.heard {
            Heard::Unheard { opened } => Some(TextChange::Opened {
                fresh: opened == Some(now),
            }),
            Heard::Lost => Some(TextChange::Lost),
            Heard::At(mark) if mark == now => None,
            Heard::At(mark) => Some(text.edits_since(mark).map_or(TextChange::Lost, |splices| {
                TextChange::Edited(splices.collect())
            })),
        };
        self.heard = Heard::At(now);
        change
    }

    /// Heard as lost either way, so the listener publishes it all again; off also clears the
    /// layer. `true` when that cleared a mark.
    pub fn switch(&mut self, on: bool) -> bool {
        self.heard = Heard::Lost;
        let cleared = !on && !self.layer.is_empty();
        if cleared {
            self.layer.clear();
        }
        cleared
    }

    /// A lost layer was cleared: a heard listener hears it lost, an unheard one's open is stale.
    fn track<T>(&mut self, result: &Result<T, Lost>) {
        if result.is_err() {
            self.heard = match self.heard {
                Heard::Unheard { .. } => Heard::Unheard { opened: None },
                Heard::Lost | Heard::At(_) => Heard::Lost,
            };
        }
    }
}

#[cfg(test)]
mod tests;
