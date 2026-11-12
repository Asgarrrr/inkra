use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    App, AppContext as _, Bounds, Context, DisplayId, Entity, Focusable, Global, Pixels, Size,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, point, px,
    size,
};
use inkra_text::Buffer;
use inkra_vault::{Fs, RealFs, Stamp, Vault, VaultError};

use crate::Quit;
use crate::app_files::{cache_file, session_file};
use crate::editor::EditorView;
use crate::keymap::key_bindings;
use crate::last_vault;
use crate::restore::{
    self, Opened, SavedFileTab, SavedGroup, SavedMember, SavedTab, SavedWindow, Session,
};
use crate::shell::{AppShell, Left, couldnt_open};
use crate::sidebar::title_of;
use crate::theme::{self, store::Resolved};
use crate::timeline::{Step, Timeline};
use crate::ui::Assets;
use crate::vault::NoteStore;

pub(crate) const WINDOW_SIZE: Size<Pixels> = size(px(1000.), px(640.));

/// The user's home, set by `run` only when `HOME` is known.
pub(crate) struct Home(pub(crate) PathBuf);

impl Global for Home {}

pub(crate) fn home(cx: &App) -> Option<PathBuf> {
    cx.try_global::<Home>().map(|home| home.0.clone())
}

pub(crate) enum Launch {
    /// `AppShell` renders its empty state, with `notice` if the remembered vault failed; no editor.
    Nothing { notice: Option<String> },
    /// The path travels for `⌘S` to write back to; the `Stamp` is what the
    /// first write compares against.
    Note(PathBuf, String, Stamp),
    /// Scanned behind the first frame; the session file, if the app keeps one, read before it;
    /// the index cache file, if the app keeps one, read beside the scan.
    Vault(Vault, Option<PathBuf>, Option<PathBuf>),
}

/// Read once, before the window opens: its frame places the window, the first frame draws the saved layout,
/// and the tabs' restore reuses it.
pub(crate) fn read_session(fs: &Arc<dyn Fs>, launched: &Launch, cx: &mut App) -> Option<Opened> {
    let started = Instant::now();
    let opened = futures::executor::block_on(opened(Arc::clone(fs), launched))?;
    Timeline::session_read(started.elapsed(), cx);
    Some(opened)
}

/// The session `launched` keeps, read; `None` when it keeps none.
pub(crate) async fn opened(fs: Arc<dyn Fs>, launched: &Launch) -> Option<Opened> {
    let Launch::Vault(vault, Some(file), _) = launched else {
        return None;
    };
    let session = restore::read(fs, file.clone(), vault.root().to_path_buf()).await;
    Some(Opened {
        file: file.clone(),
        session,
    })
}

/// The saved frame on its display while that display is plugged in and the frame overlaps it; else centred on the main one.
pub(crate) fn placement(
    saved: Option<&SavedWindow>,
    cx: &App,
) -> (Option<DisplayId>, Bounds<Pixels>) {
    let on_display = saved.and_then(|saved| {
        let display = cx.displays().into_iter().find(|display| {
            display
                .uuid()
                .is_ok_and(|uuid| uuid.to_string() == saved.display)
        })?;
        let frame = Bounds::new(
            point(px(saved.x), px(saved.y)),
            size(px(saved.width), px(saved.height)),
        );
        display
            .bounds()
            .intersects(&frame)
            .then(|| (Some(display.id()), frame))
    });
    on_display.unwrap_or_else(|| (None, Bounds::centered(None, WINDOW_SIZE, cx)))
}

