use std::fmt;

/// Offset into the buffer's UTF-8 bytes — the storage coordinate, and the one
/// text shaping indexes by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

/// Offset into the buffer's UTF-16 code units — the coordinate platform text
/// input protocols speak. Purely a property of the text, with no layout input,
/// so it lives here rather than beside the protocol code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16Offset(pub usize);

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Utf16Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_and_utf16_offsets_are_distinct_types_with_value_equality() {
        assert_eq!(ByteOffset(3), ByteOffset(3));
        assert_eq!(Utf16Offset(3).0, ByteOffset(3).0);
    }

    #[test]
    fn offsets_order_by_value() {
        assert!(ByteOffset(1) < ByteOffset(2));
        assert!(Utf16Offset(5) > Utf16Offset(4));
    }
}
