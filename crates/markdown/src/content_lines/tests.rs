use super::content_lines;
use crate::parse;

/// Asserts the content lines of `text`'s one code block, offsets as `usize`.
fn assert_lines(text: &str, expected: &[(usize, &str)]) {
    let markdown = parse(text);
    let [block] = &markdown.code_blocks[..] else {
        panic!("{text:?} has no single code block");
    };
    let lines: Vec<_> = content_lines(text, &markdown.spans, block)
        .map(|(start, line)| (start.get(), line))
        .collect();
    assert_eq!(lines, expected, "{text:?}");
}

#[test]
fn a_fence_line_is_empty_at_its_end_and_a_code_line_is_its_span() {
    assert_lines("```rust\nlet x\n```", &[(7, ""), (8, "let x"), (17, "")]);
    assert_lines("~~~~py x\n~~~~", &[(8, ""), (13, "")]);
}

#[test]
fn a_block_in_a_quote_yields_its_code_alone() {
    assert_lines(
        "> ```rust\n> let\n>\n> mut\n> ```",
        &[(9, ""), (12, "let"), (17, ""), (20, "mut"), (29, "")],
    );
}

#[test]
fn a_block_in_a_list_item_yields_an_empty_line_at_its_end() {
    assert_lines(
        "- a\n\n  ```rust\n  x\n\n  y\n  ```",
        &[(14, ""), (17, "x"), (19, ""), (22, "y"), (29, "")],
    );
}

#[test]
fn an_indented_block_yields_its_code_without_the_indent() {
    assert_lines("    let x\n    y", &[(4, "let x"), (14, "y")]);
}
