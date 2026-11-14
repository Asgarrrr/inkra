//! What a lexer carries from one line to the next (code highlighting spec, D2).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
    pub(crate) prev: Prev,
}

/// What is open at a line end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Open {
    #[default]
    Nothing,
    BlockComment {
        depth: u8,
    },
    /// `None` is an escaped string.
    Str {
        raw_hashes: Option<u8>,
    },
}

/// The previous significant token, D4's one-token look-behind; whitespace and comments leave it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Prev {
    #[default]
    Other,
    /// A lone `.`, not `..` or `...`.
    Dot,
    Fn,
    /// `struct`, `enum`, `trait`, `type` or `impl`.
    TypeKeyword,
}
