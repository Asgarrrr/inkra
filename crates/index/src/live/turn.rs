//! One foreground turn: its inputs, then its end, which returns the work the turn made due.

use inkra_vault::{EntryId, Stamp, Vault};

use super::{Applied, BuildJob, Built, LiveIndex, Load, Merge, Merged, Retired, Ticket, TurnEnd};
use crate::content::NoteContent;
use crate::persist::Outcome;
use crate::words::RetiredWords;

/// The inputs of one foreground turn; only `LiveIndex::turn` makes one.
pub struct Turn<'a> {
    live: &'a mut LiveIndex,
    vault: &'a Vault,
    applied: Option<Applied>,
    words: Option<RetiredWords>,
    /// The vault changed in this turn, or while the build it applied ran: the end starts one.
    changed: bool,
}

/// Everything the turn made due. Run each job off the foreground and hand its result
/// back in a later turn; free `retired` off the foreground.
#[must_use = "run its jobs; free `retired` off the foreground"]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent one-turn flags, any of them true together"
)]
pub struct Effects {
    /// Read, parse, hand to `Turn::landed`. At most the free slots, in queue order.
    pub loads: Vec<Load>,
    /// Diff off the foreground, hand to `Turn::built`. At most one runs (L6).
    pub build: Option<BuildJob>,
    /// Run off the foreground, hand to `Turn::merged`.
    pub merge: Option<Merge>,
    /// `None` when the turn retired nothing, so an idle turn spawns no drop.
    pub retired: Option<Retired>,
    /// The note set changed: names and link resolution may differ.
    pub layout: bool,
    /// `revision` or `progress` moved: a reader's answer may differ.
    pub notify: bool,
    /// True once: the startup load ended and its settle applied.
    pub indexed: bool,
    /// True once: this turn applied the build carrying the cache's seed.
    pub seeded: bool,
}

impl LiveIndex {
    /// Runs `inputs` against the live `vault`, then ends the turn: the work it made due.
    pub fn turn<R>(
        &mut self,
        vault: &Vault,
        inputs: impl FnOnce(&mut Turn<'_>) -> R,
    ) -> (R, Effects) {
        let start = (self.revision, self.progress());
        let mut turn = Turn {
            live: self,
            vault,
            applied: None,
            words: None,
            changed: false,
        };
        let result = inputs(&mut turn);
        let Turn {
            applied,
            words,
            changed,
            ..
        } = turn;
        let (layout, seeded, retired) = applied
            .map_or((false, false, Retired::default()), |applied| {
                (applied.layout, applied.seeded, applied.retired)
            });
        let slots = self.slots;
        let loads = std::iter::from_fn(|| self.next_load(vault, slots)).collect();
        let TurnEnd { merge, indexed } = self.end_turn();
        // One build, after the full scan: the root pass alone would cost a build the next swap discards.
        let build = if changed && vault.is_scanned() {
            self.start_build(vault.shared_entries())
        } else {
            None
        };
        let notify = (self.revision, self.progress()) != start;
        let effects = Effects {
            loads,
            build,
            merge,
            retired: Some(retired.with_words(words)).filter(|retired| !retired.is_empty()),
            layout,
            notify,
            indexed,
            seeded,
        };
        (result, effects)
    }
}

impl Turn<'_> {
    /// The vault's entries changed. A build starts once the vault is scanned; one running turns dirty.
    pub fn vault_changed(&mut self) {
        self.changed = true;
    }

    /// The cache outcome `LiveIndex::await_cache` waits for: the next build carries it.
    ///
    /// # Panics
    /// If no outcome is awaited.
    pub fn cached(&mut self, outcome: Outcome) {
        self.live.cached(outcome);
        self.changed = true;
    }

    /// A build's result, from `BuildJob::run`.
    ///
    /// # Panics
    /// If a build applied in this turn already (L6).
    pub fn built(&mut self, built: Built) {
        let applied = self.live.apply_build(built, self.vault);
        self.changed |= applied.dirty;
        assert!(
            self.applied.replace(applied).is_none(),
            "one build applies per turn (L6)"
        );
    }

    /// A note saved: `stamp` is the one indexed, and the parse of its text comes back through `landed`.
    #[must_use]
    pub fn saved(&mut self, id: EntryId, stamp: Stamp) -> Ticket {
        self.live.saved(id, stamp)
    }

    /// A load or a save's parse landed.
    pub fn landed(&mut self, ticket: Ticket, loaded: inkra_vault::Result<NoteContent>) {
        self.live.landed(ticket, loaded, self.vault);
    }

    /// Rebuild index: every note loads again; its seeded content stays until each reload lands.
    pub fn rebuild(&mut self) {
        self.live.rebuild();
    }

    /// Notes a search read changed or gone since their load: each indexed one loads again.
    pub fn stale(&mut self, ids: &[EntryId]) {
        self.live.stale(ids);
    }

    /// A word merge's result, from `Merge::run`.
    ///
    /// # Panics
    /// If a merge applied in this turn already.
    pub fn merged(&mut self, merged: Merged) {
        let words = self.live.merged(merged);
        assert!(
            self.words.replace(words).is_none(),
            "one merge applies per turn"
        );
    }
}
