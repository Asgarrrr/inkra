use super::{Key, strip_md};

#[test]
fn composed_and_decomposed_forms_fold_alike() {
    assert_eq!(Key::fold("Caf\u{e9}"), Key::fold("Cafe\u{301}"));
}

#[test]
fn case_folds_beyond_ascii() {
    assert_eq!(Key::fold("\u{c9}T\u{c9}"), Key::fold("\u{e9}t\u{e9}"));
    assert_eq!(Key::fold("Notes").as_str(), "notes");
}

#[test]
fn a_lowercased_fold_is_composed_again() {
    let dotted = Key::fold("\u{130}");
    assert_eq!(dotted, Key::fold("i\u{307}"));
    assert_eq!(dotted.as_str(), "i\u{307}");
}

#[test]
fn only_a_trailing_md_in_any_case_is_stripped() {
    assert_eq!(strip_md("plan.md"), "plan");
    assert_eq!(strip_md("plan.MD"), "plan");
    assert_eq!(strip_md("plan.Md"), "plan");
    assert_eq!(strip_md("a.b"), "a.b");
    assert_eq!(strip_md("x.md.md"), "x.md");
    assert_eq!(strip_md("md"), "md");
    assert_eq!(strip_md(".md"), "");
    assert_eq!(strip_md("é.md"), "é");
    assert_eq!(strip_md("aé"), "aé");
}

#[test]
fn a_path_key_keeps_its_folders_and_a_stem_key_drops_them() {
    assert_eq!(Key::path("Projects/Plan").as_str(), "projects/plan");
    assert_eq!(Key::stem("Projects/Plan").as_str(), "plan");
    assert_eq!(Key::stem("x.md").as_str(), "x.md");
    assert_eq!(Key::stem("Plan").as_str(), "plan");
}
