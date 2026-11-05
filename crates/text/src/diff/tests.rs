use std::time::{Duration, Instant};

use super::*;
use crate::anchor::Bias;

fn hunk(range: Range<usize>, text: &str) -> Hunk {
    Hunk {
        range: ByteOffset::new(range.start)..ByteOffset::new(range.end),
        text: text.to_string(),
    }
}

fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    diff(Version::default(), old, new).hunks
}

/// Applies the diff from the buffer's own text and version to `new`.
fn reload(buffer: &mut Buffer, new: &str) {
    let diff = diff(buffer.version(), buffer.text(), new);
    assert!(buffer.apply_diff(&diff));
}

fn reloaded(old: &str, new: &str) -> Buffer {
    let mut buffer = Buffer::new(old);
    reload(&mut buffer, new);
    buffer
}

/// A line of exactly `len` bytes whose last word is `word`.
fn line_of(len: usize, word: &str) -> String {
    format!("{} {word}\n", "x".repeat(len - word.len() - 2))
}

fn lines_of(word: &str, count: usize) -> String {
    format!("{word}\n").repeat(count)
}

/// Every line differs, so only the word pass can split the change into more than one hunk.
fn refined(old: &str, new: &str) -> bool {
    hunks(old, new).len() > 1
}

#[test]
fn identical_texts_give_no_hunk() {
    assert_eq!(hunks("", ""), []);
    assert_eq!(hunks("one\n\ntwo\n", "one\n\ntwo\n"), []);
}

#[test]
fn an_appended_line_is_one_insertion_at_the_end() {
    assert_eq!(hunks("one\n", "one\ntwo\n"), [hunk(4..4, "two\n")]);
}

#[test]
fn a_prepended_line_is_one_insertion_at_the_start() {
    assert_eq!(hunks("two\n", "one\ntwo\n"), [hunk(0..0, "one\n")]);
}

#[test]
fn a_changed_word_is_a_word_sized_hunk_not_the_line() {
    assert_eq!(
        hunks(
            "keep\nthe quick brown fox\nkeep\n",
            "keep\nthe quick red fox\nkeep\n"
        ),
        [hunk(15..20, "red")]
    );
}

#[test]
fn a_hunk_in_a_moved_text_takes_its_replacement_from_the_new_side() {
    assert_eq!(
        hunks("a\nthe red fox\n", "new\nlines\na\nthe tan fox\n"),
        [hunk(0..0, "new\nlines\n"), hunk(6..9, "tan")]
    );
}

#[test]
fn the_diff_keeps_a_carriage_return_as_text() {
    assert_eq!(hunks("one\r\ntwo\n", "one\ntwo\n"), [hunk(3..5, "\n")]);
}

#[test]
fn multibyte_words_are_replaced_on_char_boundaries() {
    assert_eq!(
        hunks("un café noir\n", "un thé noir\n"),
        [hunk(3..8, "thé")]
    );
    assert_eq!(hunks("go 🎉 now\n", "go 🚀 now\n"), [hunk(3..7, "🚀")]);
    assert_eq!(
        hunks("東京 と 大阪\n", "東京 と 京都\n"),
        [hunk(11..17, "京都")]
    );
}

#[test]
fn a_changed_base_under_a_combining_mark_replaces_the_whole_grapheme() {
    assert_eq!(hunks("e\u{301}b", "a\u{301}b"), [hunk(0..4, "a\u{301}b")]);
}

#[test]
fn a_changed_member_of_a_zwj_sequence_replaces_the_whole_sequence() {
    let family = |child| format!("go 👨\u{200d}👩\u{200d}{child} now\n");
    assert_eq!(
        hunks(&family("👧"), &family("👦")),
        [hunk(3..21, "👨\u{200d}👩\u{200d}👦")]
    );
}

#[test]
fn punctuation_splits_on_each_new_char_but_not_a_run() {
    assert_eq!(hunks("a?!b\n", "a?b\n"), [hunk(2..3, "")]);
    assert_eq!(hunks("a!!b\n", "a!b\n"), [hunk(1..3, "!")]);
    assert_eq!(hunks("a.b\n", "a,b\n"), [hunk(1..2, ",")]);
}

#[test]
fn mixed_whitespace_is_one_run() {
    assert_eq!(hunks("a \tb\n", "a\tb\n"), [hunk(1..3, "\t")]);
}

#[test]
fn word_refinement_stops_past_eight_lines_on_either_side() {
    assert!(refined(&lines_of("a", 8), &lines_of("b", 8)));
    assert!(!refined(&lines_of("a", 9), &lines_of("b", 8)));
    assert!(!refined(&lines_of("a", 8), &lines_of("b", 9)));
}

#[test]
fn word_refinement_stops_past_512_bytes_on_either_side() {
    assert!(refined(&line_of(512, "a"), "y b\n"));
    assert!(!refined(&line_of(513, "a"), "y b\n"));
    assert!(refined("y b\n", &line_of(512, "a")));
    assert!(!refined("y b\n", &line_of(513, "a")));
}

#[test]
fn applying_the_hunks_yields_the_new_text() {
    let new = "new first\nthe tan fox\n\nlast\n";
    assert_eq!(reloaded("the red fox\n\n\nlast line\n", new).text(), new);
}

#[test]
fn one_undo_restores_the_text_before_the_reload() {
    let old = "the red fox\n\n\nlast line\n";
    let mut buffer = reloaded(old, "new first\nthe tan fox\n\nlast\n");
    buffer.undo();
    assert_eq!(buffer.text(), old);
}

