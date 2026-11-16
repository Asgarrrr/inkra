//! Code blocks coloured by token class, as a built-in feature with one layer (code highlighting
//! spec, D6).

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{App, Entity};
use inkra_highlight::{Batch, BlockChunk, TokenClass, batch};
use inkra_text::{Buffer, ByteOffset, Splice};

use crate::decorations::{self, Chunk, LayerDecl, LayerId, MarkStyle, TextChange};
use crate::editor::parses::parsed;
use crate::feature::{FeatureId, Manifest};
use crate::theme::ColorToken;

mod open;
mod task;

use task::Flights;

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
    activate: |fcx| fcx.on_text_change(SYNTAX, listener(Rc::default())),
};

/// The syntax layer's listener; it owns `flights`, each buffer's background task (D10).
fn listener(flights: Rc<RefCell<Flights>>) -> impl Fn(&Entity<Buffer>, TextChange, &mut App) {
    move |buffer, change, cx| hear(&flights, buffer, change, cx)
}

/// Replans `buffer` from what `change` says moved (D7, D10).
fn hear(flights: &Rc<RefCell<Flights>>, buffer: &Entity<Buffer>, change: TextChange, cx: &mut App) {
    let dirty = match change {
        TextChange::Edited(splices) => {
            Splice::compose(splices).map(|splice| splice.at..splice.at + splice.inserted_len)
        }
        TextChange::Lost | TextChange::Opened { fresh: false } => {
            Some(ByteOffset::new(0)..buffer.read(cx).len())
        }
        TextChange::Opened { fresh: true } => None,
    };
    relex(flights, buffer, dirty, cx);
}

/// Removes each chunk no longer a block's, and lexes each block no chunk holds exactly: in one
/// publish, those `dirty` meets within the main thread's budget; the rest in `buffer`'s background
/// task (D7, D10, deviations 6, 8, 9 and 10).
fn relex(
    flights: &Rc<RefCell<Flights>>,
    buffer: &Entity<Buffer>,
    dirty: Option<Range<ByteOffset>>,
    cx: &mut App,
) {
    let markdown = parsed(buffer, cx);
    let held = decorations::chunks(SYNTAX, buffer, cx);
    let text = buffer.read(cx);
    let Batch {
        clear,
        lexed,
        pending,
    } = batch(text.text(), &markdown, &held, dirty);
    let lexed = lexed.into_iter().map(styled).collect();
    let at = text.edit_mark();
    // Clears go first in the batch: a later change wins, and a lexed block may cover a clear.
    decorations::publish(SYNTAX, &buffer.downgrade(), clear, lexed, at, cx);
    task::spawn(flights, buffer, &markdown, &pending, cx);
}

/// A block chunk with each token class as its theme colour.
pub(crate) fn styled((range, shape, tokens): BlockChunk) -> Chunk {
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
