use super::{NameProblem, RelPath, check_name, note_file_name};
use std::path::Path;

#[test]
fn an_ordinary_name_is_accepted() {
    assert_eq!(check_name("Meeting notes"), None);
}

#[test]
fn an_empty_or_blank_name_is_refused() {
    assert_eq!(check_name(""), Some(NameProblem::Empty));
    assert_eq!(check_name("   "), Some(NameProblem::Empty));
}

#[test]
fn surrounding_whitespace_is_trimmed_not_refused() {
    assert_eq!(check_name("  plan  "), None);
    assert_eq!(note_file_name("  plan  "), "plan.md");
}

#[test]
fn a_separator_is_refused_rather_than_read_as_nesting() {
    assert_eq!(check_name("a/b"), Some(NameProblem::Separator));
}

#[test]
fn a_colon_is_refused_because_the_finder_shows_it_as_a_separator() {
    assert_eq!(check_name("plan:notes"), Some(NameProblem::Separator));
}

#[test]
fn a_leading_dot_is_refused() {
    assert_eq!(check_name(".plan"), Some(NameProblem::LeadingDot));
}

#[test]
fn the_reserved_names_are_refused() {
    assert_eq!(check_name("."), Some(NameProblem::Reserved));
    assert_eq!(check_name(".."), Some(NameProblem::Reserved));
}

#[test]
fn a_name_longer_than_the_filesystem_accepts_is_refused() {
    assert_eq!(check_name(&"a".repeat(252)), None);
    assert_eq!(check_name(&"a".repeat(253)), Some(NameProblem::TooLong));
}

#[test]
fn the_length_limit_counts_utf16_units_not_chars() {
    // Each emoji is 2 UTF-16 units; 126 of them plus `.md` is 255 exactly.
    assert_eq!(check_name(&"😀".repeat(126)), None);
    assert_eq!(check_name(&"😀".repeat(127)), Some(NameProblem::TooLong));
}

#[test]
fn a_bare_name_gains_the_extension() {
    assert_eq!(note_file_name("plan"), "plan.md");
}

#[test]
fn an_existing_extension_is_not_doubled() {
    assert_eq!(note_file_name("plan.md"), "plan.md");
}

/// `read_dir` matches `md` case-sensitively; `plan.MD` must not survive.
#[test]
fn an_uppercase_extension_is_normalized_rather_than_kept() {
    assert_eq!(note_file_name("plan.MD"), "plan.md");
    assert_eq!(note_file_name("plan.Md"), "plan.md");
}

#[test]
fn an_unrelated_extension_is_kept_and_the_note_one_added() {
    assert_eq!(note_file_name("notes.2026"), "notes.2026.md");
}

/// An empty stem would leave `.md` — a dotfile `read_dir` would hide.
#[test]
fn the_extension_alone_stays_a_name_rather_than_becoming_a_dotfile() {
    assert_eq!(note_file_name(".md"), ".md.md");
}

#[test]
fn every_accepted_name_is_one_relpath_accepts() {
    for name in [
        "plan",
        "plan.md",
        "notes.2026",
        "Meeting notes",
        "a",
        "élan",
        "日本語",
    ] {
        assert_eq!(check_name(name), None, "{name} should be accepted");
        assert!(
            RelPath::new(Path::new(&note_file_name(name))).is_some(),
            "{name} passed check_name but RelPath refused it"
        );
    }
}
