use std::iter;

use unicode_normalization::UnicodeNormalization;

use super::{PathChar, QueryChar};

fn matches(query: char, path: char) -> bool {
    QueryChar::new(query).matches(PathChar::new(path))
}

#[test]
fn case_folds_both_ways() {
    assert!(matches('a', 'A'));
    assert!(matches('A', 'a'));
    assert!(matches('É', 'é'));
    assert!(matches('é', 'É'));
}

#[test]
fn a_plain_query_char_matches_its_accented_forms() {
    assert!(matches('e', 'é'));
    assert!(matches('e', 'É'));
    assert!(matches('E', 'ê'));
}

#[test]
fn an_accented_query_char_matches_only_itself() {
    assert!(!matches('é', 'e'));
    assert!(!matches('é', 'è'));
}

#[test]
fn the_kelvin_sign_folds_to_k() {
    assert!(matches('\u{212A}', 'k'));
    assert!(matches('k', '\u{212A}'));
}

#[test]
fn a_multi_char_lowercase_compares_by_its_first_char() {
    // `İ` lowercases to `i` + U+0307.
    assert!(matches('i', 'İ'));
}

#[test]
fn long_s_is_not_s() {
    assert!(!matches('s', 'ſ'));
}

#[test]
fn an_ascii_char_folds_as_the_general_rule_does() {
    for c in (0..=127u8).map(char::from) {
        let path = PathChar::new(c);
        let decomposed: String = iter::once(c).nfd().collect();
        assert_eq!(
            path.exact.to_string(),
            c.to_lowercase().to_string(),
            "{c:?}"
        );
        assert_eq!(path.base.to_string(), decomposed.to_lowercase(), "{c:?}");
    }
}
