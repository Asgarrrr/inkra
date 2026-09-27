//! The oracle's past failures, pinned: their seeds stop replaying once `case()` changes.

use proptest::test_runner::TestCaseError;

use super::model_cases::Case;
use super::model_tests::{highlights_are_word_prefixes, lists_what_the_model_lists};

fn case(files: &[&str], query: &str) -> Case {
    Case {
        files: files.iter().map(|file| (*file).to_owned()).collect(),
        scanned: vec![0; 8],
        loaded: vec![None; 8],
        query: query.to_owned(),
        showing: None,
        recent: Vec::new(),
    }
}

#[test]
fn an_empty_query_beside_a_final_sigma_text() -> Result<(), TestCaseError> {
    let mut case = case(&["a.md", "e.md", "é.md"], "");
    case.loaded[2] = Some(("ς".to_owned(), 0));
    case.recent = vec![0];
    lists_what_the_model_lists(&case, &[], 0)
}

#[test]
fn accented_names_under_accented_folders() -> Result<(), TestCaseError> {
    let files = ["a.md", "e.md", "e/ß.md", "e\u{301}.md", "é.md", "é/e.md"];
    lists_what_the_model_lists(&case(&files, "e"), &[], 0)
}

#[test]
fn a_final_sigma_query_with_a_note_showing() -> Result<(), TestCaseError> {
    let mut case = case(&["a.md", "e.md", "é.md", "ςa/a.md", "ςa/e.md"], "ς");
    case.loaded[1] = Some((String::new(), 1));
    case.showing = Some(1);
    lists_what_the_model_lists(&case, &[], 0)
}

#[test]
fn a_long_s_query_under_folders() -> Result<(), TestCaseError> {
    let mut case = case(&["a/a.md", "k/ſ.md", "किताब/ſ.md"], "ſ");
    case.scanned[2] = 1;
    lists_what_the_model_lists(&case, &[], 0)
}

#[test]
fn a_class_0_mark_before_the_stem_word() -> Result<(), TestCaseError> {
    highlights_are_word_prefixes(&case(&["a/\u{34f}e.md"], "e"))
}

#[test]
fn a_stem_word_also_a_folder_word() -> Result<(), TestCaseError> {
    let mut case = case(&["e.md", "e/l'été.md", "é.md"], "e");
    case.loaded[0] = Some((String::new(), 1));
    lists_what_the_model_lists(&case, &[], 0)
}
