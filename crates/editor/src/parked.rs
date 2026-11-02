use inkra_text::{Anchor, Bias, Buffer, Version};

use crate::selection::Selection;

/// A selection kept as anchors while its view is not showing, biased as `EditorState::map_through`.
#[derive(Debug)]
pub struct ParkedSelection {
    start: Anchor,
    end: Anchor,
    captured: Selection,
    version: Version,
}

impl ParkedSelection {
    /// # Panics
    /// If an end of `selection` is past the end of `buffer` or splits a char.
    #[must_use]
    pub fn park(selection: &Selection, buffer: &mut Buffer) -> Self {
        Self {
            start: buffer.anchor(selection.start(), Bias::Right),
            end: buffer.anchor(selection.end(), Bias::Left),
            captured: selection.clone(),
            version: buffer.version(),
        }
    }

    /// # Panics
    /// If `buffer` is not the one `park` anchored in.
    #[must_use]
    pub fn unpark(self, buffer: &mut Buffer) -> Selection {
        let selection = self.peek(buffer);
        buffer.forget(self.start);
        buffer.forget(self.end);
        selection
    }

    /// As `unpark`, forgetting nothing: the selection stays parked.
    ///
    /// # Panics
    /// If `buffer` is not the one `park` anchored in.
    #[must_use]
    pub fn peek(&self, buffer: &Buffer) -> Selection {
        let range = buffer.resolve(self.start)..buffer.resolve(self.end);
        // The version alone is not enough: an undo can restore it with an anchor moved.
        if buffer.version() == self.version && range == self.captured.range() {
            self.captured.clone()
        } else {
            Selection::settled(range, self.captured.reversed())
        }
    }
}

#[cfg(test)]
mod tests;
