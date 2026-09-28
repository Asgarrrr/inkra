//! Events a `Buffer` entity emits, apart from the vault so a view hears them without importing it.

use std::sync::Arc;

use gpui::EventEmitter;
use inkra_text::{Buffer, Diff};

/// The buffer took a change from disk; each view showing it maps its offsets through the diff.
#[derive(Debug, Clone)]
pub(crate) struct Reloaded(pub(crate) Arc<Diff>);

impl EventEmitter<Reloaded> for Buffer {}
