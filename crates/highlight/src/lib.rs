//! Code in a fenced block, lexed into token classes, one line at a time (code highlighting spec).

use std::ops::Range;

use inkra_text::ByteOffset;

pub use backlog::{Backlog, TASK_BYTES, backlog, lex_backlog, lex_pending};
pub use batch::{BLOCK_LIMIT, Batch, BlockChunk, batch, block_marks};
pub use open::{OPEN_PARSE_LIMIT, open_chunks};
pub use plan::MAIN_THREAD_BUDGET;
use state::LineState;

mod backlog;
mod batch;
#[cfg(test)]
mod fixtures;
mod open;
mod plan;
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

/// A code block's tokens over its content `lines`, in the language of its info word at `info` in
/// `text`; none without a word or for an unknown one.
#[must_use]
pub(crate) fn block_tokens<'a>(
    text: &str,
    info: Option<Range<ByteOffset>>,
    lines: impl Iterator<Item = (ByteOffset, &'a str)>,
) -> Vec<(Range<ByteOffset>, TokenClass)> {
    let mut tokens = Vec::new();
    if let Some(language) = info_language(text, info) {
        highlight(language, lines, &mut tokens);
    }
    tokens
}

/// The language of the info word at `info` in `text`; `None` without a word or for an unknown one.
pub(crate) fn info_language(text: &str, info: Option<Range<ByteOffset>>) -> Option<Language> {
    info.and_then(|word| Language::from_info(&text[word.start.get()..word.end.get()]))
}

#[cfg(test)]
mod tests;
