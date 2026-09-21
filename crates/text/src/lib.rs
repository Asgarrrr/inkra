//! The text model: a document, the offsets into it, and the transaction and
//! undo machinery every edit goes through.
//!
//! The bottom of the stack. No gpui, no filesystem, no clock — an edit takes the
//! `Instant` from its caller — so every property here is testable without a
//! window. See `docs/buffer-model.md` for the offset rules and what was
//! rejected.

mod anchor;
mod buffer;
mod offset;

pub use anchor::{Anchor, Bias};
pub use buffer::{Buffer, GROUP_INTERVAL};
pub use offset::{ByteOffset, Utf16Offset};
