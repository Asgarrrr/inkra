//! When a refresh starts: at once when idle, else once the scan in flight applies.

use gpui::Context;

use super::QuickOpenDelegate;
use crate::picker::Picker;

/// Search spec D7: a refresh never cancels a scan in flight; it marks it stale instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scan {
    Idle,
    /// `user` for a `Run::Search`: only then does a keystroke wait `SEARCH_DEBOUNCE` behind it.
    Running {
        user: bool,
    },
    /// A refresh arrived during the scan: one re-run follows once it applies.
    Stale {
        user: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Run {
    /// A keystroke, or the first names after the scan: the user waits on it.
    Search,
    /// After the names changed: it waits, then keeps the selected note if still listed.
    Refresh,
    /// After a notify or a deferred refresh: keeps the selection and waits nothing.
    Notify,
}

impl QuickOpenDelegate {
    /// Search spec D7: a refresh never cancels a running scan, whose query the user still waits on.
    pub(super) fn refresh_or_defer(
        picker: &mut Picker<Self>,
        run: Run,
        cx: &mut Context<Picker<Self>>,
    ) {
        match picker.delegate.scan {
            Scan::Running { user } => picker.delegate.scan = Scan::Stale { user },
            Scan::Stale { .. } => {}
            Scan::Idle => {
                picker.delegate.next_run = run;
                if run == Run::Search {
                    picker.refresh(cx);
                } else {
                    picker.refresh_in_background(cx);
                }
            }
        }
    }
}
