use std::time::{Duration, Instant};

use inkra_editor::{EditorState, Goal, LogicalMovement, ParkedSelection, Selection};
use inkra_text::{Buffer, ByteOffset, Hunk, Version, diff};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Gesture {
    Type(String),
    Backspace,
    Delete,
    Move(Movement, bool),
    MoveTo(usize, bool),
    SelectAll,
    Undo,
    Redo,
}

#[derive(Debug, Clone, Copy)]
enum Movement {
    Left,
    Right,
    DocStart,
    DocEnd,
}

impl From<Movement> for LogicalMovement {
    fn from(movement: Movement) -> Self {
        match movement {
            Movement::Left => Self::Left,
            Movement::Right => Self::Right,
            Movement::DocStart => Self::DocStart,
            Movement::DocEnd => Self::DocEnd,
        }
    }
}

fn text_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z]{1,3}",
        Just("é".to_string()),
        Just("日".to_string()),
        Just("🎉".to_string()),
        Just("e\u{301}".to_string()),
        Just("👨\u{200d}👩\u{200d}👧".to_string()),
    ]
}

fn movement_strategy() -> impl Strategy<Value = Movement> {
    prop_oneof![
        Just(Movement::Left),
        Just(Movement::Right),
        Just(Movement::DocStart),
        Just(Movement::DocEnd),
    ]
}

fn gesture_strategy() -> impl Strategy<Value = Gesture> {
    prop_oneof![
        4 => text_strategy().prop_map(Gesture::Type),
        2 => Just(Gesture::Backspace),
        2 => Just(Gesture::Delete),
        4 => (movement_strategy(), any::<bool>()).prop_map(|(m, extend)| Gesture::Move(m, extend)),
        2 => (any::<usize>(), any::<bool>()).prop_map(|(at, extend)| Gesture::MoveTo(at, extend)),
        1 => Just(Gesture::SelectAll),
        2 => Just(Gesture::Undo),
        1 => Just(Gesture::Redo),
    ]
}

fn assert_addressable(buffer: &Buffer, state: &EditorState) -> Result<(), TestCaseError> {
    let selection = state.selection();
    prop_assert!(
        selection.start() <= selection.end(),
        "selection {}..{} is inverted",
        selection.start(),
        selection.end()
    );
    prop_assert!(
        selection.end() <= buffer.len(),
        "selection end {} escapes a buffer of {} bytes",
        selection.end(),
        buffer.len()
    );
    prop_assert!(
        buffer.text().is_char_boundary(selection.start().get())
            && buffer.text().is_char_boundary(selection.end().get()),
        "selection {}..{} splits a character in {:?}",
        selection.start(),
        selection.end(),
        buffer.text()
    );

    if let Some(marked) = state.marked_range() {
        prop_assert!(
            marked.end <= buffer.len(),
            "composing region {}..{} escapes a buffer of {} bytes",
            marked.start,
            marked.end,
            buffer.len()
        );
        prop_assert!(
            buffer.text().is_char_boundary(marked.start.get())
                && buffer.text().is_char_boundary(marked.end.get()),
            "composing region {}..{} splits a character",
            marked.start,
            marked.end
        );
    }

    let _ = buffer.slice(selection.start()..selection.end());
    Ok(())
}

proptest! {
    #[test]
    fn the_selection_stays_addressable_through_any_gesture_sequence(
        initial in prop::collection::vec(text_strategy(), 0..4),
        gestures in prop::collection::vec(gesture_strategy(), 0..40),
    ) {
        let mut buffer = Buffer::new(initial.concat());
        let mut state = EditorState::new(Selection::at(buffer.len()));
        let start = Instant::now();

        assert_addressable(&buffer, &state)?;

        for (index, gesture) in gestures.into_iter().enumerate() {
            let now = start + Duration::from_millis(500) * u32::try_from(index).unwrap_or(u32::MAX);

            match gesture {
                Gesture::Type(text) => {
                    let range = state.selection().range();
                    state.edit(&mut buffer, now, range, &text);
                }
                Gesture::Backspace => state.backspace(&mut buffer, now),
                Gesture::Delete => state.delete(&mut buffer, now),
                Gesture::Move(movement, extend) => {
                    state.move_(&buffer, movement.into(), extend);
                }
                Gesture::MoveTo(at, extend) => {
                    state.move_to(&buffer, ByteOffset::new(at % (buffer.len().get() + 1)), extend);
                }
                Gesture::SelectAll => state.select_all(&buffer),
                Gesture::Undo => {
                    state.undo(&mut buffer);
                }
                Gesture::Redo => {
                    state.redo(&mut buffer);
                }
            }

            assert_addressable(&buffer, &state)?;
        }
    }

    #[test]
    fn typing_over_a_selection_replaces_exactly_it(
        parts in prop::collection::vec(text_strategy(), 1..6),
        anchor in any::<usize>(),
        head in any::<usize>(),
        typed in text_strategy(),
    ) {
        let text = parts.concat();
        let mut buffer = Buffer::new(text.clone());
        let mut state = EditorState::new(Selection::at(ByteOffset::new(0)));

        state.move_to(&buffer, ByteOffset::new(anchor % (text.len() + 1)), false);
        state.move_to(&buffer, ByteOffset::new(head % (text.len() + 1)), true);

        let range = state.selection().range();
        let mut expected = text;
        expected.replace_range(range.start.get()..range.end.get(), &typed);

        state.edit(&mut buffer, Instant::now(), range.clone(), &typed);

        prop_assert_eq!(buffer.text(), expected.as_str());
        prop_assert_eq!(state.cursor(), range.start + typed.len());
        prop_assert!(state.selection().is_empty());
    }

    #[test]
    fn cut_then_paste_restores_the_text(
        parts in prop::collection::vec(text_strategy(), 1..6),
        trailing_newline in any::<bool>(),
        anchor in any::<usize>(),
        head in any::<usize>(),
    ) {
        let mut text = parts.join("\n");
        if trailing_newline {
            text.push('\n');
        }

        let mut buffer = Buffer::new(text.clone());
        let mut state = EditorState::new(Selection::at(ByteOffset::new(0)));
        state.move_to(&buffer, ByteOffset::new(anchor % (text.len() + 1)), false);
        state.move_to(&buffer, ByteOffset::new(head % (text.len() + 1)), true);

        let selection = state.selection().range();
        let line = buffer.line_range(buffer.line_for_offset(state.cursor()));
        // Gains a newline only when nothing is selected and the caret's line runs unterminated to the end.
        let gains_a_newline = selection.is_empty() && line.end >= buffer.len();

        let copied = state.cut(&mut buffer);
        state.paste(&mut buffer, &copied);

        let expected = if gains_a_newline {
            format!("{text}\n")
        } else {
            text.clone()
        };
        prop_assert_eq!(buffer.text(), expected.as_str());

        if !selection.is_empty() {
            prop_assert_eq!(state.cursor(), selection.end);
        }
    }
}

