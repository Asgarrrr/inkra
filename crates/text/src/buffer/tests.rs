#![cfg_attr(test, allow(clippy::unwrap_used))]

use super::*;

fn at(offset: usize) -> ByteOffset {
    ByteOffset(offset)
}

#[test]
fn new_buffer_holds_initial_text() {
    assert_eq!(Buffer::new("hello").text(), "hello");
}

#[test]
fn is_empty_tracks_whether_the_text_is_empty() {
    assert!(Buffer::new("").is_empty());
    assert!(!Buffer::new("x").is_empty());
}

#[test]
fn insert_places_text_at_byte_offset() {
    let mut buf = Buffer::new("héllo");
    buf.insert(at(3), "X");
    assert_eq!(buf.text(), "héXllo");
}

#[test]
fn delete_removes_a_byte_range() {
    let mut buf = Buffer::new("héllo");
    buf.delete(at(1)..at(4));
    assert_eq!(buf.text(), "hlo");
}

#[test]
#[should_panic(expected = "char boundaries")]
fn replace_panics_when_a_bound_splits_a_char() {
    let mut buf = Buffer::new("héllo");
    buf.insert(at(2), "X");
}

#[test]
fn no_op_replace_records_nothing() {
    let mut buf = Buffer::new("abc");
    buf.replace(at(1)..at(1), "");
    assert!(buf.undo().is_none());
}

#[test]
fn undo_reverts_the_last_edit_and_reports_the_restored_range() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "d");
    assert_eq!(buf.text(), "abcd");
    assert_eq!(buf.undo(), Some(at(3)..at(3)));
    assert_eq!(buf.text(), "abc");
}

#[test]
fn undo_with_empty_history_returns_none() {
    assert!(Buffer::new("abc").undo().is_none());
}

#[test]
fn redo_reapplies_the_undone_edit() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "d");
    buf.undo();
    assert_eq!(buf.redo(), Some(at(3)..at(4)));
    assert_eq!(buf.text(), "abcd");
}

#[test]
fn new_edit_after_undo_clears_the_redo_stack() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "d");
    buf.undo();
    buf.insert(at(0), "X");
    assert!(buf.redo().is_none());
}

#[test]
fn a_transaction_undoes_as_one_step() {
    let mut buf = Buffer::new("abc");
    buf.transact(|buf| {
        buf.delete(at(0)..at(1));
        buf.insert(at(0), "XY");
    });
    assert_eq!(buf.text(), "XYbc");
    buf.undo();
    assert_eq!(buf.text(), "abc");
    assert!(buf.undo().is_none());
}

#[test]
fn nested_transactions_join_the_outermost_step() {
    let mut buf = Buffer::new("");
    buf.transact(|buf| {
        buf.insert(at(0), "a");
        buf.transact(|buf| buf.insert(at(1), "b"));
    });
    assert_eq!(buf.text(), "ab");
    buf.undo();
    assert_eq!(buf.text(), "");
    assert!(buf.undo().is_none());
}

#[test]
fn an_empty_transaction_records_nothing() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "d");
    buf.transact(|_| {});
    buf.undo();
    assert_eq!(buf.text(), "abc");
}

#[test]
fn a_transaction_does_not_clear_redo_until_it_records_an_edit() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "d");
    buf.undo();
    buf.transact(|_| {});
    assert_eq!(buf.redo(), Some(at(3)..at(4)));
}

#[test]
#[should_panic(expected = "undo() inside an open transaction")]
fn undo_inside_an_open_transaction_is_refused() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "1");
    buf.transact(|buf| {
        buf.insert(at(0), "X");
        // Without the guard this pops the older transaction and applies its now-stale offsets.
        buf.undo();
    });
}

#[test]
#[should_panic(expected = "redo() inside an open transaction")]
fn redo_inside_an_open_transaction_is_refused() {
    let mut buf = Buffer::new("abc");
    buf.insert(at(3), "1");
    buf.undo();
    buf.transact(|buf| {
        buf.insert(at(0), "X");
        buf.redo();
    });
}

