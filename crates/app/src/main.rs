//! Inkra: a note editor on GPUI.
//!
//! This file is the bootstrap and nothing else. Editing semantics live in
//! `inkra-editor`, the text model in `inkra-text`, and the view in `editor/`.

mod editor;
mod keymap;

use gpui::{App, Bounds, Focusable, WindowBounds, WindowOptions, actions, prelude::*, px, size};
use inkra_text::Buffer;

use editor::EditorView;
use keymap::key_bindings;

actions!(inkra, [Quit]);

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(600.0), px(320.0)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                // Multi-line, and taller than the window, so that starting the
                // app exercises per-line layout and the scroll container —
                // neither of which any test renders.
                let buffer = cx.new(|_| {
                    Buffer::new(
                        "héllo 😀 — type here\n\
                         Enter splits a line, up and down keep their column.\n\
                         \n\
                         An empty line above.\n\
                         A much longer line, so the ragged right edge has \
                         something to be ragged about.\n\
                         short\n\
                         six\nseven\neight\nnine\nten\neleven\ntwelve\n\
                         thirteen — past the window, so scrolling has to work",
                    )
                });
                cx.new(|cx| EditorView::new(buffer, cx))
            },
        );

        match window {
            Ok(window) => {
                let focused = window.update(cx, |view, window, cx| {
                    window.focus(&view.focus_handle(cx), cx);
                    cx.activate(true);
                });
                if let Err(err) = focused {
                    eprintln!("failed to focus window: {err}");
                }
            }
            Err(err) => {
                eprintln!("failed to open window: {err}");
                std::process::exit(1);
            }
        }
    });
}
