//! The platform text-input edge.
//!
//! `EntityInputHandler` speaks UTF-16 offsets throughout, so every method here
//! is a conversion around a call into `inkra-editor`. Both the conversion and
//! the protocol's precedence rules live below this layer — in `Buffer` and
//! `EditorState` — because `NameField` speaks the same protocol and a second
//! copy of either is a second thing to get wrong. What is left here is the two
//! methods that need pixels, and the leases the rest go through.

use std::ops::Range;
use std::time::Instant;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};

use super::EditorView;

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let buffer = self.buffer.read(cx);
        let range = buffer.from_utf16_range(&range_utf16);
        actual_range.replace(buffer.to_utf16_range(&range));
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
            range: buffer.to_utf16_range(&selection.range()),
            reversed: selection.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        let range = self.state.marked_range()?;
        Some(self.buffer.read(cx).to_utf16_range(&range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // Through the gate: dropping the composing region removes its underline,
        // so it owes a repaint like any other view-state change.
        self.update_state(cx, |state| state.set_marked_range(None));
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| {
            state.replace_in_range(buffer, now, range_utf16.as_ref(), new_text);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        self.edit_buffer(window, cx, |state, buffer| {
            state.mark_and_replace_in_range(
                buffer,
                now,
                range_utf16.as_ref(),
                new_text,
                new_selected_range_utf16,
            );
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.buffer.read(cx).from_utf16_range(&range_utf16);
        // `range_bounds` is element-relative and already clamped to the line
        // holding `range.start`; the platform wants window coordinates.
        let local = layout.range_bounds(&range);
        Some(Bounds::new(
            element_bounds.origin + local.origin,
            local.size,
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let offset = layout.offset_for_position(position - bounds.origin);
        let buffer = self.buffer.read(cx);
        Some(buffer.to_utf16(buffer.clip(offset)).0)
    }
}
