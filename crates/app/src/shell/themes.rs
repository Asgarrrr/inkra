//! The vault's theme: adopted once the window can show a notice, reloaded when its files change.

use gpui::{AppContext as _, Context};
use inkra_vault::RelPath;

use super::AppShell;
use crate::theme::{
    self,
    store::{self, Resolved},
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
}
