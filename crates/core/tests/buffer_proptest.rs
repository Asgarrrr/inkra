use gpui_test_core::{Buffer, CharOffset};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Insert(usize, String),
    Delete(usize, usize),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<usize>(), "[a-zA-Z]{1,3}").prop_map(|(at, text)| Op::Insert(at, text)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::Delete(a.min(b), a.max(b))),
    ]
}

proptest! {
    #[test]
    fn buffer_matches_reference_model(ops in prop::collection::vec(op_strategy(), 0..30)) {
        let mut buf = Buffer::new("");
        let mut model: Vec<char> = Vec::new();

        for op in ops {
            match op {
                Op::Insert(at, text) => {
                    let at = at % (model.len() + 1);
                    buf.insert(CharOffset(at), &text);
                    model.splice(at..at, text.chars());
                }
                Op::Delete(start, end) => {
                    if model.is_empty() {
                        continue;
                    }
                    let start = start % model.len();
                    let end = (start + 1 + end % model.len().max(1)).min(model.len());
                    buf.delete(CharOffset(start), CharOffset(end));
                    model.splice(start..end, std::iter::empty());
                }
            }

            let expected: String = model.iter().collect();
            prop_assert_eq!(buf.text(), expected.as_str());
        }
    }
}
