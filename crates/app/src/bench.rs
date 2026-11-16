//! A note typed into, as the editor sees it, for `benches/layout.rs` (incremental layout spec, D11).

use std::rc::Rc;

use gpui::{TextStyle, Window, px};
use inkra_editor::{HeardLayer, RevisionLog};
use inkra_highlight::{Batch, BlockChunk, backlog, batch, lex_backlog};
use inkra_markdown::{Markdown, parse};
use inkra_text::{Buffer, ByteOffset};

use crate::decorations::{Drawn, MarkStyle};
use crate::editor::layout::{BlockScrolls, DocumentLayout, LayoutInputs, shape_document};
use crate::highlighting::styled;
use crate::theme::builtin;

/// A note with its layout shaped, its code blocks coloured or not, and a caret that types `x` or moves.
pub struct Typing {
    buffer: Buffer,
    markdown: Rc<Markdown>,
    /// Empty and never published to without colours.
    syntax: [HeardLayer<MarkStyle>; 1],
    /// Bumped when the colours move, as `decorations::sync` and `publish` bump it.
    log: RevisionLog,
    colours: bool,
    style: TextStyle,
    scrolls: BlockScrolls,
    caret: ByteOffset,
    layout: Option<DocumentLayout>,
    /// The layout the last `shape` replaced, dropped by the next `keystroke` or `move_caret`, outside the timing.
    retired: Option<DocumentLayout>,
}

impl Typing {
    /// Shapes `text` from scratch, its blocks lexed when `colours`, with the caret at byte `caret`.
    ///
    /// # Panics
    ///
    /// If `caret` is past the end of `text` or not on a char boundary.
    #[must_use]
    pub fn new(text: String, caret: usize, colours: bool, window: &mut Window) -> Self {
        let buffer = Buffer::new(text);
        let markdown = Rc::new(parse(buffer.text()));
        let mut syntax = [HeardLayer::new(buffer.edit_mark())];
        if colours {
            let Batch { lexed, pending, .. } = batch(buffer.text(), &markdown, &[], None);
            let lexed = drained(buffer.text(), &markdown, lexed, &pending);
            syntax[0].land(&buffer, lexed.into_iter().map(styled).collect());
        }
        let mut typing = Self {
            buffer,
            markdown,
            syntax,
            log: RevisionLog::new(),
            colours,
            style: window.text_style(),
            scrolls: BlockScrolls::default(),
            caret: ByteOffset::new(caret),
            layout: None,
            retired: None,
        };
        typing.shape(window);
        typing
    }

    /// Types `x` at the caret, then parses, and recolours when `colours`, as the editor's listeners do.
    pub fn keystroke(&mut self) {
        self.retired = None;
        let caret = self.caret;
        self.buffer.transact(|txn| txn.insert(caret, "x"));
        self.caret = caret + 1;
        self.markdown = Rc::new(parse(self.buffer.text()));
        if !self.colours {
            return;
        }
        let [syntax] = &mut self.syntax;
        if let Some(moved) = syntax.catch_up(&self.buffer) {
            self.log.bump(self.log.revision() + 1, moved);
        }
        let held: Vec<_> = syntax.layer().chunks().collect();
        let (text, dirty) = (self.buffer.text(), Some(caret..caret + 1));
        let Batch {
            clear,
            lexed,
            pending,
        } = batch(text, &self.markdown, &held, dirty);
        let lexed = drained(text, &self.markdown, lexed, &pending);
        let lexed: Vec<_> = lexed.into_iter().map(styled).collect();
        let at = self.buffer.edit_mark();
        if let Some(moved) = syntax.publish(&self.buffer, clear, lexed, at) {
            self.log.bump(self.log.revision() + 1, moved);
        }
    }

    /// Moves the caret to byte `caret`, as a click does, and returns the byte it left. The text, the
    /// parse and the colours stay.
    pub fn move_caret(&mut self, caret: usize) -> usize {
        self.retired = None;
        std::mem::replace(&mut self.caret, ByteOffset::new(caret)).get()
    }

    /// Lays the note out from the previous layout, as a keystroke's frame does; from scratch at first.
    pub fn shape(&mut self, window: &mut Window) {
        let next = self.shape_from(self.layout.as_ref(), window);
        self.retired = self.layout.replace(next);
    }

    fn shape_from(&self, previous: Option<&DocumentLayout>, window: &mut Window) -> DocumentLayout {
        let selection = self.caret..self.caret;
        let decorations = Drawn::for_tests(&self.syntax, &self.log, self.buffer.edit_mark());
        let inputs = LayoutInputs {
            buffer: &self.buffer,
            markdown: &self.markdown,
            selection: &selection,
            marked: None,
            style: &self.style,
            wrap_width: Some(px(700.)),
            margin: px(76.),
            code_block_wrap: false,
            theme: builtin::dark(),
            scrolls: &self.scrolls,
            decorations,
        };
        shape_document(&inputs, previous, window)
    }
}

/// `lexed`, then the `pending` blocks, backlog after backlog, as the listener lexes them.
fn drained(
    text: &str,
    markdown: &Markdown,
    mut lexed: Vec<BlockChunk>,
    pending: &[usize],
) -> Vec<BlockChunk> {
    let mut rest = pending;
    while !rest.is_empty() {
        let landed = lex_backlog(backlog(text, markdown, rest));
        rest = &rest[landed.len()..];
        lexed.extend(landed);
    }
    lexed
}
