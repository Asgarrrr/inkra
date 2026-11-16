//! JavaScript, and TypeScript through its dialect: comments, strings, template strings across
//! lines, regex literals, numbers, keywords, JSX, and identifiers in context (code highlighting
//! spec, D4).

use std::ops::Range;

use inkra_text::ByteOffset;

use crate::TokenClass::{
    self, Attribute, Comment, Constant, Function, Keyword, Property, String, Type,
};
use crate::scan::{
    Sink, block_comment, dotted_end, drive, generic_end, is_all_caps, is_ident_start, is_word,
    number, quoted, skip_spaces,
};
use crate::state::{LineState, Nest, Open, Prev};
use crate::typescript;

mod jsx;
mod literals;
mod symbols;

/// What a dialect adds to JavaScript (Languages).
#[derive(Clone, Copy)]
pub(crate) struct Dialect {
    /// `<` at an expression start opens a JSX tag.
    pub(crate) jsx: bool,
    /// TypeScript's type keywords, annotations and generic lists.
    pub(crate) types: bool,
}

/// The reserved words, and `async`; `this` and `super` stay plain, as in Zed.
const KEYWORDS: [&str; 35] = [
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "return",
    "switch",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Keywords only before an operand on their line; names everywhere else.
const SOFT_KEYWORDS: [&str; 6] = ["as", "from", "get", "of", "set", "static"];

/// Objects Zed colours as types before a `.`.
const BUILTIN_OBJECTS: [&str; 25] = [
    "Array",
    "ArrayBuffer",
    "BigInt",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "Function",
    "Map",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "RegExp",
    "Set",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "WeakMap",
    "WeakSet",
];

/// An identifier byte: a word byte or `$`.
fn is_ident(byte: u8) -> bool {
    is_word(byte) || byte == b'$'
}

fn ident_end(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| !is_ident(byte))
        .map_or(bytes.len(), |len| at + len)
}

/// Whether `byte` can start what a soft keyword takes: a name, a literal, a pattern or `*`.
fn starts_operand(byte: Option<&u8>) -> bool {
    byte.is_some_and(|&byte| is_ident_start(byte) || b"$\"'`{[*#".contains(&byte))
}

/// The look-behind past a keyword after `prev`; a modifier keeps a key's start.
fn after_keyword(keyword: &str, prev: Prev) -> Prev {
    match keyword {
        "abstract" | "async" | "declare" | "override" | "private" | "protected" | "public"
        | "readonly" | "static"
            if prev == Prev::KeyStart =>
        {
            Prev::KeyStart
        }
        "function" => Prev::Fn,
        "class" | "enum" | "extends" | "implements" | "interface" | "namespace" | "new"
        | "type" => Prev::TypeKeyword,
        "if" | "while" | "for" | "with" => Prev::Condition,
        "let" | "const" | "var" => Prev::Declare,
        _ => Prev::Other,
    }
}

/// One line's tokens.
struct Tokens<'a> {
    line: &'a str,
    sink: Sink<'a>,
    state: LineState,
    dialect: Dialect,
}

