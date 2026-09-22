//! Tests that need the gpui edge.
//!
//! Editing semantics are tested in `inkra-editor` without a window. What is
//! left here is what only a window can prove: the UTF-16 protocol conversions
//! and input-method composition (`protocol.rs`), real keystrokes reaching the
//! buffer (`dispatch.rs`), scroll and click geometry (`scroll.rs`), a swapped
//! or shared buffer (`swap.rs`), and the system clipboard (`clipboard.rs`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use gpui::{Context, Entity, TestAppContext, VisualTestContext, Window, prelude::*};
use inkra_text::{Buffer, ByteOffset};

use super::EditorView;
use crate::keymap::key_bindings;

mod clipboard;
mod dispatch;
mod protocol;
mod scroll;
mod swap;

/// Drives the entity through a headless window, the only way to reach
/// `EntityInputHandler`, whose methods all require a `Window`.
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

/// A focused window with the real key bindings installed, for tests that drive
/// dispatch rather than calling handlers directly.
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