#[test]
fn a_multi_edit_transaction_reports_a_region_valid_in_the_resulting_text() {
    let mut buf = Buffer::new("abcd");
    buf.transact(|buf| {
        buf.insert(at(0), "<");
        buf.insert(at(5), ">");
    });
    assert_eq!(buf.text(), "<abcd>");

    let undone = buf.undo().unwrap();
    assert_eq!(buf.text(), "abcd");
    assert!(
        undone.end <= buf.len(),
        "{undone:?} escapes {:?}",
        buf.text()
    );

    let redone = buf.redo().unwrap();
    assert_eq!(buf.text(), "<abcd>");
    assert!(
        redone.end <= buf.len(),
        "{redone:?} escapes {:?}",
        buf.text()
    );
}

#[test]
fn edits_within_the_group_interval_merge_into_one_undo_step() {
    let mut buf = Buffer::new("");
    let start = Instant::now();
    for (index, ch) in "hello".chars().enumerate() {
        let now = start + GROUP_INTERVAL / 2 * u32::try_from(index).unwrap();
        let at = buf.len();
        buf.transact_at(now, |buf| buf.insert(at, &ch.to_string()));
    }
    assert_eq!(buf.text(), "hello");

    buf.undo();
    assert_eq!(buf.text(), "");
    assert!(buf.undo().is_none());
}

#[test]
fn edits_further_apart_than_the_group_interval_stay_separate() {
    let mut buf = Buffer::new("");
    let start = Instant::now();
    buf.transact_at(start, |buf| buf.insert(at(0), "a"));
    buf.transact_at(start + GROUP_INTERVAL * 2, |buf| buf.insert(at(1), "b"));

    buf.undo();
    assert_eq!(buf.text(), "a");
    buf.undo();
    assert_eq!(buf.text(), "");
}

#[test]
fn an_edit_exactly_one_group_interval_later_stays_separate() {
    let mut buf = Buffer::new("");
    let start = Instant::now();
    buf.transact_at(start, |buf| buf.insert(at(0), "a"));
    buf.transact_at(start + GROUP_INTERVAL, |buf| buf.insert(at(1), "b"));

    buf.undo();
    assert_eq!(buf.text(), "a");
}

#[test]
fn a_merged_run_redoes_as_one_step() {
    let mut buf = Buffer::new("");
    let start = Instant::now();
    buf.transact_at(start, |buf| buf.insert(at(0), "a"));
    buf.transact_at(start, |buf| buf.insert(at(1), "b"));

    buf.undo();
    assert_eq!(buf.text(), "");
    buf.redo();
    assert_eq!(buf.text(), "ab");
    assert!(buf.redo().is_none());
}

#[test]
fn finalize_transaction_stops_the_next_edit_from_merging() {
    let mut buf = Buffer::new("");
    let start = Instant::now();
    buf.transact_at(start, |buf| buf.insert(at(0), "a"));
    buf.finalize_transaction();
    buf.transact_at(start, |buf| buf.insert(at(1), "b"));

    buf.undo();
    assert_eq!(buf.text(), "a");
}

#[test]
fn an_ungrouped_transaction_never_absorbs_a_later_one() {
    let mut buf = Buffer::new("");
    buf.transact(|buf| buf.insert(at(0), "a"));
    buf.transact_at(Instant::now(), |buf| buf.insert(at(1), "b"));

    buf.undo();
    assert_eq!(buf.text(), "a");
}

#[test]
fn undo_keeps_only_the_edited_text_not_a_buffer_snapshot() {
    let mut buf = Buffer::new("x".repeat(10_000));
    buf.insert(at(0), "a");
    let recorded: usize = buf
        .undo_stack
        .iter()
        .flat_map(|transaction| &transaction.edits)
        .map(|edit| edit.removed.len() + edit.inserted.len())
        .sum();
    assert_eq!(recorded, 1);
}

#[test]
fn version_starts_at_zero_and_moves_with_an_edit() {
    let mut buffer = Buffer::new("abc");
    assert_eq!(buffer.version(), 0);
    buffer.insert(ByteOffset(3), "x");
    assert_ne!(buffer.version(), 0);
}

