use gpui::Pixels;
use inkra_editor::Selection;
use inkra_text::{Bias, Buffer, ByteOffset};

use super::TopLine;

/// What a tab puts back on its note: the selection and the text on top (tabs spec, A2).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ViewState {
    pub(crate) selection: Selection,
    pub(crate) top: TopLine,
    /// Each scrolled block's start and offset; never saved to disk.
    pub(crate) scrolls: Vec<(ByteOffset, Pixels)>,
}

impl ViewState {
    /// Onto `buffer`'s graphemes, tail then head, so direction and goal survive.
    pub(super) fn clipped(self, buffer: &Buffer) -> Self {
        let clip = |offset| buffer.clip_grapheme(offset, Bias::Left);
        let mut selection = Selection::at(clip(self.selection.tail()));
        selection.set_head(clip(self.selection.head()), self.selection.goal());
        Self {
            selection,
            top: self.top.clipped(buffer),
            scrolls: self.scrolls,
        }
    }
}
