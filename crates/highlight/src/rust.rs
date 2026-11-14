//! Rust: keywords (code highlighting spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;

/// `Self`, `true`, `false`, `union` and `macro_rules` are not keywords here.
const KEYWORDS: [&str; 35] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "type", "unsafe", "use", "where",
    "while",
];

/// A word char is ASCII alphanumeric, `_` or non-ASCII, so a separator is one ASCII byte.
fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_' || !char.is_ascii()
}

pub(crate) fn lex(line: &str, start: ByteOffset, out: &mut Vec<(Range<ByteOffset>, TokenClass)>) {
    let mut word_start = start;
    for word in line.split(|char| !is_word(char)) {
        if KEYWORDS.contains(&word) {
            out.push((word_start..word_start + word.len(), TokenClass::Keyword));
        }
        word_start = word_start + word.len() + 1;
    }
}

#[cfg(test)]
mod tests;
