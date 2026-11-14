use super::Language;

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
