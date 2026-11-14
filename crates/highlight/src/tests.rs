use proptest::prelude::*;

use inkra_text::ByteOffset;

use super::{Language, highlight};

#[test]
fn a_rust_info_word_resolves_in_any_case() {
    for word in ["rust", "rs", "RUST", "Rs"] {
        assert_eq!(Language::from_info(word), Some(Language::Rust), "{word}");
    }
}

#[test]
fn an_unknown_info_word_resolves_to_none() {
    for word in ["nope", "", "rusty", "rust "] {
        assert_eq!(Language::from_info(word), None, "{word:?}");
    }
}

static FRAGMENTS: &[&str] = &[
    "fn", "let", "r#", "r#\"", "\"#", "\"", "'", "\\", "/*", "*/", "//", "b'", "x", " ", "é", "中",
    "🦀", "\n", "#", "<", ">", "(", ")", "{", "}", "[", "]", "!", ".", "1", "e", "-", ":",
];

proptest! {
    #[test]
    fn tokens_are_sorted_disjoint_and_on_char_boundaries_of_one_line(
        fragments in prop::collection::vec(prop::sample::select(FRAGMENTS), 0..64),
        gaps in prop::collection::vec(1usize..4, 65),
    ) {
        let text = fragments.concat();
        let mut lines = Vec::new();
        let mut start = 0;
        for (line, gap) in text.split('\n').zip(gaps) {
            lines.push((start, line));
            start += line.len() + gap;
        }
        for language in [Language::Rust] {
            let mut out = Vec::new();
            let offsets = lines.iter().map(|&(start, line)| (ByteOffset::new(start), line));
            highlight(language, offsets, &mut out);
            for pair in out.windows(2) {
                prop_assert!(pair[0].0.end <= pair[1].0.start, "{pair:?}");
            }
            for (range, _) in &out {
                let (start, end) = (range.start.get(), range.end.get());
                prop_assert!(start < end, "{range:?}");
                let Some(&(at, line)) = lines
                    .iter()
                    .find(|&&(at, line)| at <= start && end <= at + line.len())
                else {
                    return Err(TestCaseError::fail(format!("{range:?} in {lines:?}")));
                };
                prop_assert!(line.is_char_boundary(start - at), "{range:?} in {line:?}");
                prop_assert!(line.is_char_boundary(end - at), "{range:?} in {line:?}");
            }
        }
    }
}
