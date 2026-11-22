//! The cases the properties skip: a split the writer asked for, and the open questions the spec
//! leaves for `↩` (reported with list editing slice L1).

use inkra_text::ByteOffset;

use crate::Markdown;
use crate::list_edit::Doc;
use crate::spans::{line_end, line_start};

/// Whether the edit leaves a blank line right under an empty marker line, which then ends that item
/// before the lines it held: rule 7's blank line, or the line rule 2 leaves at its text start. The
/// spec's rule 7 assumes a paragraph above (open question, reported).
pub(super) fn blank_under_empty_marker(
    text: &str,
    markdown: &Markdown,
    caret: usize,
    new: &str,
) -> bool {
    let line = line_start(text, caret);
    let Some(above) = line.checked_sub(1).map(|end| line_start(text, end)) else {
        return false;
    };
    let blank = |text: &str, at: usize| {
        text[at..line_end(text, at)]
            .trim_matches(['>', ' ', '\t'])
            .is_empty()
    };
    let empty = markdown
        .gutter_on(ByteOffset::new(above))
        .filter(|gutter| gutter.item.as_ref().is_some_and(|item| item.marked))
        .is_some_and(|gutter| {
            text[gutter.range.end.get().min(line - 1)..line - 1]
                .trim()
                .is_empty()
        });
    empty && !blank(text, line) && blank(new, line)
}

/// Whether the text after the caret starts with a byte that can open a block, or the caret splits
/// its line's leading blanks, a code indent among them: splitting there changes the structure
/// because the writer asked for it, as the plain key would.
pub(super) fn splits_opener(text: &str, caret: usize) -> bool {
    let tail = &text[caret..line_end(text, caret)];
    let rest = tail.trim_start_matches([' ', '\t']);
    let lead = text[line_start(text, caret)..caret]
        .trim_matches([' ', '\t', '>'])
        .is_empty();
    rest.starts_with(|byte: char| byte.is_ascii_digit() || "-*+>#`~".contains(byte))
        || (lead && rest.len() < tail.len() && !rest.is_empty())
}

/// Whether the caret is on a code line that no item holds, in a quote: its gutter records no leaf
/// text start outside a list (task boxes spec, D11), so rule 2 cannot copy its code indent (open
/// question, reported).
pub(super) fn unheld_quoted_code(text: &str, markdown: &Markdown, caret: usize) -> bool {
    let line = line_start(text, caret);
    let coded = markdown.code_blocks.iter().any(|block| {
        line_start(text, block.range.start.get()) <= line && line <= block.range.end.get()
    });
    coded
        && Doc { text, markdown }.line_at(line).holder().is_none()
        && markdown.gutter_on(ByteOffset::new(line)).is_some()
}
