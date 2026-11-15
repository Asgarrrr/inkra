//! Code blocks coloured by token class, as a built-in feature with one layer (code highlighting
//! spec, D6).

#[cfg(test)]
use std::cell::Cell;
use std::ops::Range;

use gpui::{App, Entity};
use inkra_highlight::{Language, Plan, TokenClass, dirty, highlight, plan};
use inkra_markdown::{CodeBlock, Span};
use inkra_text::{Buffer, ByteOffset};

use crate::decorations::{self, LayerDecl, LayerId, MarkStyle, TextChange};
use crate::editor::parses::parsed;
use crate::feature::{FeatureId, Manifest};
use crate::theme::ColorToken;

mod lines;

const FEATURE: FeatureId = FeatureId::new("highlighting");

const SYNTAX: LayerId = LayerId::new(FEATURE, "syntax");

#[cfg(test)]
thread_local! { static LEXED: Cell<usize> = const { Cell::new(0) }; }

const SYNTAX_DECL: LayerDecl = LayerDecl {
    id: SYNTAX,
    open: None,
};

pub(crate) static MANIFEST: Manifest = Manifest {
    id: FEATURE,
    default_on: true,
    after_first_frame: true,
    commands: &[],
    panes: &[],
    buttons: &[],
    layers: &[SYNTAX_DECL],
    activate: |fcx| fcx.on_text_change(SYNTAX, listener),
};

/// Clears each chunk no longer a block's, then lexes the blocks the plan names (D7).
fn listener(buffer: &Entity<Buffer>, change: TextChange, cx: &mut App) {
    let markdown = parsed(buffer, cx);
    let held = decorations::chunks(SYNTAX, buffer, cx);
    let text = buffer.read(cx);
    let dirty = match change {
        TextChange::Edited(splices) => dirty(splices),
        TextChange::Opened(Some(_)) => None,
        TextChange::Opened(None) | TextChange::Lost => Some(ByteOffset::new(0)..text.len()),
    };
    let blocks: Vec<_> = markdown
        .code_blocks
        .iter()
        .map(|block| (block.range.clone(), text.line_at(block.range.start).start))
        .collect();
    let Plan { clear, lex } = plan(&blocks, &held, dirty);
    #[cfg(test)]
    LEXED.with(|lexed| lexed.set(lexed.get() + lex.len()));
    // Clears first: at one edit mark the later publish wins, and a lexed block may cover a clear.
    let publishes: Vec<_> = clear
        .into_iter()
        .map(|chunk| (chunk, Vec::new()))
        .chain(lex.into_iter().map(|index| {
            let block = &markdown.code_blocks[index];
            (
                block.range.clone(),
                colour(text.text(), &markdown.spans, block),
            )
        }))
        .collect();
    let (at, buffer) = (text.edit_mark(), buffer.downgrade());
    for (chunk, marks) in publishes {
        decorations::publish(SYNTAX, &buffer, chunk, marks, at, cx);
    }
}

/// `block`'s marks, none when its info word names no known language.
fn colour(text: &str, spans: &[Span], block: &CodeBlock) -> Vec<(Range<ByteOffset>, MarkStyle)> {
    let language = block
        .language
        .clone()
        .map(|word| &text[word.start.get()..word.end.get()])
        .and_then(Language::from_info);
    let Some(language) = language else {
        return Vec::new();
    };
    let mut tokens = Vec::new();
    highlight(
        language,
        lines::content_lines(text, spans, block),
        &mut tokens,
    );
    tokens
        .into_iter()
        .map(|(range, class)| {
            let color = Some(token(class));
            (range, MarkStyle { color })
        })
        .collect()
}

fn token(class: TokenClass) -> ColorToken {
    match class {
        TokenClass::Comment => ColorToken::SyntaxComment,
        TokenClass::Keyword => ColorToken::SyntaxKeyword,
        TokenClass::String => ColorToken::SyntaxString,
        TokenClass::Constant => ColorToken::SyntaxConstant,
        TokenClass::Function => ColorToken::SyntaxFunction,
        TokenClass::Type => ColorToken::SyntaxType,
        TokenClass::Property => ColorToken::SyntaxProperty,
        TokenClass::Attribute => ColorToken::SyntaxAttribute,
        TokenClass::Punctuation => ColorToken::SyntaxPunctuation,
        TokenClass::Operator => ColorToken::SyntaxOperator,
    }
}

#[cfg(test)]
mod tests;
