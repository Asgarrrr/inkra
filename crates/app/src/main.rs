//! Inkra: a note editor on GPUI.
//!
//! This file is the bootstrap and nothing else. Editing semantics live in
//! `inkra-editor`, the text model in `inkra-text`, the editor view in
//! `editor/`, and the window's outer layout in `shell.rs`.

mod editor;
mod keymap;
mod shell;
mod sidebar;
mod vault;

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Bounds, Focusable, WindowBounds, WindowOptions, actions, prelude::*, px, size};
use inkra_text::Buffer;
use inkra_vault::{Fs, RealFs, Vault};

use editor::EditorView;
use keymap::key_bindings;
use shell::AppShell;
use vault::NoteStore;

actions!(inkra, [Quit]);

/// What the command-line argument named, already read.
enum Launch {
    /// No argument: an empty buffer and no vault, per the launch-state table in
    /// `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`. Nothing
    /// renders in `check.sh` and a bare `cargo run` exercises neither per-line
    /// layout nor the scroll container, so the eyeball pass needs an argument.
    Nothing,
    /// One file, with no vault behind it. The path travels with the text
    /// because `⌘S` needs somewhere to write back to.
    Note(PathBuf, String),
    /// A directory: the vault, scanned. No note is open until a row is clicked.
    Vault(Vault),
}

/// Reads whatever the argument names before the window opens.
///
/// Deliberately blocking: there is nothing to show until it succeeds, and a
/// failure should exit rather than flash an empty window. Loads after this one
/// go through `NoteStore::open_note`, on a background task.
fn launch() -> Result<Launch, String> {
    // `args_os`, not `args`: the latter panics on an argument that is not valid
    // UTF-8, and a path on Unix need not be. `RelPath::display_name`
    // (`crates/vault/src/path.rs`) already treats such a name as renderable
    // rather than fatal; aborting here would contradict it.
    let Some(argument) = std::env::args_os().nth(1) else {
        return Ok(Launch::Nothing);
    };
    let path = PathBuf::from(argument);
    if path.is_dir() {
        // Read the root once before scanning it. `Vault::scan` skips a
        // directory it cannot read rather than reporting it, so without this an
        // unreadable vault opens an empty window instead of saying why — and
        // an empty tree is a legitimate state for a vault with no notes in it,
        // so the two are indistinguishable afterwards.
        futures::executor::block_on(RealFs.read_dir(&path)).map_err(|error| format!("{error}"))?;
        return futures::executor::block_on(Vault::scan(&RealFs, &path))
            .map(Launch::Vault)
            .map_err(|error| format!("{error}"));
    }
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| Launch::Note(path, loaded.text))
        .map_err(|error| format!("{error}"))
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let launched = match launch() {
            Ok(launched) => launched,
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
                let (text, note, scanned) = match launched {
                    Launch::Nothing => (String::new(), None, None),
                    Launch::Note(path, text) => (text, Some(path), None),
                    Launch::Vault(vault) => (String::new(), None, Some(vault)),
                };
                let buffer = cx.new(|_| Buffer::new(text));
                let editor = cx.new(|cx| EditorView::new(buffer.clone(), cx));
                let vault = scanned.map(|vault| cx.new(|_| vault));
                let notes = cx.new(|_| {
                    let mut store = NoteStore::new(Arc::new(RealFs), vault);
                    if let Some(path) = note {
                        store.adopt(buffer, path);
                    }
                    store
                });
                cx.new(|cx| AppShell::new(editor, notes, cx))
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
