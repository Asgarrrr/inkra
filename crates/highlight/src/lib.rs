//! Code in a fenced block, lexed into token classes, one line at a time (code highlighting spec).

use std::ops::Range;

use inkra_text::ByteOffset;

use state::LineState;

mod rust;
mod state;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
}

impl Language {
    /// The language a fence's info word names, in any case; `None` for an unknown word.
    #[must_use]
    pub fn from_info(word: &str) -> Option<Self> {
        ["rust", "rs"]
            .iter()
            .any(|name| name.eq_ignore_ascii_case(word))
            .then_some(Self::Rust)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenClass {
    Comment,
    Keyword,
    String,
    Constant,
    Function,
    Type,
    Property,
    Attribute,
    Punctuation,
    Operator,
}

type Lexer =
    fn(LineState, &str, ByteOffset, &mut Vec<(Range<ByteOffset>, TokenClass)>) -> LineState;

/// Appends the tokens of each content line, at its start offset, to `out`; sorted and disjoint
/// when the lines are in order.
pub fn highlight<'a>(
    language: Language,
    lines: impl Iterator<Item = (ByteOffset, &'a str)>,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) {
    let lex: Lexer = match language {
        Language::Rust => rust::lex,
    };
    lines.fold(LineState::default(), |state, (start, line)| {
        lex(state, line, start, out)
    });
}

#[cfg(test)]
mod tests;
