//! `⌘O` opens a chosen folder in place of the shell, once the shell left is settled (open-a-vault spec, D3).

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    AsyncWindowContext, Context, Focusable, PathPromptOptions, WeakEntity, Window, actions,
};
use inkra_vault::{Fs, VaultError};

use super::AppShell;
use super::leave::Leave;
use super::notice::couldnt_open;
use crate::bootstrap::{build_shell, vault_launch, vault_root};

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
        if !self.start_leaving(Leave::Open) {
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
        cx.spawn_in(window, async move |this, cx| {
            let result = match chosen.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => Self::open_path(&this, path, fs, open, cx).await,
                    None => Ok(()),
                },
                Ok(Ok(None)) | Err(_) => Ok(()),
                Ok(Err(error)) => Err(format!("Couldn't show the folder picker: {error}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if shell.stop_leaving(Leave::Open)
                    && let Err(text) = result
                {
                    shell.show_notice(text, None, cx);
                }
            });
        })
        .detach();
    }

    /// Settles this shell, then replaces it with one on `path`; `Ok` when nothing needs telling.
    async fn open_path(
        this: &WeakEntity<Self>,
        path: PathBuf,
        fs: Arc<dyn Fs>,
        open: Option<PathBuf>,
        cx: &mut AsyncWindowContext,
    ) -> Result<(), String> {
        let root = vault_root(&path).map_err(|source| {
            couldnt_open(
                &path,
                &VaultError::Io {
                    path: path.clone(),
                    source,
                },
            )
        })?;
        if open.as_ref() == Some(&root) {
            return Ok(());
        }
        let checked = {
            let (fs, root) = (Arc::clone(&fs), root.clone());
            cx.background_executor()
                .spawn(async move { fs.check_dir(&root).await })
                .await
        };
        if let Err(error) = checked {
            return Err(couldnt_open(&root, &error));
        }
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
            let still = this
                .read_with(cx, |shell, _| shell.leaving == Some(Leave::Open))
                .unwrap_or(false);
            if !still {
                return;
            }
            let shell = window.replace_root(cx, |window, cx| {
                build_shell(fs, vault_launch(&root, None), None, window, cx)
            });
            window.focus(&shell.focus_handle(cx), cx);
        });
        Ok(())
    }
}
