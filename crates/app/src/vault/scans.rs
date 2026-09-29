//! Runs the vault's scans (`inkra_vault::Scans`) on the background executor and wires their swaps to the store.

use std::sync::Arc;

use futures::channel::oneshot;
use gpui::{Context, Entity, Priority, prelude::*};
use inkra_vault::{
    Claims, EntryId, FileIdentity, Listed, Mtime, Pending, Scan, Scanned, Scans, Swapped, Vault,
};

use super::NoteStore;
use crate::background::Background;
use crate::timeline::{Step, Timeline, timed};

impl NoteStore {
    /// Resolves once a scan that started after this call has swapped in; dropped if its walk fails.
    pub(super) fn rescan(&mut self, cx: &mut Context<Self>) -> oneshot::Receiver<()> {
        let Some(vault) = self.vault.clone() else {
            return oneshot::channel().1;
        };
        let (wait, scan) = self.scans.request(vault.read(cx));
        self.spawn_scan(scan, cx);
        wait
    }

    /// A watch's batch: read now, or merged into what the scan after the running one reads.
    pub(super) fn rescan_changed(&mut self, batch: Pending, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let scan = self.scans.batch(batch, vault.read(cx));
        self.spawn_scan(scan, cx);
    }

    /// After every save; during a scan only recorded, for the swap to replay (open-first spec, O2).
    pub(super) fn reidentify(
        &mut self,
        id: EntryId,
        identity: FileIdentity,
        mtime: Mtime,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let scans = &mut self.scans;
        vault.update(cx, |live, cx| {
            if scans.reidentify(live, id, identity, mtime) {
                cx.notify();
            }
        });
    }

    fn spawn_scan(&self, scan: Option<Scan>, cx: &mut Context<Self>) {
        let (Some(mut scan), Some(vault)) = (scan, self.vault.clone()) else {
            return;
        };
        let fs = Arc::clone(&self.fs);
        let jobs = Background {
            executor: cx.background_executor().clone(),
            priority: Priority::Low,
        };
        // Detached: a store dropped mid-scan lets the walk finish, then its updates find nothing.
        cx.spawn(async move |this, cx| {
            if scan.is_initial() {
                let root_fs = Arc::clone(&fs);
                let listed = cx
                    .background_spawn(async move { scan.list_root(&*root_fs).await })
                    .await;
                // One turn: no re-key falls between the root pass's swap and the full pass's base.
                let Ok((applied, full)) = timed(cx, |cx| {
                    this.update(cx, |this, cx| this.swap_root(&vault, listed, cx))
                }) else {
                    return;
                };
                scan = full;
                if applied {
                    cx.update(|cx| Timeline::mark(Step::Root, cx));
                }
            }
            let scanned = cx
                .background_spawn(async move { scan.walk(&*fs, &jobs).await })
                .await;
            let _ = timed(cx, |cx| {
                this.update(cx, |this, cx| this.swap(&vault, scanned, cx))
            });
            cx.update(|cx| {
                if vault.read(cx).is_scanned() {
                    Timeline::mark(Step::Scanned, cx);
                }
            });
        })
        .detach();
    }

    /// The root pass's swap, and the full pass it continues into; whether the listing swapped in.
    fn swap_root(
        &mut self,
        vault: &Entity<Vault>,
        listed: Listed,
        cx: &mut Context<Self>,
    ) -> (bool, Scan) {
        let (swapped, full) = self.apply(
            vault,
            |scans, live, claims| scans.swap_root(live, listed, claims),
            cx,
        );
        (self.after_swap(vault, swapped, cx), full)
    }

    fn swap(&mut self, vault: &Entity<Vault>, scanned: Scanned, cx: &mut Context<Self>) {
        let (swapped, next) = self.apply(
            vault,
            |scans, live, claims| scans.swap(live, scanned, claims),
            cx,
        );
        self.after_swap(vault, swapped, cx);
        self.spawn_scan(next, cx);
    }

    /// Runs one of `Scans`' swaps on the live vault in this turn, with the open notes' claims.
    fn apply<T>(
        &mut self,
        vault: &Entity<Vault>,
        swap: impl FnOnce(&mut Scans, &mut Vault, &Claims) -> (Swapped, T),
        cx: &mut Context<Self>,
    ) -> (Swapped, T) {
        let claims = self.sessions.claims();
        let scans = &mut self.scans;
        vault.update(cx, |live, cx| {
            let (swapped, rest) = swap(scans, live, &claims);
            // Our own save's echo changes nothing: the sidebar and the index skip their O(n) pass.
            if swapped.notify {
                cx.notify();
            }
            (swapped, rest)
        })
    }

    /// Checks the open notes against the state a result replaced; whether the result swapped in.
    fn after_swap(
        &mut self,
        vault: &Entity<Vault>,
        swapped: Swapped,
        cx: &mut Context<Self>,
    ) -> bool {
        match swapped.replaced {
            Ok(previous) => {
                self.check_open_notes(vault, &previous, cx);
                // The replaced state may be the last owner of 50,000 entries: freeing them is foreground time.
                cx.background_spawn(async move { drop(previous) }).detach();
                true
            }
            Err(error) => {
                eprintln!("vault scan: {error}");
                false
            }
        }
    }
}
