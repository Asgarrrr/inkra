//! The shell's vault-mode half: the panes that have no meaning without a vault.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{AppContext as _, Context, Entity, Subscription, Window};
use inkra_layout::ViewId;
use inkra_vault::{Departures, Vault};

use super::{AppShell, FILES};
use crate::images::Images;
use crate::index::IndexSync;
use crate::open::FileActivated;
use crate::pane::PaneEntry;
use crate::sidebar::Sidebar;
use crate::vault::NoteStore;

/// What the shell has only with a vault: built once in `AppShell::new`, never later.
pub(super) struct VaultPanes {
    pub(super) vault: Entity<Vault>,
    pub(super) links: Entity<IndexSync>,
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) images: Entity<Images>,
    /// Each registered pane: Files from the start, a feature's once it activates.
    pub(super) entries: HashMap<ViewId, PaneEntry>,
    /// The vault's `departures` at the last walk of the tabs for gone files; `None` before the first.
    pub(super) checked: Option<Departures>,
    /// The tree's `FileActivated`, the vault (tab titles, the session file and a waiting
    /// restore follow it, as the sidebar's rows do), and an image's load landing.
    _subscriptions: [Subscription; 3],
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
        let fs = notes.read(cx).fs();
        let images = cx.new(|_| Images::new(fs));
        let subscriptions = [
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
        let entries = HashMap::from([(FILES, PaneEntry::new(sidebar.clone()))]);
        Self {
            vault,
            links,
            sidebar,
            images,
            entries,
            checked: None,
            _subscriptions: subscriptions,
        }
    }
}
