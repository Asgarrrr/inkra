use std::iter;

use unicode_normalization::UnicodeNormalization;

/// A path char as the name matcher compares it (spec D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PathChar {
    pub(super) exact: char,
    /// The first char of the NFD decomposition, lowercased: `É` → `e`.
    pub(super) base: char,
}

/// A query char, lowercased; `plain` when it has no diacritic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct QueryChar {
    pub(super) c: char,
    pub(super) plain: bool,
}

/// A char whose lowercase is several chars compares by the first.
fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl PathChar {
    pub(super) fn new(c: char) -> Self {
        if c.is_ascii() {
            let lower = c.to_ascii_lowercase();
            return Self {
                exact: lower,
                base: lower,
            };
        }
        let first = iter::once(c).nfd().next().unwrap_or(c);
        Self {
            exact: lower(c),
            base: lower(first),
        }
    }
}

impl QueryChar {
    pub(super) fn new(c: char) -> Self {
        let c = lower(c);
        Self {
            c,
            plain: iter::once(c).nfd().nth(1).is_none(),
        }
    }

    /// Case always folds; accents fold only from a plain query char, as nucleo's `Smart`.
    pub(super) fn matches(self, path: PathChar) -> bool {
        self.c == path.exact || (self.plain && self.c == path.base)
    }
}

#[cfg(test)]
mod tests;
