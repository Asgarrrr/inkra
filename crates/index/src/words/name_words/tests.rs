use unicode_normalization::UnicodeNormalization;

use super::{NameWord, NameWords};
use crate::test_support::{note_set, vault};
use crate::words::WORD_MAX_CHARS;

fn words(names: &NameWords, order: usize) -> Vec<(String, std::ops::Range<usize>, bool)> {
    names
        .of(order)
        .map(
            |NameWord {
                 folded,
                 bytes,
                 in_stem,
             }| (folded.to_owned(), bytes, in_stem),
        )
        .collect()
}

fn one(path: &str) -> Vec<(String, std::ops::Range<usize>, bool)> {
    words(&NameWords::build([path]), 0)
}

#[test]
fn a_name_s_words_are_folded_with_their_path_ranges() {
    assert_eq!(
        one("Projets/Été 2026"),
        [
            ("projets".to_owned(), 0..7, false),
            ("ete".to_owned(), 8..13, true),
            ("2026".to_owned(), 14..18, true)
        ]
    );
}

#[test]
fn each_name_reads_its_own_words() {
    let names = NameWords::build(["a/b", "---", "c"]);
    assert_eq!(
        words(&names, 0),
        [("a".to_owned(), 0..1, false), ("b".to_owned(), 2..3, true)]
    );
    assert_eq!(words(&names, 1), []);
    assert_eq!(words(&names, 2), [("c".to_owned(), 0..1, true)]);
}

#[test]
fn an_nfd_file_name_folds_like_its_nfc_form() {
    let (_fs, vault) = vault(&["E\u{301}te\u{301}.md"]);
    let names = note_set(&vault).names();
    let path = names.path(0);
    assert_eq!(&**path, "E\u{301}te\u{301}".nfc().collect::<String>());
    let found: Vec<(String, std::ops::Range<usize>, bool)> = names
        .words()
        .of(0)
        .map(
            |NameWord {
                 folded,
                 bytes,
                 in_stem,
             }| (folded.to_owned(), bytes, in_stem),
        )
        .collect();
    assert_eq!(found, [("ete".to_owned(), 0..path.len(), true)]);
}

#[test]
fn a_hangul_name_folds_to_jamo() {
    assert_eq!(
        one("한글"),
        [(
            "\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}".to_owned(),
            0.."한글".len(),
            true
        )]
    );
}

#[test]
fn a_stem_with_no_word_char_has_no_stem_word() {
    assert_eq!(one("notes/---"), [("notes".to_owned(), 0..5, false)]);
    assert_eq!(
        one("notes/\u{301}"),
        [("notes".to_owned(), 0..5, false)],
        "a word folding to nothing is dropped"
    );
}

#[test]
fn a_long_name_word_is_capped() {
    assert_eq!(
        one(&"a".repeat(70)),
        [("a".repeat(WORD_MAX_CHARS), 0..70, true)]
    );
}

#[test]
fn heap_bytes_counts_the_folded_words_and_both_arrays() {
    let names = NameWords::build(["Ab/c", "d"]);
    assert_eq!(
        names.heap_bytes(),
        "abcd".len() + 3 * size_of::<super::Word>() + 3 * size_of::<u32>()
    );
    assert_eq!(NameWords::default().heap_bytes(), size_of::<u32>());
}
