//! Highlighting colours a fenced block's code by token class (code highlighting spec, S1a).

use gpui::{Hsla, VisualTestContext, WindowHandle};

use super::decorations::opened;
use super::{at, editor};
use crate::decorations;
use crate::feature::Manifest;
use crate::highlighting;
use crate::shell::AppShell;
use crate::theme::{self, ColorToken};

static CATALOG: &[&Manifest] = &[&highlighting::MANIFEST];

/// The note's colours, as offsets and tokens.
fn marks(
    window: WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
) -> Vec<(usize, usize, ColorToken)> {
    let buffer = editor(&window, cx).read_with(cx, |editor, _| editor.buffer().clone());
    cx.update(|_, cx| {
        let len = buffer.read(cx).len();
        decorations::drawn(&buffer, cx)
            .colors_in(at(0)..len)
            .into_iter()
            .map(|(range, token)| (range.start.get(), range.end.get(), token))
            .collect()
    })
}

#[gpui::test]
fn a_rust_block_colours_its_keywords_and_leaves_prose(cx: &mut gpui::TestAppContext) {
    let (_, window, mut visual) = opened(cx, CATALOG, "prose fn here\n\n```rust\nlet mut\n```\n");

    assert_eq!(
        marks(window, &mut visual),
        [
            (23, 26, ColorToken::SyntaxKeyword),
            (27, 30, ColorToken::SyntaxKeyword)
        ]
    );
    let editor = editor(&window, &mut visual);
    let (shown, runs, keyword, code) = visual.update(|_, cx| {
        let colors = &theme::current(cx).colors;
        let (shown, runs) = editor.read(cx).line_colors(3);
        (
            shown,
            runs,
            Hsla::from(colors.syntax_keyword),
            Hsla::from(colors.md_code_block),
        )
    });
    assert_eq!(shown, "let mut");
    assert_eq!(runs, [(3, keyword), (1, code), (3, keyword)]);
}
