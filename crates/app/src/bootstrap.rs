use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    App, AppContext as _, Bounds, Entity, Focusable, Pixels, Size, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
use inkra_text::Buffer;
use inkra_vault::{Fs, RealFs, Stamp, Vault};

use crate::Quit;
use crate::app_files::{cache_file, session_file};
use crate::editor::EditorView;
use crate::keymap::key_bindings;
use crate::restore::{self, Opened};
use crate::shell::AppShell;
use crate::theme::{self, store::Resolved};
use crate::timeline::{Step, Timeline};
use crate::ui::Assets;
use crate::vault::NoteStore;

pub(crate) const WINDOW_SIZE: Size<Pixels> = size(px(1000.), px(640.));

pub(crate) enum Launch {
    /// `AppShell` renders its empty state; there is no editor in this state.
    Nothing,
    /// The path travels for `⌘S` to write back to; the `Stamp` is what the
    /// first write compares against.
    Note(PathBuf, String, Stamp),
    /// Scanned behind the first frame; the session file, if the app keeps one, read before it;
    /// the index cache file, if the app keeps one, read beside the scan.
    Vault(Vault, Option<PathBuf>, Option<PathBuf>),
}

/// Takes the `Fs` rather than naming `RealFs` itself, so the off-screen
/// runner gets the same composition the real window gets.
pub(crate) fn root(
    fs: Arc<dyn Fs>,
    launched: Launch,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AppShell> {
    let (editor, notes, session, cache) = match launched {
        Launch::Nothing => (None, cx.new(|cx| NoteStore::new(fs, None, cx)), None, None),
        Launch::Note(path, text, stamp) => {
            let buffer = cx.new(|_| Buffer::new(text));
            let editor = cx.new(|cx| EditorView::new(buffer.clone(), cx));
            let notes = cx.new(|cx| NoteStore::detached(fs, buffer, path, stamp, cx));
            (Some(editor), notes, None, None)
        }
        Launch::Vault(vault, session, cache) => {
            // Read once, here: the first frame draws the saved layout, and the tabs' restore reuses it.
            let opened = session.map(|file| {
                let started = Instant::now();
                let session = futures::executor::block_on(restore::read(
                    Arc::clone(&fs),
                    file.clone(),
                    vault.root().to_path_buf(),
                ));
                Timeline::session_read(started.elapsed(), cx);
                Opened { file, session }
            });
            let vault = cx.new(|_| vault);
            (
                None,
                cx.new(|cx| NoteStore::new(fs, Some(vault), cx)),
                opened,
                cache,
            )
        }
    };
    cx.new(|cx| AppShell::new(editor, notes, session, cache, window, cx))
}

/// Lexical, never `canonicalize`: a symlinked vault keeps its own session (restore-tabs spec, R4).
fn vault_root(path: &Path) -> io::Result<PathBuf> {
    // `absolute` keeps a trailing slash; rebuilding from components drops it.
    Ok(std::path::absolute(path)?.components().collect())
}

/// No `HOME`, or a vault holding the app's folder: no session and no cache, so none read or written.
pub(crate) fn vault_launch(root: &Path, home: Option<&Path>) -> Launch {
    let session = home.and_then(|home| session_file(home, root));
    let cache = home.and_then(|home| cache_file(home, root));
    Launch::Vault(Vault::new(root), session, cache)
}

/// Deliberately blocking: a failure should exit rather than flash an empty window.
fn launch() -> Result<Launch, String> {
    // `args_os`, not `args`: the latter panics on a non-UTF-8 path.
    let Some(argument) = std::env::args_os().nth(1) else {
        return Ok(Launch::Nothing);
    };
    let path = PathBuf::from(argument);
    if path.is_dir() {
        let root = vault_root(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        // Opens the folder, lists nothing: a vault of any size costs the first frame nothing (open-first spec, O6).
        futures::executor::block_on(RealFs.check_dir(&root)).map_err(|error| format!("{error}"))?;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        return Ok(vault_launch(&root, home.as_deref()));
    }
    futures::executor::block_on(RealFs.load(&path))
        .map(|loaded| Launch::Note(path, loaded.text, loaded.stamp))
        .map_err(|error| format!("{error}"))
}

pub fn run() {
    crate::logger::init();
    let started = Instant::now();
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx: &mut App| {
        cx.set_global(Timeline::new(started));
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let launched = match launch() {
            Ok(launched) => launched,
            Err(message) => {
                log::error!("{message}");
                std::process::exit(1);
            }
        };

        let resolved = match &launched {
            Launch::Vault(vault, ..) => {
                futures::executor::block_on(theme::store::load(&RealFs, vault.root()))
            }
            Launch::Nothing | Launch::Note(..) => Resolved::default(),
        };
        theme::install(resolved.theme.clone(), cx);
        theme::set_code_block_wrap(resolved.code_block_wrap, cx);
        let bounds = Bounds::centered(None, WINDOW_SIZE, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(theme::TRAFFIC_LIGHT_POSITION),
                }),
                // The app moves the window from its drag regions; AppKit never drags it from a tab.
                app_owns_titlebar_drag: true,
                // The chrome is translucent over this blur; the note paints itself opaque.
                window_background: WindowBackgroundAppearance::Blurred,
                ..Default::default()
            },
            |window, cx| root(Arc::new(RealFs), launched, window, cx),
        );

        match window {
            Ok(window) => {
                let focused = window.update(cx, |view, window, cx| {
                    window.focus(&view.focus_handle(cx), cx);
                    cx.activate(true);
                    // Marks the first draw's start, ~4.5 ms before its present: any later hook waits on AppKit.
                    window.on_next_frame(|_, cx| Timeline::mark(Step::FirstFrame, cx));
                    view.adopt_theme(resolved, cx);
                });
                if let Err(err) = focused {
                    log::warn!("failed to focus window: {err}");
                }
            }
            Err(err) => {
                log::error!("failed to open window: {err}");
                std::process::exit(1);
            }
        }
    });
}

#[cfg(test)]
mod tests;
