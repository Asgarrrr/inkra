//! The vault's theme: adopted once the window can show a notice, reloaded when its files change.

use gpui::{AppContext as _, Context, Window};
use inkra_vault::RelPath;

use super::AppShell;
use crate::theme::{
    self, Appearance, builtin,
    picker::{SelectTheme, ThemeChosen, ThemeDelegate, ThemeRow},
    store::{self, Choice, Resolved},
};

impl AppShell {
    pub(crate) fn adopt_theme(&mut self, resolved: Resolved, cx: &mut Context<Self>) {
        self.themes.choice = resolved.choice;
        theme::install(resolved.theme, cx);
        if !resolved.notices.is_empty() {
            self.show_notice(resolved.notices.join("; "), None, cx);
        }
    }

    /// Our own write's echo reloads equal colours, which `install` ignores.
    pub(super) fn config_changed(&mut self, paths: &[RelPath], cx: &mut Context<Self>) {
        if !paths.iter().any(|path| self.themes.concerns(path)) {
            return;
        }
        let notes = self.notes.read(cx);
        let Some(vault) = notes.vault() else {
            return;
        };
        let fs = notes.fs();
        let root = vault.read(cx).root().to_path_buf();
        self.themes.reload = Some(cx.spawn(async move |shell, cx| {
            let resolved = cx
                .background_spawn(async move { store::load(&*fs, &root).await })
                .await;
            shell
                .update(cx, |shell, cx| shell.adopt_theme(resolved, cx))
                .ok();
        }));
    }

    pub(super) fn select_theme(
        &mut self,
        _: &SelectTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_theme_picker(window, cx);
    }

    fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let named = |name: &'static str, appearance| ThemeRow {
            label: name.into(),
            choice: Choice::Named(name.into()),
            theme: Some(builtin::of(appearance).clone()),
        };
        let rows = vec![
            ThemeRow {
                label: "System".into(),
                choice: Choice::System,
                theme: None,
            },
            named("Flexoki Dark", Appearance::Dark),
            named("Flexoki Light", Appearance::Light),
        ];
        let current = self.themes.choice.clone();
        let before = (!theme::follows_system(cx)).then(|| theme::current(cx).clone());
        self.toggle_picker_with(
            window,
            cx,
            |cx| ThemeDelegate::new(rows, current, before, cx),
            |shell, event: &ThemeChosen, _| shell.choose_theme(&event.0),
        );
    }

    /// A stale reload must not override the pick.
    fn choose_theme(&mut self, choice: &Choice) {
        self.themes.choice = choice.clone();
        self.themes.reload = None;
    }
}
