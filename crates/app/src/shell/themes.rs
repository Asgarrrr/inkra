//! The vault's theme, adopted once the window can show why it fell back.

use gpui::Context;

use super::AppShell;
use crate::theme::{self, store::Resolved};

impl AppShell {
    pub(crate) fn adopt_theme(&mut self, resolved: Resolved, cx: &mut Context<Self>) {
        theme::install(resolved.theme, cx);
        if !resolved.notices.is_empty() {
            self.show_notice(resolved.notices.join("; "), None, cx);
        }
    }
}
