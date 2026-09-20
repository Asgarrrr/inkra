//! The platform text-input edge.
//!
//! `EntityInputHandler` speaks UTF-16 offsets throughout, so every method here
//! is a conversion around a call into `inkra-editor`. The conversion itself
//! lives in `inkra-text`; what lives here is the protocol's precedence rules
//! and its caret arithmetic.

use std::ops::Range;
use std::time::Instant;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window, point};
use inkra_text::{Buffer, ByteOffset, Utf16Offset};

use super::EditorView;

impl EditorView {
    fn range_to_utf16(buffer: &Buffer, range: &Range<ByteOffset>) -> Range<usize> {
        buffer.to_utf16(range.start).0..buffer.to_utf16(range.end).0
    }

    fn range_from_utf16(buffer: &Buffer, range: &Range<usize>) -> Range<ByteOffset> {
        let start = buffer.from_utf16(Utf16Offset(range.start));
        let end = buffer.from_utf16(Utf16Offset(range.end));
        start.min(end)..start.max(end)
    }

    /// The range an input-method edit applies to, following GPUI's precedence:
    /// explicit range, else the composing range, else the selection.
    fn target_range(
        &self,
        buffer: &Buffer,
        range_utf16: Option<&Range<usize>>,
    ) -> Range<ByteOffset> {
        range_utf16
            .map(|range| Self::range_from_utf16(buffer, range))
            .or_else(|| self.state.marked_range())
            .unwrap_or_else(|| self.state.selection().range())
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.buffer.read(cx);
        let range = Self::range_from_utf16(buffer, &range_utf16);
        actual_range.replace(Self::range_to_utf16(buffer, &range));
        Some(buffer.text()[range.start.0..range.end.0].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let buffer = self.buffer.read(cx);
        let selection = self.state.selection();
        Some(UTF16Selection {
            range: Self::range_to_utf16(buffer, &selection.range()),
            reversed: selection.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.state.marked_range()?;
        Some(Self::range_to_utf16(self.buffer.read(cx), &range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.state.set_marked_range(None);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.target_range(self.buffer.read(cx), range_utf16.as_ref());
        let now = Instant::now();
        self.edit_buffer(cx, |state, buffer| state.edit(buffer, now, range, new_text));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.target_range(self.buffer.read(cx), range_utf16.as_ref());
        let start = range.start;
        let now = Instant::now();

        self.edit_buffer(cx, |state, buffer| {
            state.edit(buffer, now, range, new_text);

            state.set_marked_range(
                (!new_text.is_empty()).then(|| start..ByteOffset(start.0 + new_text.len())),
            );

            // The input method reports the caret in UTF-16 units relative to
            // the text it just inserted. Rebase it while still in UTF-16 and
            // convert once: converting first would resolve it against the whole
            // buffer, and adding `start` on top would then count the offset
            // twice.
            if let Some(selected) = new_selected_range_utf16 {
                let inserted_at = buffer.to_utf16(start).0;
                let cursor = buffer.from_utf16(Utf16Offset(inserted_at + selected.end));
                state.move_to(buffer, cursor, false);
            }
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = Self::range_from_utf16(self.buffer.read(cx), &range_utf16);
        Some(Bounds::from_corners(
            point(
                element_bounds.left() + line.x_for_index(range.start.0),
                element_bounds.top(),
            ),
            point(
                element_bounds.left() + line.x_for_index(range.end.0),
                element_bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(position.x - bounds.left())?;
        let buffer = self.buffer.read(cx);
        Some(buffer.to_utf16(buffer.clip(ByteOffset(index))).0)
    }
}
