use std::time::{Duration, Instant};

use inkra_text::{Buffer, ByteOffset, Utf16Offset};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(usize, String),
    Delete(usize, usize),
    /// Two edits that must collapse into a single undo step.
    ReplaceInTransaction(usize, usize, String),
    /// Two inserts at *different* positions in one transaction — the case that
    /// exercises inverting a multi-edit transaction in reverse order.
    InsertTwiceInTransaction(usize, usize, String, String),
    Undo,
    Redo,
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
        // Newlines belong in the shared strategy, not a line-index-only one:
        // undo, grapheme stepping and UTF-16 conversion all have to survive
        // them too, and until multi-line they were never generated.
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

/// Maps an arbitrary index onto a char boundary of `text`, so generated offsets
/// are always valid without the strategy knowing the buffer contents.
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

/// Every invariant the line index owes its callers, checked against `text()`
/// rather than against a second implementation of the same scan.
fn check_line_index(buf: &Buffer) -> Result<(), TestCaseError> {
    let count = buf.line_count();
    prop_assert_eq!(buf.lines().count(), count);

    let mut expected_start = ByteOffset(0);
    for line in 0..count {
        let range = buf.line_range(line);

        prop_assert_eq!(range.start, expected_start, "line {} starts late", line);
        prop_assert!(range.start <= range.end, "line {} is inverted", line);
        prop_assert!(
            !buf.text()[range.start.0..range.end.0].contains('\n'),
            "line {} holds a newline",
            line
        );
        // Every offset on the line resolves back to it, both edges included.
        prop_assert_eq!(buf.line_for_offset(range.start), line);
        prop_assert_eq!(buf.line_for_offset(range.end), line);

        // The next line starts one byte past this one's newline.
        expected_start = ByteOffset(range.end.0 + 1);
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
                    buf.insert(ByteOffset(at), &text);
                    model.insert_str(at, &text);
                }
                Op::Delete(start, end) => {
                    let (start, end) = ordered_boundaries(&model, start, end);
                    if start == end {
                        continue;
                    }
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.delete(ByteOffset(start)..ByteOffset(end));
                    model.replace_range(start..end, "");
                }
                Op::ReplaceInTransaction(start, end, text) => {
                    let (start, end) = ordered_boundaries(&model, start, end);
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.transact(|buf| {
                        buf.delete(ByteOffset(start)..ByteOffset(end));
                        buf.insert(ByteOffset(start), &text);
                    });
                    model.replace_range(start..end, &text);
                }
                Op::InsertTwiceInTransaction(first_at, second_at, first, second) => {
                    let first_at = boundary(&model, first_at);
                    model_undo.push(model.clone());
                    model_redo.clear();

                    // The second position is chosen against the text as it
                    // stands *after* the first insert, matching what a caller
                    // composing two edits would actually compute.
                    let mut expected = model.clone();
                    expected.insert_str(first_at, &first);
                    let second_at = boundary(&expected, second_at);
                    expected.insert_str(second_at, &second);

                    buf.transact(|buf| {
                        buf.insert(ByteOffset(first_at), &first);
                        buf.insert(ByteOffset(second_at), &second);
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

        // Equal versions imply equal text. The converse does not hold: undoing
        // and retyping the same characters reaches the same text at a new
        // version. This holds for a per-transaction id too — nothing here
        // calls `transact_at`, so nothing merges; the merge case is
        // `version_moves_whenever_a_timed_edit_changes_the_text`.
        for (i, (version_i, text_i)) in observations.iter().enumerate() {
            for (version_j, text_j) in observations.iter().skip(i + 1) {
                if version_i == version_j {
                    prop_assert_eq!(text_i, text_j);
                }
            }
        }
    }

    /// The version must move on every edit that moves the text, including one
    /// that merges into the previous transaction. Delays straddle
    /// `GROUP_INTERVAL` (300 ms), so both the merging and the non-merging
    /// branch of `close` are exercised. A per-transaction id passes every
    /// other property in this file and fails here.
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

            // Every generated string is non-empty, so every insert moves the
            // text and owes a new version.
            buf.transact_at(now, |buf| buf.insert(ByteOffset(at), &text));

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
            let offset = ByteOffset(byte);
            prop_assert_eq!(buf.from_utf16(buf.to_utf16(offset)), offset);
        }
        prop_assert_eq!(
            buf.to_utf16(buf.len()),
            Utf16Offset(buf.text().encode_utf16().count())
        );
    }

    /// The line index is derived from the text on every call, so the thing to
    /// prove is that its four views agree with each other and with `text()` —
    /// including after edits, which is where a cached index would drift.
    #[test]
    fn the_line_index_stays_consistent_across_edits(
        parts in prop::collection::vec(text_strategy(), 0..8),
        edits in prop::collection::vec((any::<usize>(), text_strategy()), 0..8),
    ) {
        let mut buf = Buffer::new(parts.concat());
        check_line_index(&buf)?;

        for (at, text) in edits {
            let at = boundary(buf.text(), at);
            buf.insert(ByteOffset(at), &text);
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

        let mut offset = ByteOffset(0);
        let mut steps = 0;
        while offset < buf.len() {
            let next = buf.next_grapheme(offset);
            prop_assert!(next > offset, "next_grapheme must advance");
            prop_assert!(buf.text().is_char_boundary(next.0));
            prop_assert_eq!(buf.previous_grapheme(next), offset);
            offset = next;
            steps += 1;
            prop_assert!(steps <= buf.len().0, "grapheme walk failed to terminate");
        }
        prop_assert_eq!(offset, buf.len());
    }
}
