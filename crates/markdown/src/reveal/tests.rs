use std::ops::Range;

use inkra_text::ByteOffset;

use super::{Conceal, is_active, touched};

fn range(start: usize, end: usize) -> Range<ByteOffset> {
    ByteOffset(start)..ByteOffset(end)
}

fn bold() -> Conceal {
    // "a **b** c": the opening `**`.
    Conceal {
        element: range(2, 7),
        marker: range(2, 4),
        replacement: "",
    }
}

#[test]
fn a_caret_away_from_the_element_leaves_it_hidden() {
    assert!(is_active(&bold(), &range(0, 0)));
    assert!(is_active(&bold(), &range(1, 1)));
    assert!(is_active(&bold(), &range(8, 8)));
}

#[test]
fn a_caret_at_either_end_or_inside_reveals_it() {
    for caret in [2, 3, 5, 7] {
        assert!(!is_active(&bold(), &range(caret, caret)), "caret {caret}");
    }
}

#[test]
fn a_selection_reveals_what_it_touches() {
    assert!(!is_active(&bold(), &range(0, 2)));
    assert!(!is_active(&bold(), &range(7, 9)));
    assert!(is_active(&bold(), &range(0, 1)));
}

#[test]
fn touched_lists_each_touched_element_once_per_conceal() {
    let closing = Conceal {
        element: range(2, 7),
        marker: range(5, 7),
        replacement: "",
    };
    let other = Conceal {
        element: range(10, 13),
        marker: range(10, 11),
        replacement: "",
    };
    let conceals = [bold(), closing, other];
    let at_bold = range(3, 3);
    assert_eq!(touched(&conceals, &at_bold).count(), 2);
    let away = range(8, 8);
    assert_eq!(touched(&conceals, &away).count(), 0);
    let inside_bold = range(4, 4);
    assert!(touched(&conceals, &at_bold).eq(touched(&conceals, &inside_bold)));
}
