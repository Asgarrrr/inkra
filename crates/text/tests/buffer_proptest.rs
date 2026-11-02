use std::time::{Duration, Instant};

use inkra_text::{Buffer, ByteOffset, Utf16Offset};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(usize, String),
    Delete(usize, usize),
    ReplaceInTransaction(usize, usize, String),
    InsertTwiceInTransaction(usize, usize, String, String),
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
enum Utf16Op {
    Edit(usize, usize, &'static str),
    Undo,
    Redo,
    To(usize),
    From(usize),
}

fn text_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z]{1,3}",
        Just("é".to_string()),
        Just("ñ".to_string()),
        Just("日".to_string()),
        Just("🎉".to_string()),
        Just("café".to_string()),
        Just("e\u{301}".to_string()),
        Just("👨\u{200d}👩\u{200d}👧".to_string()),
        // Newlines live here, not a line-index-only strategy: undo and UTF-16 conversion must survive them too.
        Just("\n".to_string()),
        Just("\n\n".to_string()),
        Just("a\nb".to_string()),
    ]
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (any::<usize>(), text_strategy()).prop_map(|(at, text)| Op::Insert(at, text)),
        3 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Delete(a, b)),
        2 => (any::<usize>(), any::<usize>(), text_strategy())
            .prop_map(|(a, b, text)| Op::ReplaceInTransaction(a, b, text)),
        2 => (any::<usize>(), any::<usize>(), text_strategy(), text_strategy())
            .prop_map(|(a, b, first, second)| Op::InsertTwiceInTransaction(a, b, first, second)),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
    ]
}

fn utf16_text() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec!["a", "é", "😀", "\n", "ab"])
}

fn utf16_op_strategy() -> impl Strategy<Value = Utf16Op> {
    prop_oneof![
        3 => (any::<usize>(), 0usize..4, utf16_text())
            .prop_map(|(at, len, text)| Utf16Op::Edit(at, len, text)),
        1 => Just(Utf16Op::Undo),
        1 => Just(Utf16Op::Redo),
        3 => any::<usize>().prop_map(Utf16Op::To),
        3 => any::<usize>().prop_map(Utf16Op::From),
    ]
}

/// The linear walk from byte 0 that `from_utf16` replaced.
fn from_utf16_from_start(text: &str, target: usize) -> usize {
    let mut utf16 = 0;
    for (byte, ch) in text.char_indices() {
        if utf16 >= target {
            return byte;
        }
        utf16 += ch.len_utf16();
    }
    text.len()
}

fn boundary(text: &str, index: usize) -> usize {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();
    boundaries[index % boundaries.len()]
}

fn ordered_boundaries(text: &str, a: usize, b: usize) -> (usize, usize) {
    let (a, b) = (boundary(text, a), boundary(text, b));
    (a.min(b), a.max(b))
}

fn check_line_index(buf: &Buffer) -> Result<(), TestCaseError> {
    let count = buf.line_count();
    prop_assert_eq!(buf.lines().count(), count);

    let mut expected_start = ByteOffset::new(0);
    for line in 0..count {
        let range = buf.line_range(line);

        prop_assert_eq!(range.start, expected_start, "line {} starts late", line);
        prop_assert!(range.start <= range.end, "line {} is inverted", line);
        prop_assert!(
            !buf.slice(range.start..range.end).contains('\n'),
            "line {} holds a newline",
            line
        );
        prop_assert_eq!(buf.line_for_offset(range.start), line);
        prop_assert_eq!(buf.line_for_offset(range.end), line);

        expected_start = range.end + 1;
    }
    prop_assert_eq!(buf.line_range(count - 1).end, buf.len());

    let (offsets, texts): (Vec<_>, Vec<_>) = buf.lines().unzip();
    prop_assert_eq!(texts.join("\n"), buf.text());
    for (line, start) in offsets.into_iter().enumerate() {
        prop_assert_eq!(start, buf.line_range(line).start);
    }
    Ok(())
}

