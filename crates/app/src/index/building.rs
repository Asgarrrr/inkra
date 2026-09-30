use gpui::Context;
use inkra_index::live::Turn;

use super::IndexSync;

impl IndexSync {
    /// The vault changed: a turn starts the build once the vault is scanned.
    pub(super) fn reconcile(&mut self, cx: &mut Context<Self>) {
        // Typed, or clippy asks for the path `Turn::vault_changed`, which binds one lifetime and fails the bound.
        self.turn(cx, |turn: &mut Turn<'_>| turn.vault_changed());
    }
}
