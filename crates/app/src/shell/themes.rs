//! The vault's theme: adopted once the window can show a notice, reloaded when its files change.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, AppContext as _, Context, SharedString, Window};
use inkra_vault::{Fs, RelPath};

use super::AppShell;
use crate::theme::{
    self, Appearance, Theme, builtin,
    picker::{SelectTheme, ThemeChosen, ThemeDelegate, ThemeRow},
    store::{self, Choice, Resolved},
};

impl AppShell {
    pub(crate) fn adopt_theme(&mut self, resolved: Resolved, cx: &mut Context<Self>) {
        self.themes.choice = resolved.choice;
        // An open picker restores `before` on dismiss, so a reload must update it, not the preview.
        if let Some(picker) = self.modal_picker::<ThemeDelegate>() {
            picker.update(cx, |picker, _| picker.delegate.set_before(resolved.theme));
        } else {
            theme::install(resolved.theme, cx);
        }
        if !resolved.notices.is_empty() {
            self.show_notice(resolved.notices.join("; "), None, cx);
        }
    }

    /// Our own write's echo reloads equal colours, which `install` ignores.
    pub(super) fn config_changed(&mut self, paths: &[RelPath], cx: &mut Context<Self>) {
        if !paths.iter().any(|path| self.themes.concerns(path)) {
            return;
        }
        let Some((fs, root)) = self.vault_root(cx) else {
            return;
        };
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
        let Some((fs, root)) = self.vault_root(cx) else {
            self.open_theme_picker(Vec::new(), window, cx);
            return;
        };
        self.themes.listing = Some(cx.spawn_in(window, async move |shell, cx| {
            let listed = cx
                .background_spawn(async move { store::list(&*fs, &root).await })
                .await;
            shell
                .update_in(cx, |shell, window, cx| {
                    shell.open_theme_picker(listed, window, cx);
                })
                .ok();
        }));
    }

    fn open_theme_picker(
        &mut self,
        listed: Vec<(SharedString, Arc<Theme>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let named = |name: &'static str, appearance| ThemeRow {
            label: name.into(),
            choice: Choice::Named(name.into()),
            theme: Some(builtin::of(appearance).clone()),
        };
        let mut rows = vec![
            ThemeRow {
                label: "System".into(),
                choice: Choice::System,
                theme: None,
            },
            named("Flexoki Dark", Appearance::Dark),
            named("Flexoki Light", Appearance::Light),
        ];
        rows.extend(listed.into_iter().map(|(name, theme)| ThemeRow {
            label: name.clone(),
            choice: Choice::Named(name),
            theme: Some(theme),
        }));
        let current = self.themes.choice.clone();
        let before = (!theme::follows_system(cx)).then(|| theme::current(cx).clone());
        self.toggle_picker_with(
            window,
            cx,
            |cx| ThemeDelegate::new(rows, current, before, cx),
            |shell, event: &ThemeChosen, cx| shell.choose_theme(&event.0, cx),
        );
    }

    /// A stale reload must not override the pick.
    fn choose_theme(&mut self, choice: &Choice, cx: &mut Context<Self>) {
        self.themes.choice = choice.clone();
        self.themes.reload = None;
        let Some((fs, root)) = self.vault_root(cx) else {
            return;
        };
        let (previous, choice) = (self.themes.write.take(), choice.clone());
        self.themes.write = Some(cx.spawn(async move |shell, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let written = cx
                .background_spawn(async move { store::write_choice(&*fs, &root, &choice).await })
                .await;
            if let Err(text) = written {
                shell
                    .update(cx, |shell, cx| shell.show_notice(text, None, cx))
                    .ok();
            }
        }));
    }

    fn vault_root(&self, cx: &App) -> Option<(Arc<dyn Fs>, PathBuf)> {
        let notes = self.notes.read(cx);
        let root = notes.vault()?.read(cx).root().to_path_buf();
        Some((notes.fs(), root))
    }
}
