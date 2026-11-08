use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Context, Entity, Pixels, Subscription, TestAppContext, VisualTestContext, Window, prelude::*,
};
use inkra_text::{Buffer, ByteOffset};

use super::EditorView;
use crate::keymap::key_bindings;

mod clipboard;
mod code_block_anchors;
mod code_block_bar;
mod code_block_reveal;
mod code_block_scroll;
mod code_block_wrap;
mod code_blocks;
mod conceal;
mod dispatch;
mod follow;
mod gutter;
mod hanging;
mod headings;
mod inline_marker;
mod margin;
mod markdown;
mod paint;
mod protocol;
mod restore;
mod scroll;
mod shapes;
mod split;
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
        .update(cx, |view, window, cx| {
            window.focus(view.input.focus_handle(), cx);
        })
        .unwrap();
    let visual = VisualTestContext::from_window(window.into(), cx);
    (window, visual)
}

/// Set by a notify once the editor was painted: a draw asking for a second frame.
pub(super) fn notified_after_paint(
    editor: &Entity<EditorView>,
    cx: &mut VisualTestContext,
) -> (Subscription, Rc<Cell<bool>>) {
    let flag = Rc::new(Cell::new(false));
    let set = Rc::clone(&flag);
    let subscription = cx.update(|_, cx| {
        cx.observe(editor, move |editor, cx| {
            if editor.read(cx).input.last_bounds().is_some() {
                set.set(true);
            }
        })
    });
    (subscription, flag)
}

/// Releases the window's editor, so it forgets its block scrolls: the test harness drops
/// entities without releasing them.
pub(super) fn close(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}

pub(super) const TALL: usize = 200;

pub(super) fn tall_document() -> String {
    "line\n".repeat(TALL)
}

pub(super) fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

pub(super) fn assert_x(actual: Pixels, expected: f32) {
    let actual = f32::from(actual);
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

/// Written out rather than read back from the view: a derived expectation
/// would restate the implementation, so a padding or line-height regression would pass quietly.
pub(crate) mod geometry {
    /// The editor's `p_4()`.
    pub(crate) const PADDING: f32 = 16.;
    /// `###### ` headless: seven glyphs of 0.6 × 18 px, 75.6 rounded up.
    pub(crate) const HEADING_MARGIN: f32 = 76.;
    /// `theme::EDITOR_MARGIN_MIN`: what a narrow pane keeps on each side of the text.
    pub(crate) const MARGIN_FLOOR: f32 = 16.;
    pub(crate) const TEXT_TOP: f32 = PADDING;
    pub(crate) const LINE_HEIGHT: f32 = 28.;
    /// `theme::heading_metrics(1)`'s line height.
    pub(crate) const H1_LINE_HEIGHT: f32 = 44.;
    pub(crate) const VIEWPORT: f32 = 1080.;
    pub(crate) const MARGIN: f32 = 3. * LINE_HEIGHT;

    pub(crate) fn offset_showing_from_below(n: f32) -> gpui::Pixels {
        gpui::px(VIEWPORT - MARGIN - (TEXT_TOP + LINE_HEIGHT * (n + 1.)))
    }

    /// The offset `Reveal::Center` puts line `n`'s middle at the viewport's with.
    pub(crate) fn offset_centering(n: f32) -> gpui::Pixels {
        gpui::px(VIEWPORT / 2. - (TEXT_TOP + LINE_HEIGHT * (n + 0.5)))
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
