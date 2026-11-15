//! Code blocks coloured by token class, as a built-in feature with one layer (code highlighting
//! spec, D6).

use gpui::{App, Entity};
use inkra_highlight::{Batch, BlockChunk, TokenClass, batch, dirty};
use inkra_text::{Buffer, ByteOffset};

use crate::decorations::{self, Chunk, LayerDecl, LayerId, MarkStyle, TextChange};
use crate::editor::parses::parsed;
use crate::feature::{FeatureId, Manifest};
use crate::theme::ColorToken;

mod open;

const FEATURE: FeatureId = FeatureId::new("highlighting");

const SYNTAX: LayerId = LayerId::new(FEATURE, "syntax");

const SYNTAX_DECL: LayerDecl = LayerDecl {
    id: SYNTAX,
    open: Some(open::open),
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

/// Removes each chunk no longer a block's, then lexes the blocks the plan names, in one publish
/// (D7, deviations 6, 8 and 9).
fn listener(buffer: &Entity<Buffer>, change: TextChange, cx: &mut App) {
    let markdown = parsed(buffer, cx);
    let held = decorations::chunks(SYNTAX, buffer, cx);
    let text = buffer.read(cx);
    let dirty = match change {
        TextChange::Edited(splices) => dirty(splices),
        TextChange::Opened(Some(_)) => None,
        TextChange::Opened(None) | TextChange::Lost => Some(ByteOffset::new(0)..text.len()),
    };
    let Batch { clear, lexed } = batch(text.text(), &markdown, &held, dirty);
    let lexed = lexed.into_iter().map(styled).collect();
    let (at, buffer) = (text.edit_mark(), buffer.downgrade());
    // Clears go first in the batch: a later change wins, and a lexed block may cover a clear.
    decorations::publish(SYNTAX, &buffer, clear, lexed, at, cx);
}

/// A block chunk with each token class as its theme colour.
fn styled((range, shape, tokens): BlockChunk) -> Chunk {
    let marks = tokens
        .into_iter()
        .map(|(range, class)| {
            let color = Some(token(class));
            (range, MarkStyle { color })
        })
        .collect();
    (range, shape, marks)
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
