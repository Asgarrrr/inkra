//! A note typed into, as the editor sees it, for `benches/layout.rs` (incremental layout spec, D11).

use gpui::{TextStyle, Window, px};
use inkra_editor::HeardLayer;
use inkra_highlight::{Batch, batch, dirty};
use inkra_markdown::{Markdown, parse};
use inkra_text::{Buffer, ByteOffset};

use crate::decorations::{Drawn, MarkStyle};
use crate::editor::layout::{BlockScrolls, DocumentLayout, LayoutInputs, shape_document};
use crate::highlighting::styled;
use crate::theme::builtin;

/// A note with its code blocks coloured, its layout shaped, and a caret that types `x`.
pub struct Typing {
    buffer: Buffer,
    markdown: Markdown,
    syntax: [HeardLayer<MarkStyle>; 1],
    style: TextStyle,
    scrolls: BlockScrolls,
    caret: ByteOffset,
    layout: Option<DocumentLayout>,
    /// The layout the last `shape` replaced, dropped by the next `keystroke`, outside the timing.
    retired: Option<DocumentLayout>,
}

impl Typing {
    /// Shapes `text` from scratch, its blocks lexed, with the caret at byte `caret`.
    ///
    /// # Panics
    ///
    /// If `caret` is past the end of `text` or not on a char boundary.
    #[must_use]
    pub fn new(text: String, caret: usize, window: &mut Window) -> Self {
        let buffer = Buffer::new(text);
        let markdown = parse(buffer.text());
        let mut syntax = [HeardLayer::new(buffer.edit_mark())];
        let Batch { lexed, .. } = batch(buffer.text(), &markdown, &[], None);
        syntax[0].land(&buffer, lexed.into_iter().map(styled).collect());
        let mut typing = Self {
            buffer,
            markdown,
            syntax,
            style: window.text_style(),
            scrolls: BlockScrolls::default(),
            caret: ByteOffset::new(caret),
            layout: None,
            retired: None,
        };
        typing.shape(window);
        typing
    }

    /// Types `x` at the caret, then parses and recolours as the editor's listeners do.
    pub fn keystroke(&mut self) {
        self.retired = None;
        let mark = self.buffer.edit_mark();
        let caret = self.caret;
        self.buffer.transact(|txn| txn.insert(caret, "x"));
        self.caret = caret + 1;
        self.markdown = parse(self.buffer.text());
        let [syntax] = &mut self.syntax;
        syntax.catch_up(&self.buffer);
        let held: Vec<_> = syntax.layer().chunks().collect();
        let dirty = self.buffer.edits_since(mark).and_then(dirty);
        let Batch { clear, lexed } = batch(self.buffer.text(), &self.markdown, &held, dirty);
        let lexed = lexed.into_iter().map(styled).collect();
        syntax.publish(&self.buffer, clear, lexed, self.buffer.edit_mark());
    }

    /// Lays the note out from the previous layout, as a keystroke's frame does; from scratch at first.
    pub fn shape(&mut self, window: &mut Window) {
        let next = self.shape_from(self.layout.as_ref(), window);
        self.retired = self.layout.replace(next);
    }

    fn shape_from(&self, previous: Option<&DocumentLayout>, window: &mut Window) -> DocumentLayout {
        let selection = self.caret..self.caret;
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
            decorations: Drawn::for_tests(&self.syntax, self.buffer.edit_mark()),
        };
        shape_document(&inputs, previous, window)
    }
}
