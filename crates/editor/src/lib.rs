//! Editing semantics over [`inkra_text::Buffer`]: selection, logical movement,
//! and the commands a user gesture maps to.
//!
//! Deliberately free of gpui, the filesystem, and the clock, so it is testable
//! without a window. Anything needing font metrics — up, down, click-to-offset,
//! wrap-aware line ends — belongs in the layer above.

mod selection;
mod state;

pub use selection::{Goal, Selection};
pub use state::{EditorState, LogicalMovement};
