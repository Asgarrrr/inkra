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

pub(crate) const WINDOW_SIZE: Size<Pixels> = size(px(1000.), px(640.));

pub(crate) enum Launch {
    /// `AppShell` renders its empty state; there is no editor in this state.
    Nothing,
    /// The path travels for `⌘S` to write back to; the `Stamp` is what the
    /// first write compares against.
    Note(PathBuf, String, Stamp),
    Vault(Vault),
}

/// Takes the `Fs` rather than naming `RealFs` itself, so the off-screen
/// runner gets the same composition the real window gets.
pub(crate) fn root(fs: Arc<dyn Fs>, launched: Launch, cx: &mut App) -> Entity<AppShell> {
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

/// Deliberately blocking: a failure should exit rather than flash an empty window.
fn launch() -> Result<Launch, String> {
    // `args_os`, not `args`: the latter panics on a non-UTF-8 path.
    let Some(argument) = std::env::args_os().nth(1) else {
        return Ok(Launch::Nothing);
    };
    let path = PathBuf::from(argument);
    if path.is_dir() {
        // `Vault::scan` skips an unreadable directory rather than
        // reporting it; read it once here to say why instead.
        futures::executor::block_on(RealFs.read_dir(&path)).map_err(|error| format!("{error}"))?;
        return futures::executor::block_on(Vault::scan(&RealFs, &path))
            .map(Launch::Vault)
            .map_err(|error| format!("{error}"));
    }
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| Launch::Note(path, loaded.text, loaded.stamp))
        .map_err(|error| format!("{error}"))
}

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
