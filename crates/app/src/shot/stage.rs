use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Focusable, Modifiers, MouseButton, Pixels, Point, VisualTestAppContext, WindowHandle, point, px,
};
use image::RgbaImage;
use inkra_vault::mem_fs::{Gate, MemFs};
use inkra_vault::{Fs, Vault};

use crate::Quit;
use crate::bootstrap::{Launch, WINDOW_SIZE, root};
use crate::keymap::key_bindings;
use crate::shell::AppShell;

const VAULT_ROOT: &str = "/vault";

pub(crate) struct Stage {
    cx: VisualTestAppContext,
    /// Held walks and loads, released on drop, after the shot.
    gates: Vec<Gate>,
    /// `open_vault`'s, so a scene can change a file after opening.
    fs: Option<Arc<MemFs>>,
}

impl Stage {
    /// Both halves or neither: bindings without the `Quit` handler give a
    /// scene a silent no-op where the real app quits.
    pub(crate) fn new() -> Self {
        let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
        cx.update(|cx| {
            cx.bind_keys(key_bindings());
            cx.on_action(|_: &Quit, cx| cx.quit());
        });
        Self {
            cx,
            gates: Vec::new(),
            fs: None,
        }
    }

    pub(crate) fn open_nothing(&mut self) -> WindowHandle<AppShell> {
        self.open(Arc::new(MemFs::new()), Launch::Nothing)
    }

    pub(crate) fn open_note(&mut self, text: &str) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        let path = PathBuf::from(VAULT_ROOT).join("note.md");
        fs.insert_file(path.clone(), text);
        let loaded = futures::executor::block_on(fs.load(&path))
            .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
        self.open(fs, Launch::Note(path, loaded.text, loaded.stamp))
    }

    pub(crate) fn open_vault(&mut self, files: &[(&str, &str)]) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new(VAULT_ROOT)))
            .unwrap_or_else(|error| panic!("scan {VAULT_ROOT}: {error}"));
        self.fs = Some(Arc::clone(&fs));
        self.open(fs, Launch::Vault(scanned))
    }

    /// A change outside the app to a file of the vault `open_vault` opened.
    pub(crate) fn write(&mut self, path: &str, text: &str) {
        let Some(fs) = self.fs.as_ref() else {
            panic!("a scene writes only after `open_vault`");
        };
        fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), text);
        self.settle();
    }

    /// A file of the vault `open_vault` opened, deleted outside the app.
    pub(crate) fn remove(&mut self, path: &str) {
        let Some(fs) = self.fs.as_ref() else {
            panic!("a scene removes only after `open_vault`");
        };
        fs.remove(&PathBuf::from(VAULT_ROOT).join(path));
        self.settle();
    }

    /// A scanned vault whose load of `held` is parked, so the index stays building.
    pub(crate) fn open_vault_loading(
        &mut self,
        files: &[(&str, &str)],
        held: &str,
    ) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        self.gates
            .push(fs.block_load(PathBuf::from(VAULT_ROOT).join(held)));
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new(VAULT_ROOT)))
            .unwrap_or_else(|error| panic!("scan {VAULT_ROOT}: {error}"));
        self.open(fs, Launch::Vault(scanned))
    }

    /// A vault the store scans behind the first frame; the walk is held at `held`, so the shot shows the root level only.
    pub(crate) fn open_unscanned_vault(
        &mut self,
        files: &[(&str, &str)],
        held: &str,
    ) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        self.gates
            .push(fs.block_read_dir(PathBuf::from(VAULT_ROOT).join(held)));
        self.open(fs, Launch::Vault(Vault::new(Path::new(VAULT_ROOT))))
    }

    /// The one door to a window: every scene gets `WINDOW_SIZE`, never a literal of its own.
    fn open(&mut self, fs: Arc<dyn Fs>, launched: Launch) -> WindowHandle<AppShell> {
        let window = self
            .cx
            .open_offscreen_window(WINDOW_SIZE, |_, cx| root(fs, launched, cx))
            .unwrap_or_else(|error| panic!("open the off-screen window: {error}"));
        self.settle();
        window
    }

    /// Explicit, never automatic: an unfocused window is itself a state worth shooting.
    pub(crate) fn focus(&mut self, window: WindowHandle<AppShell>) {
        window
            .update(&mut self.cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx), cx);
            })
            .unwrap_or_else(|error| panic!("focus the window: {error}"));
        self.settle();
    }

    /// Clicking a note row hands focus to the editor, so a tree gesture must ask for this after.
    pub(crate) fn focus_tree(&mut self, window: WindowHandle<AppShell>) {
        window
            .update(&mut self.cx, |view, window, cx| {
                let handle = view.tree_focus_handle(cx);
                window.focus(&handle, cx);
            })
            .unwrap_or_else(|error| panic!("focus the tree: {error}"));
        self.settle();
    }

    pub(crate) fn keys(&mut self, window: WindowHandle<AppShell>, keystrokes: &str) {
        self.cx.simulate_keystrokes(window.into(), keystrokes);
        self.settle();
    }

    /// For text no key names, such as an emoji.
    pub(crate) fn type_text(&mut self, window: WindowHandle<AppShell>, text: &str) {
        self.cx.simulate_input(window.into(), text);
        self.settle();
    }

    /// The pickers debounce on gpui's clock, which only moves when asked.
    pub(crate) fn wait(&mut self, duration: Duration) {
        self.cx.advance_clock(duration);
        self.settle();
    }

    pub(crate) fn click(&mut self, window: WindowHandle<AppShell>, at: Point<Pixels>) {
        self.cx
            .simulate_click(window.into(), at, gpui::Modifiers::default());
        self.settle();
    }

    /// Presses at `from` and moves past gpui's 2 px threshold onto `to`, holding the button: the drag stays on screen.
    pub(crate) fn drag_holding(
        &mut self,
        window: WindowHandle<AppShell>,
        from: Point<Pixels>,
        to: Point<Pixels>,
    ) {
        let (window, none) = (window.into(), Modifiers::default());
        self.cx
            .simulate_mouse_down(window, from, MouseButton::Left, none);
        self.cx.simulate_mouse_move(
            window,
            from + point(px(4.), px(0.)),
            MouseButton::Left,
            none,
        );
        self.cx
            .simulate_mouse_move(window, to, MouseButton::Left, none);
        self.settle();
    }

    pub(crate) fn tab_center(
        &mut self,
        window: WindowHandle<AppShell>,
        ix: usize,
    ) -> Point<Pixels> {
        window
            .update(&mut self.cx, |shell, _, _| shell.tab_center(ix))
            .unwrap_or_else(|error| panic!("read tab {ix}: {error}"))
            .unwrap_or_else(|| panic!("tab {ix} is drawn"))
    }

    /// Pixels are device pixels: a 1000x640 window yields 2000x1280 on a 2x display.
    pub(crate) fn capture(&mut self, window: WindowHandle<AppShell>) -> RgbaImage {
        self.settle();
        self.cx
            .capture_screenshot(window.into())
            .unwrap_or_else(|error| panic!("capture the frame: {error}"))
    }

    fn settle(&mut self) {
        self.cx.run_until_parked();
    }
}

impl Drop for Stage {
    /// A walk still held owns a vault handle, which gpui reports as leaked when the app drops.
    fn drop(&mut self) {
        self.gates.clear();
        self.settle();
    }
}
