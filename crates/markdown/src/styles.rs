use std::ops::{BitOr, BitOrAssign};

use pulldown_cmark::HeadingLevel;

/// Semantic tags only; colour, font and size belong to `app`'s theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Styles(u16);

/// The low three bits hold a heading level, 1 to 6; zero means no heading.
const LEVEL: u16 = 0b111;

/// pulldown-cmark's six levels, `H1` first.
pub const HEADING_LEVELS: [HeadingLevel; 6] = [
    HeadingLevel::H1,
    HeadingLevel::H2,
    HeadingLevel::H3,
    HeadingLevel::H4,
    HeadingLevel::H5,
    HeadingLevel::H6,
];

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
    pub const CODE_BLOCK: Self = Self(1 << 11);

    #[must_use]
    pub fn heading(level: HeadingLevel) -> Self {
        Self(level as u16)
    }

    #[must_use]
    pub fn heading_level(self) -> Option<HeadingLevel> {
        HeadingLevel::try_from(usize::from(self.0 & LEVEL)).ok()
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
