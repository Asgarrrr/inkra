//! The shell's vault-mode half: the panes that have no meaning without a vault.

use std::path::PathBuf;

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use inkra_vault::{Departures, Vault};

use super::AppShell;
use crate::backlinks::BacklinksPanel;
use crate::images::Images;
use crate::index::IndexSync;
use crate::open::FileActivated;
use crate::sidebar::Sidebar;
use crate::vault::NoteStore;

/// What the shell has only with a vault: built once in `AppShell::new`, never later.
pub(super) struct VaultPanes {
    pub(super) vault: Entity<Vault>,
    pub(super) links: Entity<IndexSync>,
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) backlinks: Entity<BacklinksPanel>,
    pub(super) images: Entity<Images>,
    /// The vault's `departures` at the last walk of the tabs for gone files; `None` before the first.
    pub(super) checked: Option<Departures>,
    /// The two panes' `FileActivated`, the vault (tab titles, the session file and a waiting
    /// restore follow it, as the sidebar's rows do), and an image's load landing.
    _subscriptions: [Subscription; 4],
}

impl VaultPanes {
    pub(super) fn new(
        vault: Entity<Vault>,
        notes: &Entity<NoteStore>,
        cache: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), vault.clone(), cx));
        let links = cx.new(|cx| IndexSync::new(notes, vault.clone(), cache, cx));
        let backlinks = cx.new(|cx| BacklinksPanel::new(links.clone(), vault.clone(), cx));
        let fs = notes.read(cx).fs();
        let images = cx.new(|_| Images::new(fs));
        let subscriptions = [
            // A note linking to itself lists itself.
            cx.subscribe(&backlinks, |shell, _, event: &FileActivated, cx| {
                shell.open(event.file, None, event.open, cx);
            }),
            cx.subscribe(&sidebar, |shell, _, event: &FileActivated, cx| {
                shell.open(event.file, None, event.open, cx);
            }),
            cx.observe_in(&vault, window, |shell, _, window, cx| {
                shell.close_gone_files(window, cx);
                shell.save_session_later(cx);
                shell.try_restore(cx);
                cx.notify();
            }),
            cx.observe(&images, |_, _, cx| cx.notify()),
        ];
        Self {
            vault,
            links,
            sidebar,
            backlinks,
            images,
            checked: None,
            _subscriptions: subscriptions,
        }
    }
}
