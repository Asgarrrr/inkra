//! What happens before the window exists, and the composition it opens.
//!
//! `root` is the half both entry points share, and the reason it is a function
//! at all: the off-screen runner builds the window without going anywhere near
//! `std::env::args_os`. `launch` and `run` are the binary's alone.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Bounds, Entity, Focusable, Pixels, Size, WindowBounds, WindowOptions, px,
    size,
};
use inkra_text::Buffer;
use inkra_vault::{Fs, RealFs, Stamp, Vault};

use crate::Quit;
use crate::editor::EditorView;
use crate::keymap::key_bindings;
use crate::shell::AppShell;
use crate::vault::NoteStore;

/// The window a first launch gets. The previous 600x320 predates the sidebar:
/// it left 420 px of editor beside it, about eleven lines of body text. Not
/// persisted — restoring the last size and position is deferred, with the rest
/// of the window state.
pub(crate) const WINDOW_SIZE: Size<Pixels> = size(px(1000.), px(640.));

/// What the command-line argument named, already read.
pub(crate) enum Launch {
    /// No argument: no note and no vault, per the launch-state table in
    /// `docs/superpowers/specs/2026-09-20-inkra-vault-design.md`. There is no
    /// editor at all in this state — `AppShell` renders its empty state — so a
    /// bare `cargo run` is the weakest way to look at the app; the `shot` runner
    /// covers it as `empty-no-vault` and the richer states beside it.
    Nothing,
    /// One file, with no vault behind it. The path travels with the text
    /// because `⌘S` needs somewhere to write back to, and the `Stamp` because
    /// the first write of this note compares against it — a note adopted
    /// without one could not tell its own file from a changed one.
    Note(PathBuf, String, Stamp),
    /// A directory: the vault, scanned. No note is open until a row is clicked.
    Vault(Vault),
}

/// Builds the window's root view from an already-resolved launch state.
///
/// Takes the `Fs` rather than naming `RealFs` itself. That is the whole reason
/// this is not inlined into `run`: the off-screen runner hands it a `MemFs` and
/// gets the same composition the real window gets, rather than a second one
/// that could drift from it.
pub(crate) fn root(fs: Arc<dyn Fs>, launched: Launch, cx: &mut App) -> Entity<AppShell> {
    // Only a named file gets a buffer and an editor here. The other two states
    // used to get one too, untracked by `NoteStore` and therefore unsaveable by
    // any route; `AppShell` renders an empty state for them instead, so there
    // is nothing to type into and nothing to lose.
    let (note, scanned) = match launched {
        Launch::Nothing => (None, None),
        Launch::Note(path, text, stamp) => (Some((path, text, stamp)), None),
        Launch::Vault(vault) => (None, Some(vault)),
    };
    let opened = note.map(|(path, text, stamp)| {
        let buffer = cx.new(|_| Buffer::new(text));
        let editor = cx.new(|cx| EditorView::new(buffer.clone(), cx));
        (buffer, editor, path, stamp)
    });
    let vault = scanned.map(|vault| cx.new(|_| vault));
    let editor = opened.as_ref().map(|(_, editor, _, _)| editor.clone());
    let notes = cx.new(|cx| {
        let mut store = NoteStore::new(fs, vault, cx);
        if let Some((buffer, _, path, stamp)) = opened {
            store.adopt(buffer, path, stamp, cx);
        }
        store
    });
    cx.new(|cx| AppShell::new(editor, notes, cx))
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
        .map(|loaded| Launch::Note(path, loaded.text, loaded.stamp))
        .map_err(|error| format!("{error}"))
}

/// The binary's entry point. `main.rs` is a trampoline onto this.
pub fn run() {
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

        let bounds = Bounds::centered(None, WINDOW_SIZE, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| root(Arc::new(RealFs), launched, cx),
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
