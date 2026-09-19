use gpui_test_core::{Buffer, CharOffset};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(usize, String),
    Delete(usize, usize),
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
    ]
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (any::<usize>(), text_strategy()).prop_map(|(at, text)| Op::Insert(at, text)),
        3 => (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Delete(a.min(b), a.max(b))),
        1 => Just(Op::Undo),
        1 => Just(Op::Redo),
    ]
}

proptest! {
    #[test]
    fn buffer_matches_reference_model(ops in prop::collection::vec(op_strategy(), 0..30)) {
        let mut buf = Buffer::new("");
        let mut model: Vec<char> = Vec::new();
        let mut model_undo: Vec<Vec<char>> = Vec::new();
        let mut model_redo: Vec<Vec<char>> = Vec::new();

        for op in ops {
            match op {
                Op::Insert(at, text) => {
                    let at = at % (model.len() + 1);
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.insert(CharOffset(at), &text);
                    model.splice(at..at, text.chars());
                }
                Op::Delete(start, end) => {
                    if model.is_empty() {
                        continue;
                    }
                    let start = start % model.len();
                    let end = (start + 1 + end % model.len().max(1)).min(model.len());
                    model_undo.push(model.clone());
                    model_redo.clear();
                    buf.delete(CharOffset(start), CharOffset(end));
                    model.splice(start..end, std::iter::empty());
                }
                Op::Undo => {
                    let buf_did_undo = buf.undo();
                    match model_undo.pop() {
                        Some(prev) => {
                            prop_assert!(buf_did_undo, "model had undo history but Buffer::undo() returned false");
                            model_redo.push(std::mem::replace(&mut model, prev));
                        }
                        None => {
                            prop_assert!(!buf_did_undo, "model had no undo history but Buffer::undo() returned true");
                        }
                    }
                }
                Op::Redo => {
                    let buf_did_redo = buf.redo();
                    match model_redo.pop() {
                        Some(next) => {
                            prop_assert!(buf_did_redo, "model had redo history but Buffer::redo() returned false");
                            model_undo.push(std::mem::replace(&mut model, next));
                        }
                        None => {
                            prop_assert!(!buf_did_redo, "model had no redo history but Buffer::redo() returned true");
                        }
                    }
                }
            }

            let expected: String = model.iter().collect();
            prop_assert_eq!(buf.text(), expected.as_str());
        }
    }
}
