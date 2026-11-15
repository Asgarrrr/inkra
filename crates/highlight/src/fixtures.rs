//! Inputs the module tests share.

use std::ops::Range;

use inkra_text::ByteOffset;

pub(crate) fn r(a: usize, b: usize) -> Range<ByteOffset> {
    ByteOffset::new(a)..ByteOffset::new(b)
}

/// Two blocks with different shapes: `let` is a keyword at 8..11, `mut` one at 23..26.
pub(crate) const TWO_BLOCKS: &str = "```rust\nlet\n```\n\n```rs\nmut x\n```\n";
