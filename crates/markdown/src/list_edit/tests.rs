//! A line's holder and the container prefix (list editing spec, D1, D2).

use super::Doc;
use crate::parse;

/// The holder's marker, for the line starting at `line`.
fn holder_marker(text: &str, line: usize) -> Option<usize> {
    let markdown = parse(text);
    let doc = Doc {
        text,
        markdown: &markdown,
    };
    doc.holder(line)
        .map(|at| markdown.structure.items[at].marker.get())
}

#[test]
fn a_lines_holder_is_the_innermost_gutter_item_holding_it() {
    assert_eq!(holder_marker("- a\n  - b\n\n  c", 11), Some(0));
    assert_eq!(holder_marker("- a\n  - b\n    c", 10), Some(6));
    assert_eq!(holder_marker("- a\nb", 4), Some(0));
    assert_eq!(holder_marker("- a\n\nb", 5), None);
    assert_eq!(holder_marker("a\nb", 2), None);
    assert_eq!(
        holder_marker("- > - a\n  b", 8),
        Some(0),
        "the inner item is in no gutter"
    );
}

/// `line` is the start of the line whose quotes the new line takes; `column` the holder's content
/// column.
#[test]
fn the_container_prefix_holds_the_new_line_in_its_quotes_and_items() {
    for (text, line, column, want) in [
        ("a", 0, 0, ""),
        ("- a", 0, 2, "  "),
        ("> a", 0, 0, "> "),
        ("> > a", 0, 0, "> > "),
        ("> - a", 0, 4, ">   "),
        ("  > a", 0, 0, "  > "),
        (">a", 0, 0, "> "),
        (">\t- a", 0, 6, ">     "),
        ("- a\n\tb", 4, 2, "  "),
        ("> > a\nb", 6, 0, "> > "),
        ("> a\n> > b\nc", 10, 0, "> > "),
        ("> a\n\n> b", 5, 0, "> "),
    ] {
        let markdown = parse(text);
        let doc = Doc {
            text,
            markdown: &markdown,
        };
        assert_eq!(
            doc.container_prefix(line, column),
            want,
            "{text:?} at {line}"
        );
    }
}
