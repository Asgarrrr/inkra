//! What a lexer carries from one line to the next (code highlighting spec, D2).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
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