pub(crate) fn root(
    fs: Arc<dyn Fs>,
    launched: Launch,
    opened: Option<Opened>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<AppShell> {
    cx.new(|cx| build_shell(fs, launched, opened, Left::default(), window, cx))
}

/// Takes the `Fs` rather than naming `RealFs` itself, so the off-screen
/// runner gets the same composition the real window gets.
pub(crate) fn build_shell(
    fs: Arc<dyn Fs>,
    launched: Launch,
    opened: Option<Opened>,
    left: Left,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) -> AppShell {
    let (editor, notes, cache, notice) = match launched {
        Launch::Nothing { notice } => (
            None,
            cx.new(|cx| NoteStore::new(fs, None, cx)),
            None,
            notice,
        ),
        Launch::Note(path, text, stamp) => {
            let buffer = cx.new(|_| Buffer::new(text));
            let editor = cx.new(|cx| EditorView::new(buffer.clone(), cx));
            let notes = cx.new(|cx| NoteStore::detached(fs, buffer, path, stamp, cx));
            (Some(editor), notes, None, None)
        }
        Launch::Vault(vault, _, cache) => {
            let vault = cx.new(|_| vault);
            (
                None,
                cx.new(|cx| NoteStore::new(fs, Some(vault), cx)),
                cache,
                None,
            )
        }
    };
    let mut shell = AppShell::new(editor, notes, opened, cache, left, window, cx);
    if let Some(text) = notice {
        shell.show_notice(text, None, cx);
    }
    shell
}

/// Lexical, never `canonicalize`: a symlinked vault keeps its own session (restore-tabs spec, R4).
pub(crate) fn vault_root(path: &Path) -> Result<PathBuf, VaultError> {
    // `absolute` keeps a trailing slash; rebuilding from components drops it.
    std::path::absolute(path)
        .map(|absolute| absolute.components().collect())
        .map_err(|source| VaultError::Io {
            path: path.to_path_buf(),
            source,
        })
}

/// Refuses a root holding the app's folders when `HOME` is known; with no `HOME`, a vault without session or cache.
pub(crate) fn vault_launch(root: &Path, home: Option<&Path>) -> Result<Launch, String> {
    let Some(home) = home else {
        return Ok(Launch::Vault(Vault::new(root), None, None));
    };
    match (session_file(home, root), cache_file(home, root)) {
        (Some(session), Some(cache)) => {
            Ok(Launch::Vault(Vault::new(root), Some(session), Some(cache)))
        }
        _ => Err(format!(
            "“{}” holds Inkra's own files. Choose a folder inside it.",
            title_of(root)
        )),
    }
}

/// Deliberately blocking: a failed argument should exit rather than flash an empty window.
fn launch(home: Option<&Path>) -> Result<Launch, String> {
    // `args_os`, not `args`: the latter panics on a non-UTF-8 path.
    let Some(argument) = std::env::args_os().nth(1) else {
        return Ok(home.map_or(Launch::Nothing { notice: None }, |home| {
            remembered(&RealFs, home)
        }));
    };
    open_argument(&RealFs, PathBuf::from(argument), home)
}

/// Never through `open_argument`: a remembered path opens no note and its failure never exits (D5).
fn remembered(fs: &impl Fs, home: &Path) -> Launch {
    let Some(path) = futures::executor::block_on(last_vault::read(fs, home)) else {
        return Launch::Nothing { notice: None };
    };
    remembered_vault(fs, &path, home).unwrap_or_else(|notice| Launch::Nothing {
        notice: Some(notice),
    })
}

/// The remembered folder as a vault, or the notice saying why not.
fn remembered_vault(fs: &impl Fs, path: &Path, home: &Path) -> Result<Launch, String> {
    futures::executor::block_on(fs.check_dir(path)).map_err(|error| match error.io_kind() {
        Some(io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
            format!("The last vault, “{}”, is no longer there.", title_of(path))
        }
        _ => couldnt_open(path, &error),
    })?;
    let root = vault_root(path).map_err(|error| couldnt_open(path, &error))?;
    vault_launch(&root, Some(home))
}

/// A folder is a vault, anything else a note; `fs` tells them apart, so a `MemFs` test sees the choice.
fn open_argument(fs: &impl Fs, path: PathBuf, home: Option<&Path>) -> Result<Launch, String> {
    // Opens the folder, lists nothing: a vault of any size costs the first frame nothing (open-first spec, O6).
    match futures::executor::block_on(fs.check_dir(&path)) {
        Ok(()) => {
            let root = vault_root(&path).map_err(|error| error.to_string())?;
            vault_launch(&root, home)
        }
        // A file, or nothing there: the note's load reports which.
        Err(error)
            if matches!(
                error.io_kind(),
                Some(io::ErrorKind::NotADirectory | io::ErrorKind::NotFound)
            ) =>
        {
            futures::executor::block_on(fs.load(&path))
                .map(|loaded| Launch::Note(path, loaded.text, loaded.stamp))
                .map_err(|error| format!("{error}"))
        }
        Err(error) => Err(format!("{error}")),
    }
}

/// Writes the session that opens `note`, vault-relative, on the next launch of `vault` under `home`.
///
/// # Errors
///
/// A message naming the path, when the vault's root, the session's place or the write fails.
pub fn seed_session(vault: &Path, note: &str, home: &Path) -> Result<(), String> {
    seed_session_in(&RealFs, vault, note, home)
}

pub(crate) fn seed_session_in(
    fs: &impl Fs,
    vault: &Path,
    note: &str,
    home: &Path,
) -> Result<(), String> {
    let root = vault_root(vault).map_err(|error| error.to_string())?;
    let file = session_file(home, &root)
        .ok_or_else(|| format!("{}: the vault holds the app's folder", root.display()))?;
    let center = SavedMember::Group(SavedGroup {
        tabs: vec![SavedTab::File(SavedFileTab {
            file: Some(note.to_owned()),
            view: None,
        })],
        active: 0,
    });
    let text = Session::new(root, 0, center, None, None, [true, true])
        .encode()
        .map_err(|error| format!("{}: {error}", file.display()))?;
    futures::executor::block_on(fs.replace_bytes(&file, text.as_bytes()))
        .map_err(|error| format!("{}: {error}", file.display()))
}

pub fn run() {
    crate::logger::init();
    let started = Instant::now();
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx: &mut App| {
        cx.bind_keys(key_bindings());
        cx.on_action(|_: &Quit, cx| cx.quit());

        let home = std::env::var_os("HOME").map(PathBuf::from);
        let launched = match launch(home.as_deref()) {
            Ok(launched) => launched,
            Err(message) => {
                log::error!("{message}");
                std::process::exit(1);
            }
        };
        if let Some(home) = home {
            cx.set_global(Home(home));
        }
        // Only a launched vault is timed: an open from no vault would count the picker's time.
        if matches!(launched, Launch::Vault(..)) {
            cx.set_global(Timeline::new(started));
        }

        let resolved = match &launched {
            Launch::Vault(vault, ..) => {
                futures::executor::block_on(theme::store::load(&RealFs, vault.root()))
            }
            Launch::Nothing { .. } | Launch::Note(..) => Resolved::default(),
        };
        theme::install(resolved.theme.clone(), cx);
        theme::set_code_block_wrap(resolved.code_block_wrap, cx);
        let fs: Arc<dyn Fs> = Arc::new(RealFs);
        let opened = read_session(&fs, &launched, cx);
        let saved = opened
            .as_ref()
            .and_then(|opened| opened.session.as_ref()?.window.as_ref());
        let (display_id, bounds) = placement(saved, cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
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
            |window, cx| root(fs, launched, opened, window, cx),
        );

        match window {
            Ok(window) => {
                let focused = window.update(cx, |view, window, cx| {
                    window.focus(&view.focus_handle(cx), cx);
                    cx.activate(true);
                    // Marks the first draw's start, ~4.5 ms before its present: any later hook waits on AppKit.
                    window.on_next_frame(|_, cx| Timeline::mark(Step::FirstFrame, cx));
                    view.adopt_theme(resolved, cx);
                    view.remember_vault(cx);
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