/// Regression pin: `close` merges by extending the previous transaction, so its id must live
/// on the edit, not the transaction — else a merged edit would report "saved" over unsaved text.
#[test]
fn version_moves_when_an_edit_merges_into_the_previous_transaction() {
    let mut buffer = Buffer::new("abc");
    let t0 = Instant::now();
    buffer.transact_at(t0, |b| b.insert(ByteOffset(3), "x"));
    let saved = buffer.version();

    buffer.transact_at(t0 + Duration::from_millis(120), |b| {
        b.insert(ByteOffset(4), "y");
    });

    assert_eq!(buffer.text(), "abcxy");
    assert_eq!(buffer.undo_stack.len(), 1, "the edits must have merged");
    assert_ne!(
        buffer.version(),
        saved,
        "a merged edit still changes the text, so the version must move"
    );
}

#[test]
fn undo_returns_the_version_to_its_earlier_value() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let after_first = buffer.version();
    buffer.finalize_transaction();
    buffer.transact(|b| b.insert(ByteOffset(4), "y"));

    buffer.undo();

    assert_eq!(buffer.text(), "abcx");
    assert_eq!(buffer.version(), after_first);
}

#[test]
fn redo_restores_the_same_version() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let after_edit = buffer.version();
    buffer.undo();
    buffer.redo();
    assert_eq!(buffer.version(), after_edit);
}

#[test]
fn a_divergent_edit_after_undo_cannot_collide_with_the_history_it_replaced() {
    let mut buffer = Buffer::new("abc");
    buffer.transact(|b| b.insert(ByteOffset(3), "x"));
    let replaced = buffer.version();
    buffer.undo();
    buffer.transact(|b| b.insert(ByteOffset(3), "z"));

    assert_eq!(buffer.text(), "abcz");
    assert_ne!(buffer.version(), replaced);
}

#[test]
fn utf16_offsets_round_trip_across_a_surrogate_pair() {
    let buf = Buffer::new("a😀b");
    assert_eq!(buf.to_utf16(at(5)), Utf16Offset(3));
    assert_eq!(buf.from_utf16(Utf16Offset(3)), at(5));
    assert_eq!(buf.to_utf16(buf.len()), Utf16Offset(4));
}

#[test]
fn a_utf16_offset_inside_a_surrogate_pair_snaps_forward() {
    let buf = Buffer::new("a😀b");
    assert_eq!(buf.from_utf16(Utf16Offset(2)), at(5));
}

#[test]
fn utf16_offset_past_the_end_clamps_to_the_end() {
    let buf = Buffer::new("ab");
    assert_eq!(buf.from_utf16(Utf16Offset(99)), at(2));
}

#[test]
fn grapheme_steps_cross_a_combining_mark_as_one_unit() {
    let buf = Buffer::new("e\u{301}x");
    assert_eq!(buf.next_grapheme(at(0)), at(3));
    assert_eq!(buf.previous_grapheme(at(3)), at(0));
}

#[test]
fn grapheme_steps_cross_a_zwj_emoji_as_one_unit() {
    let buf = Buffer::new("👨‍👩‍👧");
    assert_eq!(buf.next_grapheme(at(0)), buf.len());
    assert_eq!(buf.previous_grapheme(buf.len()), at(0));
}

#[test]
fn grapheme_steps_saturate_at_the_ends() {
    let buf = Buffer::new("ab");
    assert_eq!(buf.previous_grapheme(at(0)), at(0));
    assert_eq!(buf.next_grapheme(at(2)), at(2));
}

#[test]
fn an_empty_buffer_is_one_empty_line() {
    let buf = Buffer::new("");
    assert_eq!(buf.line_count(), 1);
    assert_eq!(buf.line_range(0), at(0)..at(0));
    assert_eq!(buf.lines().collect::<Vec<_>>(), vec![(at(0), "")]);
}

#[test]
fn a_trailing_newline_opens_an_empty_last_line() {
    let buf = Buffer::new("a\n");
    assert_eq!(buf.line_count(), 2);
    assert_eq!(buf.line_range(1), at(2)..at(2));
}

