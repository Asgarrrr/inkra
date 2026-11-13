//! `⌘O` opens a chosen folder in place of the shell, once the shell left is settled (open-a-vault spec, D3).

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use futures::FutureExt as _;
use gpui::{
    App, AppContext as _, AsyncWindowContext, Context, Focusable, PathPromptOptions, WeakEntity,
    Window, actions,
};
use inkra_vault::{Fs, VaultError};

use super::AppShell;
use super::leave::{Leave, QUIT_WAIT};
use super::notice::couldnt_open;
use crate::bootstrap::{self, Launch, build_shell, vault_launch, vault_root};
use crate::feature::Registry;
use crate::last_vault;
use crate::restore::Opened;
use crate::settings::Switches;
use crate::theme::store::{self, Resolved};
use crate::timeline::Timeline;
use crate::vault::NoteStore;

actions!(
    vault,
    [
        /// Asks for a folder and opens it as the vault, in place of this shell.
        OpenVault
    ]
);

impl AppShell {
    pub(super) fn open_vault(
        &mut self,
        _: &OpenVault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.start_open() {
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Vault".into()),
        });
        let fs = self.notes.read(cx).fs();
        let open = self.vault_root(cx).map(|(_, root)| root);
        let home = bootstrap::home(cx);
        cx.spawn_in(window, async move |this, cx| {
            let chosen = chosen.await;
            let result = match chosen {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => Self::open_path(&this, path, fs, open, home, cx).await,
                    None => Ok(()),
                },
                Ok(Ok(None)) | Err(_) => Ok(()),
                Ok(Err(error)) => Err(format!("Couldn't show the folder picker: {error}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if shell.stop_open()
                    && let Err(text) = result
                {
                    shell.show_notice(text, None, cx);
                }
            });
        })
        .detach();
    }

    /// Records this shell's vault for the next launch; called only once the shell is in place (D4).
    pub(crate) fn remember_vault(&self, cx: &App) {
        let (Some(home), Some((fs, root))) = (bootstrap::home(cx), self.vault_root(cx)) else {
            return;
        };
        cx.background_spawn(async move { last_vault::write(&*fs, &home, &root).await })
            .detach();
    }

    /// Settles this shell, then replaces it with one on `path`; `Ok` when nothing needs telling.
    async fn open_path(
        this: &WeakEntity<Self>,
        path: PathBuf,
        fs: Arc<dyn Fs>,
        open: Option<PathBuf>,
        home: Option<PathBuf>,
        cx: &mut AsyncWindowContext,
    ) -> Result<(), String> {
        let root = vault_root(&path).map_err(|error| couldnt_open(&path, &error))?;
        if open.as_ref() == Some(&root) {
            return Ok(());
        }
        let launched = vault_launch(&root, home.as_deref())?;
        let read = cx
            .background_executor()
            .spawn(read(Arc::clone(&fs), root.clone(), launched));
        let ready = futures::select_biased! {
            ready = read.fuse() => ready,
            () = cx.background_executor().timer(QUIT_WAIT).fuse() => Err(VaultError::Io {
                path: root.clone(),
                source: io::ErrorKind::TimedOut.into(),
            }),
        }
        .map_err(|error| couldnt_open(&root, &error))?;
        let Ok(settled) = this.update_in(cx, |shell, window, cx| {
            shell.settle(Leave::Open, window, cx)
        }) else {
            return Ok(());
        };
        if !settled.await {
            return Ok(());
        }
        // One update with no await: a quit pressed after this check cannot interleave with the swap.
        let _ = cx.update(|window, cx| {
            let Ok(Some((notes, left, catalog))) = this.read_with(cx, |shell, _| {
                (shell.leaving == Some(Leave::Open)).then(|| {
                    let catalog = shell.features.catalog();
                    (shell.notes.clone(), shell.left.clone(), catalog)
                })
            }) else {
                return;
            };
            Timeline::end(cx);
            // Before the replace, while the old store lives: it copies out text typed during the leave.
            let left = left.then(notes.update(cx, NoteStore::flush_all), window, cx);
            // The new vault's switches, not the old shell's: a shell never builds a feature its vault disables.
            let features = Registry::new(
                catalog,
                ready
                    .resolved
                    .features
                    .as_ref()
                    .unwrap_or(&Switches::default()),
            );
            let shell = window.replace_root(cx, |window, cx| {
                build_shell(fs, ready.launched, ready.opened, left, features, window, cx)
            });
            window.focus(&shell.focus_handle(cx), cx);
            shell.update(cx, |shell, cx| shell.adopt_theme(ready.resolved, cx));
            shell.read(cx).remember_vault(cx);
        });
        Ok(())
    }
}

/// What the new shell is built from, read before the shell left is touched.
struct Ready {
    launched: Launch,
    opened: Option<Opened>,
    resolved: Resolved,
}

async fn read(fs: Arc<dyn Fs>, root: PathBuf, launched: Launch) -> Result<Ready, VaultError> {
    fs.check_dir(&root).await?;
    let opened = bootstrap::opened(Arc::clone(&fs), &launched).await;
    let resolved = store::load(&*fs, &root).await;
    Ok(Ready {
        launched,
        opened,
        resolved,
    })
}
