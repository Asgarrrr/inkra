//! The harness a scene drives: one off-screen window, and the few gestures a
//! scene needs to put it in the state worth looking at.
//!
//! Deliberately small. A scene that needs a gesture this does not offer adds it
//! here, in the same change, so what the runner can express stays readable in
//! one file rather than accumulating inside scene bodies.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Focusable, Pixels, Point, VisualTestAppContext, WindowHandle};
use image::RgbaImage;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Vault};

use crate::Quit;
use crate::bootstrap::{Launch, WINDOW_SIZE, root};
use crate::keymap::key_bindings;
use crate::shell::AppShell;

/// Where a scene's vault lives. Only `MemFs` is ever mounted here, so no shot
/// can touch the real filesystem however wrong a scene is.
const VAULT_ROOT: &str = "/vault";

pub(crate) struct Stage {
    cx: VisualTestAppContext,
}

impl Stage {
    /// Mirrors what `bootstrap::run` installs before opening its window. Both
    /// halves, or neither: `key_bindings` binds `cmd-q` to `Quit`
    /// (`crates/app/src/keymap.rs`), so bindings without the handler give a
    /// scene a silent no-op where the real app quits — a state the app never
    /// reaches, rendered as though it did.
    pub(crate) fn new() -> Self {
        let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
        cx.update(|cx| {
            cx.bind_keys(key_bindings());
            cx.on_action(|_: &Quit, cx| cx.quit());
        });
        Self { cx }
    }

    /// The launch-with-no-argument state: an empty buffer and no vault.
    pub(crate) fn open_nothing(&mut self) -> WindowHandle<AppShell> {
        self.open(Arc::new(MemFs::new()), Launch::Nothing)
    }

    /// One note, no vault behind it — what a launch on a single file gets.
    ///
    /// Loads through `Fs::load` rather than inventing a `Stamp`, so the note
    /// carries the one its file actually has, as the real launch does.
    pub(crate) fn open_note(&mut self, text: &str) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        let path = PathBuf::from(VAULT_ROOT).join("note.md");
        fs.insert_file(path.clone(), text);
        let loaded = futures::executor::block_on(fs.load(&path))
            .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
        self.open(fs, Launch::Note(path, loaded.text, loaded.stamp))
    }

    /// A scanned vault of the given `(relative path, contents)` pairs.
    ///
    /// No note is open: a vault launch shows the tree and an empty editor until
    /// a row is clicked.
    pub(crate) fn open_vault(&mut self, files: &[(&str, &str)]) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new(VAULT_ROOT)))
            .unwrap_or_else(|error| panic!("scan {VAULT_ROOT}: {error}"));
        self.open(fs, Launch::Vault(scanned))
    }

    /// The one door to a window, so every scene gets `WINDOW_SIZE` rather than
    /// a literal of its own. A second literal stops matching the real window
    /// the day that constant moves, and nothing catches it.
    fn open(&mut self, fs: Arc<dyn Fs>, launched: Launch) -> WindowHandle<AppShell> {
        let window = self
            .cx
            .open_offscreen_window(WINDOW_SIZE, |_, cx| root(fs, launched, cx))
            .unwrap_or_else(|error| panic!("open the off-screen window: {error}"));
        self.settle();
        window
    }

    /// Gives the editor keyboard focus.
    ///
    /// Explicit, never automatic: `open_offscreen_window` passes `focus: false`
    /// (`gpui/src/app/visual_test_context.rs:113`), and an unfocused window is
    /// itself a state worth shooting. A scene says which one it wants.
    pub(crate) fn focus(&mut self, window: WindowHandle<AppShell>) {
        window
            .update(&mut self.cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
            })
            .unwrap_or_else(|error| panic!("focus the window: {error}"));
        self.settle();
    }

    /// Dispatches keystrokes, space-separated, in gpui's notation: `"cmd-a"`,
    /// `"shift-down shift-down"`.
    pub(crate) fn keys(&mut self, window: WindowHandle<AppShell>, keystrokes: &str) {
        self.cx.simulate_keystrokes(window.into(), keystrokes);
        self.settle();
    }

    /// Clicks at a point in window coordinates.
    pub(crate) fn click(&mut self, window: WindowHandle<AppShell>, at: Point<Pixels>) {
        self.cx
            .simulate_click(window.into(), at, gpui::Modifiers::default());
        self.settle();
    }

    /// Renders the window's last frame and reads the pixels back, in *device*
    /// pixels — a 1000x640 window yields 2000x1280 on a 2x display. Not
    /// normalised, because nothing compares shots across machines yet.
    pub(crate) fn capture(&mut self, window: WindowHandle<AppShell>) -> RgbaImage {
        self.settle();
        self.cx
            .capture_screenshot(window.into())
            .unwrap_or_else(|error| panic!("capture the frame: {error}"))
    }

    /// Runs every pending task, so a frame is never captured mid-load.
    fn settle(&mut self) {
        self.cx.run_until_parked();
    }
}
