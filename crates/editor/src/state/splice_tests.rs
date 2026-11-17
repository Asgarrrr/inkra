//! `splice`: `⌘L`'s edit on one line (task boxes spec, D6).

use inkra_text::Hunk;
use proptest::prelude::*;

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset::new(offset)
}

fn hunk(range: Range<usize>, text: &str) -> Hunk {
    Hunk {
        range: at(range.start)..at(range.end),
        text: text.to_owned(),
    }
}

/// `text` spliced from a caret at `caret`.
fn spliced(text: &str, caret: usize, splice: &Hunk) -> (Buffer, EditorState) {
    let mut buffer = Buffer::new(text);
    let mut state = EditorState::new(Selection::at(at(caret)));
    state.splice(&mut buffer, splice);
    (buffer, state)
}

/// A caret at a line's start stays before its text, not before the inserted bullet.
#[test]
fn a_caret_at_an_insertion_lands_after_it() {
    let (buffer, state) = spliced("a\nb", 2, &hunk(2..2, "- [ ] "));
    assert_eq!(buffer.text(), "a\n- [ ] b");
    assert_eq!(state.selection().range(), at(8)..at(8));
}

#[test]
fn a_caret_before_the_splice_stays_and_one_after_shifts() {
    let insert = hunk(2..2, "[ ] ");
    assert_eq!(spliced("- a", 0, &insert).1.cursor(), at(0));
    assert_eq!(spliced("- a", 3, &insert).1.cursor(), at(7));
    let tick = hunk(3..4, "x");
    assert_eq!(spliced("- [ ] a", 6, &tick).1.cursor(), at(6));
    assert_eq!(spliced("- [ ] a", 2, &tick).1.cursor(), at(2));
}

/// A flip moves no text, so a caret on the middle byte stays before it.
#[test]
fn a_caret_on_a_replaced_byte_stays_before_it() {
    let (buffer, state) = spliced("- [ ] a", 3, &hunk(3..4, "x"));
    assert_eq!(buffer.text(), "- [x] a");
    assert_eq!(state.cursor(), at(3));
}

#[test]
fn a_splice_is_one_step_that_merges_with_no_typing() {
    let mut buffer = Buffer::new("a");
    let mut state = EditorState::new(Selection::at(buffer.len()));
    let start = Instant::now();

    state.insert(&mut buffer, start, "1");
    state.splice(&mut buffer, &hunk(0..0, "- [ ] "));
    state.insert(&mut buffer, start + inkra_text::GROUP_INTERVAL / 2, "2");
    assert_eq!(buffer.text(), "- [ ] a12");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [ ] a1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "a1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "a");
}

proptest! {
    /// The caret keeps the text after it past the splice, and the text before it short of the
    /// splice; inside a replacement it moves to its start.
    #[test]
    fn the_caret_keeps_its_text(
        text in "[ a-zé\n]{0,8}",
        ends in any::<[prop::sample::Index; 3]>(),
        inserted in "[ -\\[\\]x]{0,6}",
    ) {
        let bounds: Vec<usize> = (0..=text.len()).filter(|&at| text.is_char_boundary(at)).collect();
        let mut picked = ends.map(|end| bounds[end.index(bounds.len())]);
        picked[..2].sort_unstable();
        let [start, end, caret] = picked;
        let (buffer, state) = spliced(&text, caret, &hunk(start..end, &inserted));
        let after = buffer.text();
        let cursor = state.cursor().get();
        prop_assert!(after.is_char_boundary(cursor));
        if caret < start || (caret == start && start < end) {
            prop_assert_eq!(cursor, caret);
        } else if caret < end {
            prop_assert_eq!(cursor, start);
        } else {
            prop_assert_eq!(&after[cursor..], &text[caret..]);
            prop_assert!(cursor >= start + inserted.len());
        }
    }
}