#[test]
fn consecutive_newlines_each_open_a_line() {
    let buf = Buffer::new("\n\n");
    assert_eq!(buf.line_count(), 3);
    assert_eq!(buf.line_range(0), at(0)..at(0));
    assert_eq!(buf.line_range(1), at(1)..at(1));
    assert_eq!(buf.line_range(2), at(2)..at(2));
}

#[test]
fn a_line_range_excludes_its_terminating_newline() {
    let buf = Buffer::new("ab\ncd");
    assert_eq!(buf.line_range(0), at(0)..at(2));
    assert_eq!(buf.line_range(1), at(3)..at(5));
}

#[test]
fn a_newline_is_a_separator_so_its_two_sides_are_different_offsets() {
    let buf = Buffer::new("ab\ncd");
    assert_eq!(buf.line_range(0).end, at(2));
    assert_eq!(buf.line_range(1).start, at(3));
    assert_eq!(buf.next_grapheme(at(2)), at(3));
}

#[test]
fn line_for_offset_answers_at_both_edges_of_a_line() {
    let buf = Buffer::new("ab\ncd");
    assert_eq!(buf.line_for_offset(at(0)), 0);
    assert_eq!(buf.line_for_offset(at(2)), 0);
    assert_eq!(buf.line_for_offset(at(3)), 1);
    assert_eq!(buf.line_for_offset(at(5)), 1);
}

#[test]
fn line_for_offset_past_the_end_answers_for_the_last_line() {
    let buf = Buffer::new("ab\ncd");
    assert_eq!(buf.line_for_offset(at(999)), 1);
}

#[test]
fn a_multi_byte_char_does_not_split_a_line() {
    let buf = Buffer::new("é\né");
    assert_eq!(buf.line_count(), 2);
    assert_eq!(buf.line_range(0), at(0)..at(2));
    assert_eq!(buf.line_range(1), at(3)..at(5));
}

#[test]
#[should_panic(expected = "past the last line")]
fn a_line_past_the_end_is_refused() {
    let _ = Buffer::new("a\nb").line_range(2);
}

#[test]
fn lines_agrees_with_line_range() {
    let buf = Buffer::new("ab\n\ncd\n");
    for (line, (start, text)) in buf.lines().enumerate() {
        let range = buf.line_range(line);
        assert_eq!(start, range.start, "line {line} start");
        assert_eq!(text, &buf.text()[range.start.0..range.end.0], "line {line}");
    }
    assert_eq!(buf.lines().count(), buf.line_count());
}

#[test]
fn the_line_index_follows_an_edit() {
    let mut buf = Buffer::new("ab");
    assert_eq!(buf.line_count(), 1);
    buf.insert(at(1), "\n");
    assert_eq!(buf.line_count(), 2);
    assert_eq!(buf.line_range(1), at(2)..at(3));
    buf.undo();
    assert_eq!(buf.line_count(), 1);
}

#[test]
fn clip_snaps_back_onto_a_char_boundary_and_into_range() {
    let buf = Buffer::new("héllo");
    assert_eq!(buf.clip(at(2)), at(1));
    assert_eq!(buf.clip(at(999)), buf.len());
}

/// The anchor table, exercised through the public API; pure `transform` cases live in `anchor.rs`.
mod anchors {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn an_edit_before_an_anchor_shifts_it_by_the_length_change() {
        let mut buf = Buffer::new("abcde");
        let anchor = buf.anchor(at(4), Bias::Left);

        buf.insert(at(0), "XY");
        assert_eq!(buf.resolve(anchor), at(6));
        buf.delete(at(0)..at(3));
        assert_eq!(buf.resolve(anchor), at(3));
        assert_eq!(&buf.text()[buf.resolve(anchor).0..], "e");
    }

    #[test]
    fn typing_at_an_anchor_carries_only_the_right_biased_one() {
        let mut buf = Buffer::new("ab");
        let left = buf.anchor(at(1), Bias::Left);
        let right = buf.anchor(at(1), Bias::Right);

        buf.insert(at(1), "XY");
        assert_eq!(buf.resolve(left), at(1));
        assert_eq!(buf.resolve(right), at(3));
    }

