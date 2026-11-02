use std::fmt;
use std::ops::{Add, Sub};

/// A position in a text, as a UTF-8 byte index. A length stays `usize`: an offset plus a
/// length is an offset, and two offsets differ by a length.
///
/// The index is private, so only `new` builds an offset:
///
/// ```compile_fail
/// let _ = inkra_text::ByteOffset(3);
/// ```
///
/// ```
/// let _ = inkra_text::ByteOffset::new(3);
/// ```
///
/// Only `get` reads it:
///
/// ```compile_fail
/// let _: usize = inkra_text::ByteOffset::new(3).0;
/// ```
///
/// ```
/// let _: usize = inkra_text::ByteOffset::new(3).get();
/// ```
///
/// Two positions do not add:
///
/// ```compile_fail
/// use inkra_text::ByteOffset;
/// let _ = ByteOffset::new(1) + ByteOffset::new(2);
/// ```
///
/// ```
/// use inkra_text::ByteOffset;
/// let _ = ByteOffset::new(1) + 2;
/// ```
///
/// A byte and a UTF-16 position do not mix:
///
/// ```compile_fail
/// use inkra_text::{ByteOffset, Utf16Offset};
/// let _ = ByteOffset::new(3) - Utf16Offset::new(1);
/// ```
///
/// ```
/// use inkra_text::{ByteOffset, Utf16Offset};
/// let _ = ByteOffset::new(3) - ByteOffset::new(1);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(usize);

/// A position in a text, in UTF-16 code units; the same rules as [`ByteOffset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16Offset(usize);

macro_rules! offset_ops {
    ($offset:ident) => {
        impl $offset {
            #[must_use]
            pub const fn new(offset: usize) -> Self {
                Self(offset)
            }

            #[must_use]
            pub const fn get(self) -> usize {
                self.0
            }
        }

        impl Add<usize> for $offset {
            type Output = Self;

            fn add(self, len: usize) -> Self {
                Self(self.0 + len)
            }
        }

        impl Sub<usize> for $offset {
            type Output = Self;

            fn sub(self, len: usize) -> Self {
                Self(self.0 - len)
            }
        }

        impl Sub for $offset {
            type Output = usize;

            fn sub(self, start: Self) -> usize {
                self.0 - start.0
            }
        }

        impl fmt::Display for $offset {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

offset_ops!(ByteOffset);
offset_ops!(Utf16Offset);

#[cfg(test)]
mod tests;
