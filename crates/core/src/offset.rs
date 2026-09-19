use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CharOffset(pub usize);

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for CharOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_and_char_offsets_are_distinct_types_with_value_equality() {
        let a = ByteOffset(3);
        let b = ByteOffset(3);
        assert_eq!(a, b);

        let c = CharOffset(3);
        assert_eq!(c.0, a.0);
    }

    #[test]
    fn offsets_order_by_value() {
        assert!(ByteOffset(1) < ByteOffset(2));
        assert!(CharOffset(5) > CharOffset(4));
    }
}
