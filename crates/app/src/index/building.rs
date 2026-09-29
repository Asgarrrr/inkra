use std::time::Instant;

use gpui::{AppContext as _, Context};
use inkra_index::live::{Applied, Built};

use super::{IndexSync, NotesChanged};
use crate::timeline::{Step, Timeline};

impl IndexSync {
    /// Diffs the vault against the last build off the foreground; `apply_build` applies the result.
    pub(super) fn reconcile(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = &self.vault else {
            return;
        };
        // One build, after the full scan: the root pass alone would cost a build the next swap discards.
        if !vault.read(cx).is_scanned() {
            return;
        }
        let started = Instant::now();
        let Some(job) = self.live.start_build(vault.read(cx).shared_entries()) else {
            return;
        };
        #[cfg(test)]
        let hold = self.hold.clone();
        let building = cx.background_spawn(async move {
            #[cfg(test)]
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            job.run()
        });
        // Detached: it holds only a weak handle, so it ends when this entity does.
        cx.spawn(async move |this, cx| {
            let built = building.await;
            let dirty =
                Self::timed_turn(&this, cx, |this, cx| this.apply_build(built, cx)) == Some(true);
            if dirty {
                let _ = this.update(cx, Self::reconcile);
            }
        })
        .detach();
        Timeline::turn(started.elapsed(), cx);
    }

    /// Applies a build and starts the loads it queued; true when the vault changed meanwhile.
    fn apply_build(&mut self, built: Built, cx: &mut Context<Self>) -> bool {
        let Some(vault) = &self.vault else {
            return false;
        };
        let Applied {
            dirty,
            layout,
            retired,
        } = self.live.apply_build(built, vault.read(cx));
        // Freeing a 50k table takes about 9 ms, over the foreground budget.
        cx.background_spawn(async move { drop(retired) }).detach();
        if layout {
            self.revision += 1;
            Timeline::mark(Step::NamesReady, cx);
            cx.emit(NotesChanged);
        }
        self.load_more(cx);
        cx.notify();
        dirty
    }
}
