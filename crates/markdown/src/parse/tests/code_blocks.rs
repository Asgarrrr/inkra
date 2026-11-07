//! `Markdown::code_blocks` (code blocks spec): one per fenced or indented block, from the parse,
//! and the fence conceals the gap rule gives a fenced one.

use super::super::parse;
use super::{B, C, M, Q, concealed, styled};

/// Pairs, not ranges: clippy rejects a one-element array of ranges.
fn ranges(text: &str) -> Vec<(usize, usize)> {
    parse(text)
        .code_blocks
        .iter()
        .map(|block| (block.range.start.get(), block.range.end.get()))
        .collect()
}

#[test]
fn ranges_start_after_the_indent_and_end_before_the_line_ending() {
    assert_eq!(ranges("```rs\nlet x;\n```"), [(0, 16)]);
    assert_eq!(ranges("```\ncode\n```\n"), [(0, 12)]);
    assert_eq!(ranges("    code\n    more"), [(4, 17)]);
    assert_eq!(ranges("  ```rs\n  x\n  ```"), [(2, 17)]);
    assert_eq!(ranges("> ```\n> x\n> ```"), [(2, 15)]);
    assert_eq!(ranges("- a\n\n  ```\n  x\n  ```"), [(7, 20)]);
    assert_eq!(ranges("> - a\n>\n>   ```\n>   x\n>   ```"), [(12, 29)]);
    assert_eq!(ranges("```\ncode\n\n"), [(0, 9)]);
    assert_eq!(ranges("```\n```"), [(0, 7)]);
    assert_eq!(ranges("```rs"), [(0, 5)]);
    assert_eq!(ranges("~~~~py x\n~~~~"), [(0, 13)]);
    assert_eq!(ranges("a `b` c"), []);
    assert_eq!(ranges("```\na\n```\ntext\n\n    b"), [(0, 9), (20, 21)]);
}

#[test]
fn block_spans_carry_code_block_and_inline_code_does_not() {
    assert_eq!(
        styled("```rs\nlet x;\n```"),
        vec![("```rs", C | B | M), ("let x;", C | B), ("```", C | B | M)]
    );
    let text = "a `b` c";
    let spans = parse(text).spans;
    assert!(spans.iter().all(|span| !span.styles.contains(B)));
    assert!(spans.iter().any(|span| {
        span.styles.contains(C) && &text[span.range.start.get()..span.range.end.get()] == "b"
    }));
}

/// Pairs, as in `ranges`: the markers of the first block's fence conceals.
fn fences(text: &str) -> Vec<(usize, usize)> {
    let markdown = parse(text);
    let block = markdown.code_blocks[0].range.clone();
    markdown
        .conceals
        .iter()
        .filter(|conceal| conceal.element == block)
        .map(|conceal| (conceal.marker.start.get(), conceal.marker.end.get()))
        .collect()
}

#[test]
fn a_mid_line_gap_keeps_its_leading_blank() {
    assert!(concealed("*a  \nb*").contains(&("  ", "*a  \nb*", "")));
}

#[test]
fn fence_conceals_start_after_the_container_prefix() {
    assert_eq!(
        concealed("```rs\nlet x;\n```"),
        vec![
            ("```rs", "```rs\nlet x;\n```", ""),
            ("```", "```rs\nlet x;\n```", "")
        ]
    );
    assert_eq!(fences("> ```\n> x\n> ```"), [(2, 5), (12, 15)]);
    assert_eq!(fences("- a\n\n  ```\n  x\n  ```"), [(7, 10), (17, 20)]);
    assert_eq!(
        fences("> - a\n>\n>   ```\n>   x\n>   ```"),
        [(12, 15), (26, 29)]
    );
}

#[test]
fn an_empty_block_conceals_both_fences() {
    assert_eq!(fences("```\n```"), [(0, 3), (4, 7)]);
    assert_eq!(fences("~~~~py x\n~~~~"), [(0, 8), (9, 13)]);
}

#[test]
fn an_unclosed_block_conceals_its_opening_fence_only() {
    assert_eq!(fences("```rs"), [(0, 5)]);
    assert_eq!(fences("```rs\nx"), [(0, 5)]);
    assert_eq!(concealed("```\ncode\n\n"), vec![("```", "```\ncode\n", "")]);
}

#[test]
fn an_indented_block_conceals_nothing_and_its_indents_are_body_markers() {
    assert_eq!(concealed("    code\n    more"), vec![]);
    assert_eq!(
        styled("    code\n    more"),
        vec![("code", C | B), ("    ", M), ("more", C | B)]
    );
}

#[test]
fn indents_inside_a_block_carry_the_parents_styles() {
    assert_eq!(
        styled("- a\n\n  ```\n  x\n  ```"),
        vec![
            ("- ", M),
            ("  ", M),
            ("```", C | B | M),
            ("  ", M),
            ("x", C | B),
            ("  ", M),
            ("```", C | B | M)
        ]
    );
    assert_eq!(
        styled("> - a\n>\n>   ```\n>   x\n>   ```"),
        vec![
            ("> - ", Q | M),
            ("a", Q),
            (">", Q | M),
            (">   ", Q | M),
            ("```", C | B | Q | M),
            (">   ", Q | M),
            ("x", C | B | Q),
            (">   ", Q | M),
            ("```", C | B | Q | M)
        ]
    );
}

/// The first block's language, as source text.
fn language(text: &str) -> Option<&str> {
    let range = parse(text).code_blocks[0].language.clone()?;
    Some(&text[range.start.get()..range.end.get()])
}

#[test]
fn the_language_is_the_info_strings_first_word() {
    assert_eq!(language("```rust ignore\nx\n```"), Some("rust"));
    assert_eq!(language("~~~~py x\n~~~~"), Some("py"));
    assert_eq!(language("```  rust  x\n```"), Some("rust"));
    assert_eq!(language("> ```sh\n> x\n> ```"), Some("sh"));
    assert_eq!(language("```a\\_b\n```"), Some("a\\_b"));
    assert_eq!(language("```é x\n```"), Some("é"));
    assert_eq!(language("```a\u{a0}b\n```"), Some("a\u{a0}b"));
    assert_eq!(language("```rs"), Some("rs"));
    assert_eq!(language("```\nx\n```"), None);
    assert_eq!(language("    code"), None);
}
