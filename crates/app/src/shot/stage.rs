use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    Focusable, Modifiers, MouseButton, Pixels, Point, VisualTestAppContext, WindowHandle, point, px,
};
use image::RgbaImage;
use inkra_layout::Side;
use inkra_vault::mem_fs::{Gate, MemFs};
use inkra_vault::{Fs, Vault};

use crate::Quit;
use crate::bootstrap::{Launch, WINDOW_SIZE, root};
use crate::keymap::key_bindings;
use crate::restore::{SavedTab, Session};
use crate::shell::AppShell;
use crate::theme::{self, Appearance};
use crate::ui::Assets;

const VAULT_ROOT: &str = "/vault";

pub(crate) struct Stage {
    cx: VisualTestAppContext,
    appearance: Appearance,
    /// Held walks and loads, released on drop, after the shot.
    gates: Vec<Gate>,
    /// `open_vault`'s, so a scene can change a file after opening.
    fs: Option<Arc<MemFs>>,
}

impl Stage {
    /// Both halves or neither: bindings without the `Quit` handler give a
    /// scene a silent no-op where the real app quits.
    pub(crate) fn new(appearance: Appearance) -> Self {
        let mut cx = VisualTestAppContext::with_asset_source(
            gpui_platform::current_platform(false),
            Arc::new(Assets),
        );
        cx.update(|cx| {
            cx.bind_keys(key_bindings());
            cx.on_action(|_: &Quit, cx| cx.quit());
        });
        Self {
            cx,
            appearance,
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
        self.open_scanned(fs)
    }

    /// A vault whose platform refuses the watch, so the shell opens with its notice.
    pub(crate) fn open_unwatched_vault(
        &mut self,
        files: &[(&str, &str)],
    ) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        fs.make_unwatchable(VAULT_ROOT);
        self.open_scanned(fs)
    }

    fn open_scanned(&mut self, fs: Arc<MemFs>) -> WindowHandle<AppShell> {
        let scanned = futures::executor::block_on(Vault::scan(&*fs, Path::new(VAULT_ROOT)))
            .unwrap_or_else(|error| panic!("scan {VAULT_ROOT}: {error}"));
        self.fs = Some(Arc::clone(&fs));
        self.open(fs, Launch::Vault(scanned, None))
    }

    /// The vault `open_vault` opened, so a scene can inject a fault.
    pub(crate) fn fs(&self) -> &MemFs {
        let Some(fs) = self.fs.as_ref() else {
            panic!("a scene reaches the disk only after `open_vault`");
        };
        fs
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
        self.open(fs, Launch::Vault(scanned, None))
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
        self.open(fs, Launch::Vault(Vault::new(Path::new(VAULT_ROOT)), None))
    }

    /// A vault launch whose session file holds `tabs`, `active` among them; the restore lands before the shot.
    pub(crate) fn open_restored(
        &mut self,
        files: &[(&str, &str)],
        tabs: Vec<SavedTab>,
        active: usize,
    ) -> WindowHandle<AppShell> {
        let fs = Arc::new(MemFs::new());
        for (path, text) in files {
            fs.insert_file(PathBuf::from(VAULT_ROOT).join(path), *text);
        }
        let session = PathBuf::from("/support/session.json");
        let text = Session::new(VAULT_ROOT.into(), active, tabs, None)
            .encode()
            .unwrap_or_else(|error| panic!("encode the session: {error}"));
        fs.insert_file(session.clone(), text);
        let vault = Vault::new(Path::new(VAULT_ROOT));
        self.open(fs, Launch::Vault(vault, Some(session)))
    }

    /// The one door to a window: every scene gets `WINDOW_SIZE`, never a literal of its own.
    fn open(&mut self, fs: Arc<dyn Fs>, launched: Launch) -> WindowHandle<AppShell> {
        let window = self
            .cx
            .open_offscreen_window(WINDOW_SIZE, |window, cx| root(fs, launched, window, cx))
            .unwrap_or_else(|error| panic!("open the off-screen window: {error}"));
        // A new window reports the platform's Dark, so the shell's observer just overrode the theme.
        self.cx
            .update(|cx| theme::follow_system(self.appearance.into(), cx));
        window
            .update(&mut self.cx, |view, _, cx| {
                view.set_backdrop(theme::shot_backdrop(theme::current(cx)));
            })
            .unwrap_or_else(|error| panic!("set the backdrop: {error}"));
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

    /// Moves the pointer with no button held, so the row under it shows its hover style.
    pub(crate) fn hover(&mut self, window: WindowHandle<AppShell>, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_move(window.into(), at, None, Modifiers::default());
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

    pub(crate) fn right_click(&mut self, window: WindowHandle<AppShell>, at: Point<Pixels>) {
        let (window, none) = (window.into(), Modifiers::default());
        self.cx
            .simulate_mouse_down(window, at, MouseButton::Right, none);
        self.cx
            .simulate_mouse_up(window, at, MouseButton::Right, none);
        self.settle();
    }

    pub(crate) fn tab_center(
        &mut self,
        window: WindowHandle<AppShell>,
        ix: usize,
    ) -> Point<Pixels> {
        let Some(center) = window
            .update(&mut self.cx, |shell, _, _| shell.tab_center(ix))
            .unwrap_or_else(|error| panic!("read tab {ix}: {error}"))
        else {
            panic!("tab {ix} is drawn");
        };
        center
    }

    pub(crate) fn dock_icon_point(
        &mut self,
        window: WindowHandle<AppShell>,
        side: Side,
        index: usize,
    ) -> Point<Pixels> {
        window
            .update(&mut self.cx, |_, window, _| {
                AppShell::dock_icon_point(side, index, window)
            })
            .unwrap_or_else(|error| panic!("read dock icon {index}: {error}"))
    }

    pub(crate) fn click_tree_row(&mut self, window: WindowHandle<AppShell>, ix: f32) {
        let at = self.tree_row_center(window, ix);
        self.click(window, at);
    }

    pub(crate) fn hover_tree_row(&mut self, window: WindowHandle<AppShell>, ix: f32) {
        let at = self.tree_row_center(window, ix);
        self.hover(window, at);
    }

    fn tree_row_center(&mut self, window: WindowHandle<AppShell>, ix: f32) -> Point<Pixels> {
        let Some(center) = window
            .update(&mut self.cx, |shell, _, cx| shell.tree_row_center(ix, cx))
            .unwrap_or_else(|error| panic!("read the tree's row {ix}: {error}"))
        else {
            panic!("the tree's row {ix} is drawn");
        };
        center
    }

    pub(crate) fn quick_open_row_center(
        &mut self,
        window: WindowHandle<AppShell>,
        ix: f32,
    ) -> Point<Pixels> {
        let Some(center) = window
            .update(&mut self.cx, |shell, _, cx| {
                shell.quick_open_row_center(ix, cx)
            })
            .unwrap_or_else(|error| panic!("read quick open's row {ix}: {error}"))
        else {
            panic!("quick open's row {ix} is drawn");
        };
        center
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
    /// A window's views release only when it closes: dropped with the app instead, a pinned editor asserts.
    fn drop(&mut self) {
        self.gates.clear();
        for window in self.cx.update(|cx| cx.windows()) {
            let _ = self
                .cx
                .update_window(window, |_, window, _| window.remove_window());
        }
        self.settle();
    }
}
