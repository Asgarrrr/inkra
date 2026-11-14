//! Code blocks coloured by token class, as a built-in feature with one layer (code highlighting
//! spec, D6).

use gpui::{App, Entity};
use inkra_highlight::{Language, TokenClass, highlight};
use inkra_text::{Buffer, ByteOffset};

use crate::decorations::{self, LayerDecl, LayerId, MarkStyle, TextChange};
use crate::feature::{FeatureId, Manifest};
use crate::theme::ColorToken;

mod lines;

const FEATURE: FeatureId = FeatureId::new("highlighting");

const SYNTAX: LayerId = LayerId::new(FEATURE, "syntax");

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

/// Lexes every code block of the note and publishes them all as one chunk over the whole text.
fn listener(buffer: &Entity<Buffer>, _change: TextChange, cx: &mut App) {
    let markdown = crate::editor::parses::parsed(buffer, cx);
    let text = buffer.read(cx);
    let mut tokens = Vec::new();
    for block in &markdown.code_blocks {
        let word = block.language.clone().map(|word| text.slice(word));
        if let Some(language) = word.and_then(Language::from_info) {
            let lines = lines::content_lines(text.text(), &markdown.spans, block);
            highlight(language, lines, &mut tokens);
        }
    }
    let marks = tokens
        .into_iter()
        .map(|(range, class)| {
            let color = Some(token(class));
            (range, MarkStyle { color })
        })
        .collect();
    let (chunk, at, buffer) = (
        ByteOffset::new(0)..text.len(),
        text.edit_mark(),
        buffer.downgrade(),
    );
    decorations::publish(SYNTAX, &buffer, chunk, marks, at, cx);
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
    }
}