proptest! {
    #[test]
    fn buffer_matches_reference_model(ops in prop::collection::vec(op_strategy(), 0..30)) {
        let mut buf = Buffer::new("");
        let mut model = String::new();
        let mut model_undo: Vec<String> = Vec::new();
        let mut model_redo: Vec<String> = Vec::new();
        let mut observations: Vec<(u64, String)> = Vec::new();

        for op in ops {
            match op {
                Op::Insert(at, text) => {
                    let at = boundary(&model, at);
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.insert(ByteOffset::new(at), &text);
                    model.insert_str(at, &text);
                }
                Op::Delete(start, end) => {
                    let (start, end) = ordered_boundaries(&model, start, end);
                    if start == end {
                        continue;
                    }
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.delete(ByteOffset::new(start)..ByteOffset::new(end));
                    model.replace_range(start..end, "");
                }
                Op::ReplaceInTransaction(start, end, text) => {
                    let (start, end) = ordered_boundaries(&model, start, end);
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.transact(|buf| {
                        buf.delete(ByteOffset::new(start)..ByteOffset::new(end));
                        buf.insert(ByteOffset::new(start), &text);
                    });
                    model.replace_range(start..end, &text);
                }
                Op::InsertTwiceInTransaction(first_at, second_at, first, second) => {
                    let first_at = boundary(&model, first_at);
                    model_undo.push(model.clone());
                    model_redo.clear();

                    let mut expected = model.clone();
                    expected.insert_str(first_at, &first);
                    let second_at = boundary(&expected, second_at);
                    expected.insert_str(second_at, &second);

                    buf.transact(|buf| {
                        buf.insert(ByteOffset::new(first_at), &first);
                        buf.insert(ByteOffset::new(second_at), &second);
                    });
                    model = expected;
                }
                Op::Undo => {
                    let undone = buf.undo().is_some();
                    match model_undo.pop() {
                        Some(previous) => {
                            prop_assert!(undone, "model had undo history but Buffer::undo() returned None");
                            model_redo.push(std::mem::replace(&mut model, previous));
                        }
                        None => {
                            prop_assert!(!undone, "model had no undo history but Buffer::undo() returned Some");
                        }
                    }
                }
                Op::Redo => {
                    let redone = buf.redo().is_some();
                    match model_redo.pop() {
                        Some(next) => {
                            prop_assert!(redone, "model had redo history but Buffer::redo() returned None");
                            model_undo.push(std::mem::replace(&mut model, next));
                        }
                        None => {
                            prop_assert!(!redone, "model had no redo history but Buffer::redo() returned Some");
                        }
                    }
                }
            }

            prop_assert_eq!(buf.text(), model.as_str());
            observations.push((buf.version(), buf.text().to_owned()));
        }

        // Equal versions imply equal text; the converse doesn't hold — undo then retype reaches
        // the same text at a new version. The merge case is covered separately, below.
        for (i, (version_i, text_i)) in observations.iter().enumerate() {
            for (version_j, text_j) in observations.iter().skip(i + 1) {
                if version_i == version_j {
                    prop_assert_eq!(text_i, text_j);
                }
            }
        }
    }

    /// Delays straddle `GROUP_INTERVAL` so both the merging and non-merging branch of `close` run.
    #[test]
    fn version_moves_whenever_a_timed_edit_changes_the_text(
        edits in prop::collection::vec((any::<usize>(), text_strategy(), 0u64..600), 1..20)
    ) {
        let mut buf = Buffer::new("");
        let mut now = Instant::now();

        for (at, text, delay) in edits {
            let at = boundary(buf.text(), at);
            let before = buf.version();
            now += Duration::from_millis(delay);

            // Every generated string is non-empty, so every insert moves the text and owes a new version.
            buf.transact_at(now, |buf| buf.insert(ByteOffset::new(at), &text));

            prop_assert!(
                buf.version() > before,
                "version {} did not advance past {}",
                buf.version(),
                before
            );
        }
    }

    #[test]
    fn utf16_offsets_round_trip_on_every_char_boundary(
        parts in prop::collection::vec(text_strategy(), 0..10)
    ) {
        let buf = Buffer::new(parts.concat());
        for (byte, _) in buf.text().char_indices().chain(std::iter::once((buf.text().len(), ' '))) {
            let offset = ByteOffset::new(byte);
            prop_assert_eq!(buf.from_utf16(buf.to_utf16(offset)), offset);
        }
        prop_assert_eq!(
            buf.to_utf16(buf.len()),
            Utf16Offset::new(buf.text().encode_utf16().count())
        );
    }

    #[test]
    fn utf16_conversions_match_a_fresh_count_through_edits_and_undo(
        parts in prop::collection::vec(utf16_text(), 0..8),
        ops in prop::collection::vec(utf16_op_strategy(), 0..24),
    ) {
        let mut buf = Buffer::new(parts.concat());
        for op in ops {
            match op {
                Utf16Op::Edit(at, len, text) => {
                    let count = buf.text().char_indices().count() + 1;
                    let start = at % count;
                    let (start, end) = ordered_boundaries(buf.text(), start, (start + len).min(count - 1));
                    buf.transact(|buf| buf.replace(ByteOffset::new(start)..ByteOffset::new(end), text));
                }
                Utf16Op::Undo => {
                    buf.undo();
                }
                Utf16Op::Redo => {
                    buf.redo();
                }
                Utf16Op::To(n) => {
                    let byte = boundary(buf.text(), n);
                    let expected = buf.text()[..byte].encode_utf16().count();
                    prop_assert_eq!(buf.to_utf16(ByteOffset::new(byte)), Utf16Offset::new(expected));
                }
                Utf16Op::From(n) => {
                    let target = n % (buf.text().encode_utf16().count() + 3);
                    let expected = from_utf16_from_start(buf.text(), target);
                    prop_assert_eq!(buf.from_utf16(Utf16Offset::new(target)), ByteOffset::new(expected));
                }
            }
        }
    }

    #[test]
    fn the_line_index_stays_consistent_across_edits(
        parts in prop::collection::vec(text_strategy(), 0..8),
        edits in prop::collection::vec((any::<usize>(), text_strategy()), 0..8),
    ) {
        let mut buf = Buffer::new(parts.concat());
        check_line_index(&buf)?;

        for (at, text) in edits {
            let at = boundary(buf.text(), at);
            buf.insert(ByteOffset::new(at), &text);
            check_line_index(&buf)?;
        }

        while buf.undo().is_some() {
            check_line_index(&buf)?;
        }
    }

    #[test]
    fn grapheme_steps_walk_the_whole_buffer_and_stay_on_boundaries(
        parts in prop::collection::vec(text_strategy(), 0..10)
    ) {
        let buf = Buffer::new(parts.concat());

        let mut offset = ByteOffset::new(0);
        let mut steps = 0;
        while offset < buf.len() {
            let next = buf.next_grapheme(offset);
            prop_assert!(next > offset, "next_grapheme must advance");
            prop_assert!(buf.text().is_char_boundary(next.get()));
            prop_assert_eq!(buf.previous_grapheme(next), offset);
            offset = next;
            steps += 1;
            prop_assert!(steps <= buf.len().get(), "grapheme walk failed to terminate");
        }
        prop_assert_eq!(offset, buf.len());
    }
}
