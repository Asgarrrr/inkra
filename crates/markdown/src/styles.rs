use std::ops::{BitOr, BitOrAssign};

/// Semantic tags only; colour, font and size belong to `app`'s theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Styles(u16);

/// The low three bits hold a heading level, 1 to 6; zero means no heading.
const LEVEL: u16 = 0b111;

impl Styles {
    pub const NONE: Self = Self(0);
    pub const STRONG: Self = Self(1 << 3);
    pub const EMPHASIS: Self = Self(1 << 4);
    pub const STRIKETHROUGH: Self = Self(1 << 5);
    pub const CODE: Self = Self(1 << 6);
    pub const LINK: Self = Self(1 << 7);
    pub const MARKER: Self = Self(1 << 8);
    pub const QUOTE: Self = Self(1 << 9);
    pub const FRONTMATTER: Self = Self(1 << 10);

    /// # Panics
    ///
    /// When `level` is not 1 to 6: a caller bug, since pulldown-cmark has six levels.
    #[must_use]
    pub fn heading(level: u8) -> Self {
        assert!(
            (1..=6).contains(&level),
            "heading level {level} out of 1..=6"
        );
        Self(u16::from(level))
    }

    #[must_use]
    pub fn heading_level(self) -> Option<u8> {
        match self.0 & LEVEL {
            0 => None,
            level => u8::try_from(level).ok(),
        }
    }

    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Styles {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Styles {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

#[cfg(test)]
mod tests;
