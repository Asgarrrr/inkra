use proptest::prelude::*;

use inkra_text::ByteOffset;

use super::{Language, TokenClass, block_tokens, highlight};

#[test]
fn every_info_word_of_the_languages_table_resolves_in_any_case() {
    let table = [
        (Language::Rust, &["rust", "rs"][..]),
        (Language::Python, &["python", "py"]),
        (Language::Json, &["json", "jsonc"]),
        (Language::Bash, &["bash", "sh", "shell", "zsh"]),
    ];
    for (language, words) in table {
        for word in words {
            let capitalised = word[..1].to_uppercase() + &word[1..];
            for case in [*word, &word.to_uppercase(), &capitalised] {
                assert_eq!(Language::from_info(case), Some(language), "{case}");
            }
        }
    }
}

#[test]
fn an_unknown_info_word_resolves_to_none() {
    for word in ["nope", "", "rusty", "rust ", "pyth", "jso", "bashh", "js"] {
        assert_eq!(Language::from_info(word), None, "{word:?}");
    }
}

#[test]
fn a_block_lexes_by_its_info_word_and_not_without_a_known_one() {
    let at = ByteOffset::new;
    let lines = || [(at(8), "let")].into_iter();
    let keyword = [(at(8)..at(11), TokenClass::Keyword)];
    assert_eq!(
        block_tokens("```xrs\nlet\n```", Some(at(4)..at(6)), lines()),
        keyword
    );
    assert_eq!(
        block_tokens("```rsx\nlet\n```", Some(at(3)..at(6)), lines()),
        []
    );
    assert_eq!(block_tokens("```rs\n\nlet\n```", None, lines()), []);
}

static FRAGMENTS: &[&str] = &[
    "fn", "let", "r#", "r#\"", "\"#", "\"", "'", "\\", "/*", "*/", "//", "b'", "x", " ", "é", "中",
    "🦀", "\n", "#", "<", ">", "(", ")", "{", "}", "[", "]", "!", ".", "1", "e", "-", ":", "'''",
    "\"\"\"", "@", "def ", "match ", "<<", "<<-", "EOF", "\t", "$", "${", "$(", "$((", "))", "`",
    ";;", "case ", " in", "esac", "for ", "[[", "=", "|", "&", ";", "true",
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
        for language in [Language::Rust, Language::Python, Language::Json, Language::Bash] {
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