#[test]
fn a_diff_from_an_older_version_edits_nothing() {
    let mut buffer = Buffer::new("a\n");
    let stale = diff(buffer.version(), buffer.text(), "a\nc\n");
    buffer.transact(|buffer| buffer.insert(ByteOffset::new(1), "b"));
    assert!(!buffer.apply_diff(&stale));
    assert_eq!(buffer.text(), "ab\n");
}

#[test]
fn the_reload_does_not_merge_into_typing_before_it() {
    let now = Instant::now();
    let mut buffer = Buffer::new("a\n");
    buffer.transact_at(now, |buffer| buffer.insert(ByteOffset::new(1), "b"));
    reload(&mut buffer, "ab\nc\n");
    buffer.undo();
    assert_eq!(buffer.text(), "ab\n");
}

#[test]
fn typing_after_the_reload_does_not_merge_into_it() {
    let now = Instant::now();
    let mut buffer = Buffer::new("a\n");
    reload(&mut buffer, "a\nc\n");
    buffer.transact_at(now + Duration::from_millis(1), |buffer| {
        buffer.insert(ByteOffset::new(1), "b");
    });
    buffer.undo();
    assert_eq!(buffer.text(), "a\nc\n");
}

#[test]
fn no_hunk_leaves_a_typing_run_open() {
    let now = Instant::now();
    let mut buffer = Buffer::new("");
    buffer.transact_at(now, |buffer| buffer.insert(ByteOffset::new(0), "a"));
    reload(&mut buffer, "a");
    buffer.transact_at(now, |buffer| buffer.insert(ByteOffset::new(1), "b"));
    buffer.undo();
    assert_eq!(buffer.text(), "");
}

/// `a bc d` with `bc` replaced by `XYZ`: where an anchor at `offset` with `bias` resolves after.
fn anchor_across_one_hunk(offset: usize, bias: Bias) -> usize {
    let (old, new) = ("a bc d", "a XYZ d");
    assert_eq!(hunks(old, new), [hunk(2..4, "XYZ")]);
    let mut buffer = Buffer::new(old);
    let anchor = buffer.anchor(ByteOffset::new(offset), bias);
    reload(&mut buffer, new);
    buffer.resolve(anchor).get()
}

#[test]
fn an_anchor_at_a_hunk_start_goes_before_or_after_the_replacement_by_bias() {
    assert_eq!(anchor_across_one_hunk(2, Bias::Left), 2);
    assert_eq!(anchor_across_one_hunk(2, Bias::Right), 5);
}

#[test]
fn an_anchor_at_a_hunk_end_stays_on_the_text_after_it_with_either_bias() {
    assert_eq!(anchor_across_one_hunk(4, Bias::Left), 5);
    assert_eq!(anchor_across_one_hunk(4, Bias::Right), 5);
}

#[test]
fn anchors_outside_the_hunks_stay_on_their_text() {
    let old = "keep one\nthe red fox\nkeep two\n";
    let mut buffer = Buffer::new(old);
    let at = |word| {
        let Some(offset) = old.find(word) else {
            panic!("no {word}");
        };
        ByteOffset::new(offset)
    };
    let one = buffer.anchor(at("one"), Bias::Left);
    let two = buffer.anchor(at("two"), Bias::Right);
    reload(&mut buffer, "new\nkeep one\nthe crimson fox\nkeep two\n");
    assert_eq!(&buffer.text()[buffer.resolve(one).get()..][..3], "one");
    assert_eq!(&buffer.text()[buffer.resolve(two).get()..][..3], "two");
}

/// Two hunks sharing an edge, which `diff` never emits: `ab|cd` becomes `X|YZW`.
fn touching() -> Diff {
    Diff {
        base_version: Version::default(),
        hunks: vec![hunk(0..2, "X"), hunk(2..4, "YZW")],
    }
}

#[test]
fn a_mapped_offset_at_touching_hunks_shared_edge_goes_by_bias() {
    assert_eq!(
        touching().map(ByteOffset::new(2), Bias::Left),
        ByteOffset::new(1)
    );
    assert_eq!(
        touching().map(ByteOffset::new(2), Bias::Right),
        ByteOffset::new(4)
    );
}

#[test]
fn a_mapped_offset_inside_either_touching_hunk_lands_on_its_edges() {
    assert_eq!(
        touching().map(ByteOffset::new(1), Bias::Left),
        ByteOffset::new(0)
    );
    assert_eq!(
        touching().map(ByteOffset::new(1), Bias::Right),
        ByteOffset::new(1)
    );
    assert_eq!(
        touching().map(ByteOffset::new(3), Bias::Left),
        ByteOffset::new(1)
    );
    assert_eq!(
        touching().map(ByteOffset::new(3), Bias::Right),
        ByteOffset::new(4)
    );
}

#[test]
fn a_mapped_offset_equals_an_anchor_across_touching_hunks() {
    for offset in 0..=4 {
        for bias in [Bias::Left, Bias::Right] {
            let mut buffer = Buffer::new("abcd");
            let anchor = buffer.anchor(ByteOffset::new(offset), bias);
            assert!(buffer.apply_diff(&touching()));
            assert_eq!(buffer.text(), "XYZW");
            assert_eq!(
                touching().map(ByteOffset::new(offset), bias),
                buffer.resolve(anchor),
                "{offset} {bias:?}"
            );
        }
    }
}