    #[test]
    fn an_anchor_between_two_edits_of_one_transaction_survives_undo() {
        let mut buf = Buffer::new("abcd");
        let anchor = buf.anchor(at(2), Bias::Left);

        buf.transact(|buf| {
            buf.insert(at(0), "<");
            buf.insert(at(5), ">");
        });
        assert_eq!(buf.text(), "<abcd>");
        assert_eq!(buf.resolve(anchor), at(3));

        buf.undo();
        assert_eq!(buf.text(), "abcd");
        assert_eq!(buf.resolve(anchor), at(2));

        buf.redo();
        assert_eq!(buf.resolve(anchor), at(3));
    }

    #[test]
    fn undoing_an_edit_before_an_anchor_puts_it_back() {
        let mut buf = Buffer::new("abcde");
        let anchor = buf.anchor(at(4), Bias::Left);

        buf.insert(at(1), "XYZ");
        assert_eq!(buf.resolve(anchor), at(7));
        buf.undo();
        assert_eq!(buf.resolve(anchor), at(4));
    }

    #[test]
    fn an_anchor_stays_addressable_across_undo_and_redo() {
        let mut buf = Buffer::new("héllo");
        let anchor = buf.anchor(at(3), Bias::Right);

        let addressable = |buf: &Buffer| {
            let offset = buf.resolve(anchor);
            assert!(offset <= buf.len(), "{offset} escapes {:?}", buf.text());
            assert!(
                buf.text().is_char_boundary(offset.0),
                "{offset} splits a char in {:?}",
                buf.text()
            );
        };

        buf.transact(|buf| buf.replace(at(1)..at(3), "🎉"));
        addressable(&buf);
        let end = buf.len();
        buf.insert(end, "!");
        addressable(&buf);
        buf.undo();
        addressable(&buf);
        buf.undo();
        addressable(&buf);
        buf.redo();
        addressable(&buf);
        buf.redo();
        addressable(&buf);
    }

    /// Two facts about deletion that look alike but differ — kept side by side so a "fix"
    /// to one isn't quietly applied to the other.
    mod a_deleted_anchor {
        use super::*;

        #[test]
        fn does_not_return_inside_the_text_on_undo() {
            let mut buf = Buffer::new("abcde");
            let left = buf.anchor(at(2), Bias::Left);
            let right = buf.anchor(at(2), Bias::Right);

            buf.replace(at(1)..at(4), "XY");
            assert_eq!(buf.text(), "aXYe");
            assert_eq!(buf.resolve(left), at(1));
            assert_eq!(buf.resolve(right), at(3));

            buf.undo();
            assert_eq!(buf.text(), "abcde");
            assert_eq!(buf.resolve(left), at(1));
            assert_eq!(buf.resolve(right), at(4));
        }

        #[test]
        fn tracks_the_end_of_a_pure_deletion_exactly() {
            let mut buf = Buffer::new("abc");
            let anchor = buf.anchor(at(1), Bias::Right);

            buf.delete(at(0)..at(1));
            assert_eq!(buf.resolve(anchor), at(0));
            buf.undo();
            assert_eq!(buf.text(), "abc");
            assert_eq!(buf.resolve(anchor), at(1));
        }
    }

    #[test]
    #[should_panic(expected = "not live in this buffer")]
    fn resolving_a_forgotten_anchor_is_refused() {
        let mut buf = Buffer::new("abc");
        let anchor = buf.anchor(at(1), Bias::Left);
        buf.forget(anchor);
        let _ = buf.resolve(anchor);
    }

    #[test]
    #[should_panic(expected = "not live in this buffer")]
    fn forgetting_an_anchor_twice_is_refused() {
        let mut buf = Buffer::new("abc");
        let anchor = buf.anchor(at(1), Bias::Left);
        buf.forget(anchor);
        buf.forget(anchor);
    }

    struct Tracked {
        anchor: Anchor,
        bias: Bias,
        created_at: ByteOffset,
        predicted: ByteOffset,
        live: bool,
        restorable: bool,
    }

    #[derive(Debug, Clone)]
    enum Op {
        Edit(usize, usize, String),
        Transaction(usize, usize, String, usize, String),
        Anchor(usize, Bias),
        Forget(usize),
        Undo,
        Redo,
    }

