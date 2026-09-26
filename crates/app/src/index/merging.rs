use gpui::{AppContext as _, Context, Priority};
use inkra_index::{MergeWhen, MergedWords};

use super::IndexSync;

impl IndexSync {
    /// Starts a due merge, or a settle once the startup load ends; a later save awaits the policy.
    pub(super) fn merge_words(&mut self, cx: &mut Context<Self>) {
        let settle = self.settling;
        let when = if settle {
            MergeWhen::Settle
        } else {
            MergeWhen::Due
        };
        let Some(merge) = self.index.words_merge(when) else {
            // Else a merge runs, and the settle starts once it applies.
            if self.index.words_settled() {
                self.settling = false;
            }
            return;
        };
        // Low: a query's scan goes first.
        let running = cx
            .background_executor()
            .spawn_with_priority(Priority::Low, async move { merge.run() });
        // Detached, holding a weak handle: a result landing after this entity is gone is dropped.
        cx.spawn(async move |this, cx| {
            let merged = running.await;
            Self::timed_turn(&this, cx, |this, cx| this.apply_merge(merged, settle, cx));
        })
        .detach();
    }

    /// Notifies no reader, since a merge changes no answer; a settle's apply ends the settle.
    fn apply_merge(&mut self, merged: MergedWords, settle: bool, cx: &mut Context<Self>) {
        if settle {
            self.settling = false;
        }
        let retired = self.index.words_apply(merged);
        // Freeing a 50k-note segment would take milliseconds of the turn.
        cx.background_spawn(async move { drop(retired) }).detach();
        #[cfg(test)]
        {
            self.merges += 1;
        }
    }
}
