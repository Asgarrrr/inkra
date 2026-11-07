//! The token a holder of ids compares to learn whether any left the vault.

use std::sync::Arc;

/// Replaced whenever a live id leaves; a token, not a count, since a scan applies onto an older clone.
#[derive(Debug, Clone, Default)]
pub struct Departures(Arc<()>);

impl Departures {
    pub(super) fn renew(&mut self) {
        *self = Self::default();
    }
}

impl PartialEq for Departures {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Departures {}