    fn text_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            Just("é".to_string()),
            Just("日本".to_string()),
            Just("🎉".to_string()),
            Just("e\u{301}".to_string()),
            Just("ab".to_string()),
            Just("\n".to_string()),
        ]
    }

    fn bias_strategy() -> impl Strategy<Value = Bias> {
        prop_oneof![Just(Bias::Left), Just(Bias::Right)]
    }

    fn edit_op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            4 => (any::<usize>(), any::<usize>(), text_strategy())
                .prop_map(|(a, b, text)| Op::Edit(a, b, text)),
            3 => (
                any::<usize>(),
                any::<usize>(),
                text_strategy(),
                any::<usize>(),
                text_strategy(),
            )
                .prop_map(|(a, b, replaced, c, inserted)| {
                    Op::Transaction(a, b, replaced, c, inserted)
                }),
            1 => any::<usize>().prop_map(Op::Forget),
        ]
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            8 => edit_op_strategy(),
            3 => (any::<usize>(), bias_strategy()).prop_map(|(at, bias)| Op::Anchor(at, bias)),
        ]
    }

    fn history_op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            8 => op_strategy(),
            1 => Just(Op::Undo),
            1 => Just(Op::Redo),
        ]
    }

    fn boundary(text: &str, index: usize) -> usize {
        let boundaries: Vec<usize> = text
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(text.len()))
            .collect();
        boundaries[index % boundaries.len()]
    }

    fn ordered_boundaries(text: &str, a: usize, b: usize) -> Range<usize> {
        let (a, b) = (boundary(text, a), boundary(text, b));
        a.min(b)..a.max(b)
    }

    fn survives_undo(
        offset: ByteOffset,
        bias: Bias,
        at: ByteOffset,
        removed_len: usize,
        inserted_len: usize,
    ) -> bool {
        if removed_len == 0 {
            return true;
        }
        let touched_end = at.0 + removed_len;
        let strictly_inside = offset.0 > at.0 && offset.0 < touched_end;
        let right_at_start = offset.0 == at.0 && bias == Bias::Right;
        let left_at_end = inserted_len == 0 && offset.0 == touched_end && bias == Bias::Left;
        !(strictly_inside || right_at_start || left_at_end)
    }

    fn apply_edit(buf: &mut Buffer, tracked: &mut [Tracked], range: Range<usize>, text: &str) {
        let removed_len = range.end - range.start;
        if removed_len == 0 && text.is_empty() {
            return;
        }
        let at = ByteOffset(range.start);
        for entry in tracked.iter_mut().filter(|entry| entry.live) {
            entry.restorable &=
                survives_undo(entry.predicted, entry.bias, at, removed_len, text.len());
            entry.predicted = transform(entry.predicted, entry.bias, at, removed_len, text.len());
        }
        buf.replace(at..ByteOffset(range.end), text);
    }

    fn run(buf: &mut Buffer, tracked: &mut Vec<Tracked>, op: Op) {
        match op {
            Op::Edit(a, b, text) => {
                let range = ordered_boundaries(buf.text(), a, b);
                apply_edit(buf, tracked, range, &text);
            }
            Op::Transaction(a, b, replaced, c, inserted) => {
                let first = ordered_boundaries(buf.text(), a, b);
                let mut after = buf.text().to_string();
                after.replace_range(first.clone(), &replaced);
                let second = boundary(&after, c);
                buf.transact(|buf| {
                    apply_edit(buf, tracked, first, &replaced);
                    apply_edit(buf, tracked, second..second, &inserted);
                });
            }
            Op::Anchor(offset, bias) => {
                let offset = ByteOffset(boundary(buf.text(), offset));
                let anchor = buf.anchor(offset, bias);
                tracked.push(Tracked {
                    anchor,
                    bias,
                    created_at: offset,
                    predicted: offset,
                    live: true,
                    restorable: true,
                });
            }
            Op::Forget(index) => {
                let live: Vec<usize> = tracked
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.live)
                    .map(|(index, _)| index)
                    .collect();
                if live.is_empty() {
                    return;
                }
                let chosen = live[index % live.len()];
                buf.forget(tracked[chosen].anchor);
                tracked[chosen].live = false;
            }
            Op::Undo => {
                buf.undo();
            }
            Op::Redo => {
                buf.redo();
            }
        }
    }

    fn check_addressable(buf: &Buffer, tracked: &[Tracked]) -> Result<(), TestCaseError> {
        for entry in tracked.iter().filter(|entry| entry.live) {
            let offset = buf.resolve(entry.anchor);
            prop_assert!(offset <= buf.len(), "{} escapes {:?}", offset, buf.text());
            prop_assert!(
                buf.text().is_char_boundary(offset.0),
                "{} splits a char in {:?}",
                offset,
                buf.text()
            );
        }
        Ok(())
    }

    fn live_offsets(buf: &Buffer, tracked: &[Tracked]) -> Vec<Option<ByteOffset>> {
        tracked
            .iter()
            .map(|entry| entry.live.then(|| buf.resolve(entry.anchor)))
            .collect()
    }

    proptest! {
        /// Bounds alone would pass a build with `shift_anchors` wired into only one call site.
        /// So every anchor is checked against what `transform` predicts for the exact edit issued.
        #[test]
        fn anchors_move_exactly_as_transform_predicts(
            ops in prop::collection::vec(op_strategy(), 0..24)
        ) {
            let mut buf = Buffer::new("héllo\n日本");
            let mut tracked = Vec::new();

            for op in ops {
                run(&mut buf, &mut tracked, op);
                check_addressable(&buf, &tracked)?;
                for entry in tracked.iter().filter(|entry| entry.live) {
                    prop_assert_eq!(
                        buf.resolve(entry.anchor),
                        entry.predicted,
                        "anchor {:?} drifted in {:?}",
                        entry.anchor,
                        buf.text()
                    );
                }
            }
        }

        #[test]
        fn undoing_every_edit_brings_undisturbed_anchors_home(
            starts in prop::collection::vec((any::<usize>(), bias_strategy()), 0..5),
            ops in prop::collection::vec(edit_op_strategy(), 0..24),
        ) {
            let mut buf = Buffer::new("héllo\n日本");
            let mut tracked = Vec::new();
            for (offset, bias) in starts {
                run(&mut buf, &mut tracked, Op::Anchor(offset, bias));
            }

            for op in ops {
                run(&mut buf, &mut tracked, op);
                check_addressable(&buf, &tracked)?;
            }
            while buf.undo().is_some() {
                check_addressable(&buf, &tracked)?;
            }

            for entry in tracked.iter().filter(|entry| entry.live && entry.restorable) {
                prop_assert_eq!(
                    buf.resolve(entry.anchor),
                    entry.created_at,
                    "anchor {:?} did not come home",
                    entry.anchor
                );
            }
        }

        /// `transform` is monotonic for a fixed bias, so same-bias anchors can converge but never cross.
        /// `Left`/`Right` on one offset are built to part company, so pairs here share one bias.
        #[test]
        fn anchors_of_one_bias_never_swap_places(
            ops in prop::collection::vec(history_op_strategy(), 0..24)
        ) {
            let mut buf = Buffer::new("héllo\n日本");
            let mut tracked = Vec::new();

            for op in ops {
                let before = live_offsets(&buf, &tracked);
                run(&mut buf, &mut tracked, op);
                check_addressable(&buf, &tracked)?;
                let after = live_offsets(&buf, &tracked);

                for (first, one) in tracked.iter().enumerate().take(before.len()) {
                    for (second, other) in tracked.iter().enumerate().take(before.len()) {
                        if first >= second || one.bias != other.bias {
                            continue;
                        }
                        let (Some(was_one), Some(was_other), Some(now_one), Some(now_other)) =
                            (before[first], before[second], after[first], after[second])
                        else {
                            continue;
                        };
                        if was_one <= was_other {
                            prop_assert!(
                                now_one <= now_other,
                                "anchors crossed: {} -> {} against {} -> {}",
                                was_one,
                                now_one,
                                was_other,
                                now_other
                            );
                        }
                    }
                }
            }
        }
    }
}
