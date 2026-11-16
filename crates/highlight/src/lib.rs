//! Code in a fenced block, lexed into token classes, one line at a time (code highlighting spec).

use std::ops::Range;

use inkra_text::ByteOffset;

pub use backlog::{Backlog, TASK_BYTES, backlog, lex_backlog, lex_pending};
pub use batch::{BLOCK_LIMIT, Batch, BlockChunk, batch, block_marks};
pub use open::{OPEN_PARSE_LIMIT, open_chunks};
pub use plan::MAIN_THREAD_BUDGET;
use state::LineState;

mod backlog;
mod bash;
mod batch;
mod css;
#[cfg(test)]
mod fixtures;
mod javascript;
mod json;
mod open;
mod plan;
mod python;
mod rust;
mod scan;
mod state;
mod typescript;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    Json,
    Bash,
    JavaScript,
    TypeScript,
    Tsx,
    Css,
}

/// Each language's info words and lexer (Languages, D13).
const LANGUAGES: [(Language, &[&str], Lexer); 8] = [
    (Language::Rust, &["rust", "rs"], rust::lex),
    (Language::Python, &["python", "py"], python::lex),
    (Language::Json, &["json", "jsonc"], json::lex),
    (Language::Bash, &["bash", "sh", "shell", "zsh"], bash::lex),
    (
        Language::JavaScript,
        &["javascript", "js", "mjs", "cjs", "jsx"],
        javascript::lex,
    ),
    (Language::TypeScript, &["typescript", "ts"], typescript::lex),
    (Language::Tsx, &["tsx"], typescript::lex_tsx),
    (Language::Css, &["css"], css::lex),
];

impl Language {
    /// The language a fence's info word names, in any case; `None` for an unknown word.
    #[must_use]
    pub fn from_info(word: &str) -> Option<Self> {
        LANGUAGES
            .iter()
            .find(|(_, names, _)| names.iter().any(|name| name.eq_ignore_ascii_case(word)))
            .map(|&(language, _, _)| language)
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
    Tag,
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
    let Some(&(_, _, lex)) = LANGUAGES.iter().find(|row| row.0 == language) else {
        return;
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
