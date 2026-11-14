//! What a lexer carries from one line to the next (code highlighting spec, D2).

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) open: Open,
    pub(crate) prev: Prev,
    /// The open nestings, outermost first; every entry from `depth` on is `Nest::default()`, so
    /// equal nestings compare equal.
    pub(crate) stack: [Nest; 16],
    /// Counts past 16 too, where nothing is stored and the top is unknown.
    pub(crate) depth: u8,
    /// The depth of the open attribute's entry.
    pub(crate) attribute: Option<u8>,
}

impl LineState {
    /// Whether a push stores its entry.
    pub(crate) fn room(&self) -> bool {
        usize::from(self.depth) < self.stack.len()
    }

    pub(crate) fn push(&mut self, nest: Nest) {
        if let Some(slot) = self.stack.get_mut(usize::from(self.depth)) {
            *slot = nest;
        }
        self.depth = self.depth.saturating_add(1);
    }

    pub(crate) fn pop(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        if let Some(slot) = self.stack.get_mut(usize::from(self.depth)) {
            *slot = Nest::default();
        }
    }

    /// The innermost entry, known only at a depth of 1 to 16.
    pub(crate) fn top(&self) -> Option<Nest> {
        let at = usize::from(self.depth).checked_sub(1)?;
        self.stack.get(at).copied()
    }
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

/// What a nesting stack entry opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Nest {
    #[default]
    Paren,
    Bracket,
    Brace,
    Generic,
    Attribute,
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
    /// An identifier just classed `Type`.
    Type,
    /// The identifier right after `fn`.
    FnName,
    /// `::`.
    PathSep,
}
