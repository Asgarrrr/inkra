//! `apply_diff`: an edit the editor makes as a diff, as a task's flip (task boxes spec, D5, D6).

use inkra_text::{GROUP_INTERVAL, Hunk};

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

/// One step of its own: the typing on either side, within one group interval, stays apart.
#[test]
fn a_diff_is_one_step_that_merges_with_no_typing() {
    let mut buffer = Buffer::new("- [ ] ");
    let mut state = EditorState::new(Selection::at(buffer.len()));
    let start = Instant::now();

    state.insert(&mut buffer, start, "1");
    let flip = Diff::new(buffer.version(), vec![hunk(3..4, "x")]);
    assert!(state.apply_diff(&mut buffer, &flip));
    state.insert(&mut buffer, start + GROUP_INTERVAL / 2, "2");
    assert_eq!(buffer.text(), "- [x] 12");

    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [x] 1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [ ] 1");
    assert!(state.undo(&mut buffer));
    assert_eq!(buffer.text(), "- [ ] ");
}

/// The caret keeps its text, after an insertion at it and after a byte replaced under it.
#[test]
fn a_diff_maps_the_caret() {
    for (text, caret, hunk, edited, mapped) in [
        ("a", 0, hunk(0..0, "- [ ] "), "- [ ] a", 6),
        ("- [ ] a", 3, hunk(3..4, "x"), "- [x] a", 4),
        ("- a", 3, hunk(2..2, "[ ] "), "- [ ] a", 7),
    ] {
        let mut buffer = Buffer::new(text);
        let mut state = EditorState::new(Selection::at(at(caret)));
        let diff = Diff::new(buffer.version(), vec![hunk]);
        assert!(state.apply_diff(&mut buffer, &diff));
        assert_eq!(buffer.text(), edited);
        assert_eq!(
            state.selection().range(),
            at(mapped)..at(mapped),
            "{text:?}"
        );
    }
}

/// A diff from an older version would land on other text.
#[test]
fn a_diff_from_an_older_version_edits_nothing_and_keeps_the_caret() {
    let mut buffer = Buffer::new("- [ ] a");
    let mut state = EditorState::new(Selection::at(at(0)));
    let old = Diff::new(buffer.version(), vec![hunk(0..0, "z")]);
    state.insert(&mut buffer, Instant::now(), "!");

    assert!(!state.apply_diff(&mut buffer, &old));
    assert_eq!(buffer.text(), "!- [ ] a");
    assert_eq!(state.cursor(), at(1));
}
