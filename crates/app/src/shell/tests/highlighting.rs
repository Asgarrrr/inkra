//! Highlighting colours a fenced block's code by token class (code highlighting spec, S1a).

use std::time::Instant;

use gpui::{Hsla, TestAppContext, VisualTestContext, WindowHandle};

use super::decorations::opened;
use super::features::switch;
use super::{at, editor};
use crate::decorations;
use crate::feature::Manifest;
use crate::highlighting;
use crate::shell::AppShell;
use crate::theme::{self, ColorToken};

static CATALOG: &[&Manifest] = &[&highlighting::MANIFEST];

const K: ColorToken = ColorToken::SyntaxKeyword;

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

/// The colours of `text` once opened.
fn coloured(cx: &mut TestAppContext, text: &str) -> Vec<(usize, usize, ColorToken)> {
    let (_, window, mut visual) = opened(cx, CATALOG, text);
    marks(window, &mut visual)
}

#[gpui::test]
fn a_quote_block_with_an_empty_line_colours_its_content_not_its_prefix(cx: &mut TestAppContext) {
    let marks = coloured(cx, "> ```rust\n> let\n>\n> mut\n> ```\n");

    assert_eq!(marks, [(12, 15, K), (20, 23, K)]);
}

#[gpui::test]
fn a_list_item_block_with_an_empty_line_colours_its_content_not_its_indent(
    cx: &mut TestAppContext,
) {
    let marks = coloured(cx, "- a\n\n  ```rust\n  let\n\n  mut\n  ```\n");

    assert_eq!(marks, [(17, 20, K), (24, 27, K)]);
}

#[gpui::test]
fn an_indented_fence_colours_its_content(cx: &mut TestAppContext) {
    let marks = coloured(cx, "  ```rust\n  let\n  ```\n");

    assert_eq!(marks, [(12, 15, K)]);
}

#[gpui::test]
fn no_known_language_means_no_colour(cx: &mut TestAppContext) {
    let marks = coloured(cx, "```nope\nlet\n```\n\n```\nlet\n```\n\n    let\n");

    assert_eq!(marks, []);
}

#[gpui::test]
fn the_info_word_matches_in_any_case(cx: &mut TestAppContext) {
    let marks = coloured(cx, "```RS\nlet\n```\n");

    assert_eq!(marks, [(6, 9, K)]);
}

#[gpui::test]
fn the_fence_text_is_never_lexed(cx: &mut TestAppContext) {
    let (_, window, mut visual) = opened(cx, CATALOG, "```rs mut\nlet\n```\n");

    assert_eq!(marks(window, &mut visual), [(10, 13, K)]);
    visual.simulate_keystrokes("cmd-up");
    let editor = editor(&window, &mut visual);
    let (shown, runs, marker) = visual.update(|_, cx| {
        let (shown, runs) = editor.read(cx).line_colors(0);
        (shown, runs, Hsla::from(theme::current(cx).colors.md_marker))
    });
    assert_eq!(shown, "```rs mut");
    assert!(runs.iter().all(|(_, color)| *color == marker), "{runs:?}");
}

#[gpui::test]
fn typing_recolours_the_block(cx: &mut TestAppContext) {
    let (_, window, mut visual) = opened(cx, CATALOG, "```rust\nle\n```\n");
    assert_eq!(marks(window, &mut visual), []);

    visual.simulate_keystrokes("cmd-up down right right");
    visual.simulate_input("t");
    visual.run_until_parked();

    assert_eq!(marks(window, &mut visual), [(8, 11, K)]);
}

#[gpui::test]
fn a_changed_info_word_clears_the_block(cx: &mut TestAppContext) {
    let (_, window, mut visual) = opened(cx, CATALOG, "```rust\nlet\n```\n");
    assert_eq!(marks(window, &mut visual), [(8, 11, K)]);

    let buffer =
        editor(&window, &mut visual).read_with(&visual, |editor, _| editor.buffer().clone());
    buffer.update(&mut visual, |buffer, cx| {
        buffer.transact_at(Instant::now(), |buffer| buffer.delete(at(6)..at(7)));
        cx.notify();
    });
    visual.run_until_parked();

    assert_eq!(marks(window, &mut visual), []);
}

#[gpui::test]
fn each_block_colours_and_the_prose_between_stays_plain(cx: &mut TestAppContext) {
    let marks = coloured(cx, "```rust\nlet\n```\nfn prose\n```rust\nmut\n```\n");

    assert_eq!(marks, [(8, 11, K), (33, 36, K)]);
}

#[gpui::test]
fn switching_the_feature_off_clears_the_colours_and_on_restores_them(cx: &mut TestAppContext) {
    let (fs, window, mut visual) = opened(cx, CATALOG, "```rust\nlet\n```\n");
    let editor = editor(&window, &mut visual);
    let runs = |visual: &mut VisualTestContext| {
        editor.read_with(visual, |editor, _| editor.line_colors(1).1)
    };
    let code = visual.update(|_, cx| Hsla::from(theme::current(cx).colors.md_code_block));
    let coloured = runs(&mut visual);
    assert_eq!(marks(window, &mut visual), [(8, 11, K)]);

    switch(&fs, "highlighting", false, &mut visual);
    assert_eq!(marks(window, &mut visual), []);
    assert_eq!(runs(&mut visual), [(3, code)]);

    switch(&fs, "highlighting", true, &mut visual);
    assert_eq!(marks(window, &mut visual), [(8, 11, K)]);
    assert_eq!(runs(&mut visual), coloured);
}
