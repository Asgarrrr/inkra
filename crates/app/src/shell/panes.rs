//! The shell's vault-mode half: the panes that have no meaning without a vault.

use gpui::{AppContext as _, Context, Entity, Subscription};
use inkra_vault::Vault;

use inkra_layout::Docks;

use super::AppShell;
use crate::backlinks::BacklinksPanel;
use crate::index::IndexSync;
use crate::open::NoteActivated;
use crate::sidebar::Sidebar;
use crate::vault::NoteStore;

/// What the shell has only with a vault: built once in `AppShell::new`, never later.
pub(super) struct VaultPanes {
    pub(super) vault: Entity<Vault>,
    pub(super) links: Entity<IndexSync>,
    pub(super) sidebar: Entity<Sidebar>,
    pub(super) backlinks: Entity<BacklinksPanel>,
    /// Here, not on `AppShell`: with no vault there is no dock to show. Written only by `change_docks`.
    pub(super) docks: Docks,
    /// The two panes' `NoteActivated`, and the vault: tab titles, the session file and a
    /// waiting restore follow it, as the sidebar's rows do.
    _subscriptions: [Subscription; 3],
}

impl VaultPanes {
    pub(super) fn new(
        vault: Entity<Vault>,
        notes: &Entity<NoteStore>,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let sidebar = cx.new(|cx| Sidebar::new(notes.clone(), vault.clone(), cx));
        let links = cx.new(|cx| IndexSync::new(notes, vault.clone(), cx));
        let backlinks = cx.new(|cx| BacklinksPanel::new(links.clone(), vault.clone(), cx));
        let subscriptions = [
            // A note linking to itself lists itself.
            cx.subscribe(&backlinks, |shell, _, event: &NoteActivated, cx| {
                shell.open(event.note, None, event.open, cx);
            }),
            cx.subscribe(&sidebar, |shell, _, event: &NoteActivated, cx| {
                shell.open(event.note, None, event.open, cx);
            }),
            cx.observe(&vault, |shell, _, cx| {
                shell.save_session_later(cx);
                shell.try_restore(cx);
                cx.notify();
            }),
        ];
        Self {
            vault,
            links,
            sidebar,
            backlinks,
            docks: Docks::default(),
            _subscriptions: subscriptions,
        }
    }
}