fn reload_text_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            "[ab ]{1,3}",
            Just("\n".to_string()),
            Just("é".to_string()),
            Just("日".to_string()),
            Just("🎉".to_string()),
        ],
        0..12,
    )
    .prop_map(|parts| parts.concat())
}

/// The char boundary `pick` lands on in `text`, the end included.
fn boundary(text: &str, pick: usize) -> ByteOffset {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .collect();
    ByteOffset::new(boundaries[pick % boundaries.len()])
}

/// A selection from `tail` to `head`, carrying a goal so a kept or dropped goal shows.
fn selection_on(text: &str, tail: usize, head: usize) -> Selection {
    let mut selection = Selection::at(boundary(text, tail));
    selection.set_head(boundary(text, head), Goal::Column(7.5));
    selection
}

/// Distinct punctuation and symbols: each char is its own token, so `diff` finds the literal edit.
const UNIQUE: &str = "!#$%&()*+,-./:;<=>?@[]^`{|}~\n§¶•→★♥€£¥©®±×÷¿¡«»🎉🚀";

proptest! {
    #[test]
    fn parking_through_a_reload_matches_map_through(
        old in reload_text_strategy(),
        new in reload_text_strategy(),
        tail in any::<usize>(),
        head in any::<usize>(),
    ) {
        let selection = selection_on(&old, tail, head);
        let mut oracle = EditorState::new(selection.clone());
        oracle.map_through(&diff(Version::default(), &old, &new));

        let mut buffer = Buffer::new(old.clone());
        let parked = ParkedSelection::park(&selection, &mut buffer);
        prop_assert!(buffer.apply_diff(&diff(buffer.version(), &old, &new)));
        let unparked = parked.unpark(&mut buffer);

        prop_assert_eq!(unparked.range(), oracle.selection().range());
        prop_assert_eq!(unparked.reversed(), oracle.selection().reversed());
        if old != new {
            prop_assert_eq!(unparked.goal(), oracle.selection().goal());
        }
    }

    #[test]
    fn parking_through_a_sequence_of_edits_matches_map_through(
        chars in Just(UNIQUE.chars().collect::<Vec<_>>()).prop_shuffle(),
        initial in 0..12usize,
        tail in any::<usize>(),
        head in any::<usize>(),
        edits in prop::collection::vec((any::<usize>(), any::<usize>(), 0..4usize), 1..8),
    ) {
        let mut fresh = chars.into_iter();
        let text: String = fresh.by_ref().take(initial).collect();
        let selection = selection_on(&text, tail, head);
        let mut oracle = EditorState::new(selection.clone());
        let mut buffer = Buffer::new(text);
        let parked = ParkedSelection::park(&selection, &mut buffer);

        for (from, to, inserted) in edits {
            let before = buffer.text().to_string();
            let (from, to) = (boundary(&before, from), boundary(&before, to));
            let range = from.min(to)..from.max(to);
            let text: String = fresh.by_ref().take(inserted).collect();
            if range.is_empty() && text.is_empty() {
                continue;
            }
            let version = buffer.version();
            buffer.transact(|buffer| buffer.replace(range.clone(), &text));
            let edit = diff(version, &before, buffer.text());
            prop_assume!(edit.hunks() == [Hunk { range, text }]);
            oracle.map_through(&edit);
        }

        let unparked = parked.unpark(&mut buffer);
        prop_assert_eq!(unparked.range(), oracle.selection().range());
        prop_assert_eq!(unparked.reversed(), oracle.selection().reversed());
    }
}
