//! Inkra: a note editor on GPUI.
//!
//! This file is the bootstrap and nothing else. Editing semantics live in
//! `inkra-editor`, the text model in `inkra-text`, the editor view in
//! `editor/`, and the window's outer layout in `shell.rs`.

mod editor;
mod keymap;
mod shell;

use std::path::PathBuf;

use gpui::{App, Bounds, Focusable, WindowBounds, WindowOptions, actions, prelude::*, px, size};
use inkra_text::Buffer;
use inkra_vault::{Fs, RealFs};

use editor::EditorView;
use keymap::key_bindings;
use shell::AppShell;

actions!(inkra, [Quit]);

/// The note named on the command line, already loaded.
///
/// Reading the file before the window opens is deliberate: there is nothing to
/// show until it succeeds, and a failure should exit rather than flash an
/// empty window. Once the sidebar exists, loads move to a background task.
fn initial_text() -> Result<String, String> {
    // `args_os`, not `args`: the latter panics on an argument that is not valid
    // UTF-8, and a path on Unix need not be. `RelPath::display_name`
    // (`crates/vault/src/path.rs`) already treats such a name as renderable
    // rather than fatal; aborting here would contradict it.
    let Some(argument) = std::env::args_os().nth(1) else {
        // No argument: an empty buffer, per the launch-state table in
        // `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`. Nothing
        // renders in `check.sh` and a bare `cargo run` now exercises neither
        // per-line layout nor the scroll container, so the eyeball pass needs
        // a tall file named on the command line.
        return Ok(String::new());
    };
    let path = PathBuf::from(argument);
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| loaded.text)
        .map_err(|error| format!("{error}"))
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let text = match initial_text() {
            Ok(text) => text,
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(1);
            }
        };

        let bounds = Bounds::centered(None, size(px(600.0), px(320.0)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                let buffer = cx.new(|_| Buffer::new(text));
                let editor = cx.new(|cx| EditorView::new(buffer, cx));
                cx.new(|_| AppShell::new(editor))
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
