#![cfg_attr(test, allow(clippy::unwrap_used))]

use gpui::{Context, Entity, TestAppContext, VisualTestContext, Window, prelude::*};
use inkra_text::{Buffer, ByteOffset};

use super::EditorView;
use crate::keymap::key_bindings;

mod clipboard;
mod conceal;
mod dispatch;
mod headings;
mod markdown;
mod protocol;
mod scroll;
mod swap;
mod wrap;

pub(super) fn with_editor<R>(
    cx: &mut TestAppContext,
    text: &str,
    f: impl FnOnce(&mut EditorView, &mut Window, &mut Context<EditorView>) -> R,
) -> R {
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    window_on(cx, buffer).update(cx, f).unwrap()
}

pub(super) fn window_on(
    cx: &mut TestAppContext,
    buffer: Entity<Buffer>,
) -> gpui::WindowHandle<EditorView> {
    cx.add_window(move |_, cx| EditorView::new(buffer, cx))
}

pub(super) fn dispatching(
    cx: &mut TestAppContext,
    text: &str,
) -> (gpui::WindowHandle<EditorView>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let buffer = cx.update(|cx| cx.new(|_| Buffer::new(text)));
    let window = window_on(cx, buffer);
    window
        .update(cx, |view, window, cx| window.focus(&view.focus_handle, cx))
        .unwrap();
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

pub(super) fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

/// Written out rather than read back from the view: a derived expectation
/// would restate the implementation, so a padding or line-height regression would pass quietly.
pub(crate) mod geometry {
    /// The editor's `p_4()`.
    pub(crate) const PADDING: f32 = 16.;
    pub(crate) const TEXT_TOP: f32 = PADDING;
    pub(crate) const LINE_HEIGHT: f32 = 28.;
    pub(crate) const VIEWPORT: f32 = 1080.;
    pub(crate) const MARGIN: f32 = 3. * LINE_HEIGHT;

    pub(crate) fn offset_showing_from_below(n: f32) -> gpui::Pixels {
        gpui::px(VIEWPORT - MARGIN - (TEXT_TOP + LINE_HEIGHT * (n + 1.)))
    }

    pub(crate) fn top_of_line(n: f32, offset: gpui::Pixels) -> gpui::Pixels {
        gpui::px(TEXT_TOP + LINE_HEIGHT * n) + offset
    }

    pub(crate) fn line_at(y: gpui::Pixels, offset: gpui::Pixels) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let line = f32::from(y - offset - gpui::px(TEXT_TOP)) / LINE_HEIGHT;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let line = line as usize;
        line
    }

    pub(crate) fn line_is_visible(n: f32, offset: gpui::Pixels) -> bool {
        let top = top_of_line(n, offset);
        top >= gpui::px(0.) && top + gpui::px(LINE_HEIGHT) <= gpui::px(VIEWPORT)
    }
}
