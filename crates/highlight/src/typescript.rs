//! TypeScript and TSX: JavaScript's lexer with type keywords, annotations and generic lists; TSX
//! turns JSX back on (Languages).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass;
use crate::javascript::{Dialect, lex_dialect};
use crate::state::LineState;

/// Zed's TypeScript keywords that JavaScript lacks; each a keyword only before an operand.
pub(crate) const SOFT_KEYWORDS: [&str; 16] = [
    "abstract",
    "declare",
    "implements",
    "infer",
    "interface",
    "is",
    "keyof",
    "module",
    "namespace",
    "override",
    "private",
    "protected",
    "public",
    "readonly",
    "satisfies",
    "type",
];

/// Zed's `predefined_type`s but `void`, a keyword.
pub(crate) const BUILTIN_TYPES: [&str; 9] = [
    "any", "bigint", "boolean", "never", "number", "object", "string", "symbol", "unknown",
];

pub(crate) fn lex(
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    let dialect = Dialect {
        jsx: false,
        types: true,
    };
    lex_dialect(dialect, state, line, start, out)
}

pub(crate) fn lex_tsx(
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    let dialect = Dialect {
        jsx: true,
        types: true,
    };
    lex_dialect(dialect, state, line, start, out)
}

#[cfg(test)]
mod tests;