impl Tokens<'_> {
    /// Whether the look-behind ends a value, so `/` divides and `<` compares (D4).
    fn value_end(&self) -> bool {
        matches!(self.state.prev, Prev::ValueEnd | Prev::Type | Prev::FnName)
    }

    /// Lexes what starts at `at`, with nothing open there, by the innermost nesting: its end.
    fn step(&mut self, at: usize) -> usize {
        match self.state.top() {
            Some(Nest::Template) => self.template(at, at),
            Some(Nest::JsxTag) => self.jsx_tag(at, false),
            Some(Nest::JsxClosingTag) => self.jsx_tag(at, true),
            Some(Nest::JsxText) => self.jsx_text(at),
            _ => self.code(at),
        }
    }

    fn code(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let next = bytes.get(at + 1).copied();
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at + 1,
            b'/' if next == Some(b'/') => self.line_comment(at),
            b'/' if next == Some(b'*') => self.block_comment(at, at + 2),
            b'#' if at == 0 && next == Some(b'!') => self.line_comment(at),
            b'"' | b'\'' => {
                let end = quoted(bytes, at + 1, bytes[at], false, true).unwrap_or(bytes.len());
                self.sink.push(at..end, String);
                self.state.prev = Prev::ValueEnd;
                end
            }
            b'`' => {
                self.state.push(Nest::Template);
                self.template(at, at + 1)
            }
            b'.' if next.is_some_and(|byte| byte.is_ascii_digit()) => self.number(at),
            byte if byte.is_ascii_digit() => self.number(at),
            b'@' if next.is_some_and(is_ident_start) => {
                let end = dotted_end(bytes, at + 1);
                self.sink.push(at..end, Attribute);
                self.state.prev = Prev::Other;
                end
            }
            b'#' if next.is_some_and(is_ident_start) => {
                let end = ident_end(bytes, at + 1);
                let call = bytes.get(skip_spaces(bytes, end)) == Some(&b'(');
                self.sink
                    .push(at..end, if call { Function } else { Property });
                self.state.prev = Prev::ValueEnd;
                end
            }
            byte if is_ident(byte) => self.word(at),
            b'/' if !self.value_end() => self.regex(at),
            b'<' => self.angle(at),
            _ => self.symbol(at),
        }
    }

    fn line_comment(&mut self, at: usize) -> usize {
        let end = self.line.len();
        self.sink.push(at..end, Comment);
        end
    }

    fn block_comment(&mut self, token: usize, at: usize) -> usize {
        let end;
        (end, self.state.open) = block_comment(&mut self.sink, self.line.as_bytes(), token, at);
        end
    }

    fn number(&mut self, at: usize) -> usize {
        let end = number(self.line.as_bytes(), at);
        self.sink.push(at..end, Constant);
        self.state.prev = Prev::ValueEnd;
        end
    }

    fn word(&mut self, at: usize) -> usize {
        let end = ident_end(self.line.as_bytes(), at);
        let (class, prev) = self.classify(&self.line[at..end], end);
        if let Some(class) = class {
            self.sink.push(at..end, class);
        }
        self.state.prev = prev;
        end
    }

    fn keyword(&self, word: &str, after: usize) -> bool {
        let soft = || {
            SOFT_KEYWORDS.contains(&word)
                || (self.dialect.types && typescript::SOFT_KEYWORDS.contains(&word))
        };
        // Every keyword starts with a lower-case letter, so most names skip the lists.
        word.starts_with(|char: char| char.is_ascii_lowercase())
            && (KEYWORDS.contains(&word)
                || (starts_operand(self.line.as_bytes().get(after)) && soft()))
    }

    /// The class of the identifier `word` that ends at `end`, and the look-behind past it.
    fn classify(&self, word: &str, end: usize) -> (Option<TokenClass>, Prev) {
        let bytes = self.line.as_bytes();
        let after = skip_spaces(bytes, end);
        let types = self.dialect.types;
        let generic_call = types
            && generic_end(bytes, after)
                .is_some_and(|close| bytes.get(skip_spaces(bytes, close)) == Some(&b'('));
        let call = matches!(bytes.get(after), Some(b'(' | b'`')) || generic_call;
        let past_call = if generic_call {
            Prev::FnName
        } else {
            Prev::ValueEnd
        };
        let literal = matches!(word, "true" | "false" | "null" | "undefined");
        let colon = bytes.get(after) == Some(&b':') || bytes[after..].starts_with(b"?:");
        let prev = self.state.prev;
        let top = self.state.top();
        if top == Some(Nest::Generic) {
            return match word {
                _ if literal => (Some(Constant), Prev::ValueEnd),
                _ if self.keyword(word, after) => (Some(Keyword), Prev::Other),
                _ => (Some(Type), Prev::Type),
            };
        }
        if prev == Prev::Dot {
            return (Some(if call { Function } else { Property }), past_call);
        }
        let bare = word.trim_start_matches('_');
        match word {
            _ if literal => (Some(Constant), Prev::ValueEnd),
            "this" | "super" => (None, Prev::ValueEnd),
            _ if self.keyword(word, after) => (Some(Keyword), after_keyword(word, prev)),
            "constructor" if call => (Some(Type), Prev::ValueEnd),
            _ if colon && prev == Prev::KeyStart && top == Some(Nest::Brace) => {
                // `?:` cannot follow an object literal's key, so it marks a member's type.
                let optional = types && bytes[after..].starts_with(b"?:");
                (
                    Some(Property),
                    if optional {
                        Prev::Param
                    } else {
                        Prev::ValueEnd
                    },
                )
            }
            _ if types
                && colon
                && (prev == Prev::Declare
                    || (prev == Prev::KeyStart && top == Some(Nest::Paren))) =>
            {
                (None, Prev::Param)
            }
            _ if prev == Prev::Fn => (Some(Function), Prev::FnName),
            _ if (prev == Prev::TypeKeyword && bytes.get(after) != Some(&b'.'))
                || prev == Prev::Annotation =>
            {
                (Some(Type), Prev::Type)
            }
            // A lone capital is a constant, as in Zed, but in TypeScript a type parameter.
            _ if is_all_caps(bare) || (!types && matches!(bare.as_bytes(), [b'A'..=b'Z'])) => {
                (Some(Constant), Prev::ValueEnd)
            }
            _ if types
                && (bare.starts_with(|char: char| char.is_ascii_uppercase())
                    || typescript::BUILTIN_TYPES.contains(&word)) =>
            {
                (Some(Type), Prev::Type)
            }
            _ if bytes.get(after) == Some(&b'.') && BUILTIN_OBJECTS.contains(&word) => {
                (Some(Type), Prev::Type)
            }
            _ if call => (Some(Function), past_call),
            _ => (None, Prev::ValueEnd),
        }
    }
}

pub(crate) fn lex(
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    lex_dialect(
        Dialect {
            jsx: true,
            types: false,
        },
        state,
        line,
        start,
        out,
    )
}

/// JavaScript's lexer with `dialect`'s additions.
pub(crate) fn lex_dialect(
    dialect: Dialect,
    state: LineState,
    line: &str,
    start: ByteOffset,
    out: &mut Vec<(Range<ByteOffset>, TokenClass)>,
) -> LineState {
    let mut tokens = Tokens {
        line,
        sink: Sink { start, out },
        state,
        dialect,
    };
    let at = if let Open::BlockComment { .. } = state.open {
        tokens.block_comment(0, 0)
    } else {
        tokens.state.open = Open::Nothing;
        0
    };
    drive(line.len(), at, |at| {
        (tokens.state.open == Open::Nothing).then(|| tokens.step(at))
    });
    tokens.state
}

#[cfg(test)]
mod tests;
