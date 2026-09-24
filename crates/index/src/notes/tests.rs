use super::{NoteTable, Resolution};
use crate::test_support::{id, target, vault};

fn resolves(files: &[&str], from: &str, link: &str) -> Option<String> {
    let (_fs, vault) = vault(files);
    let table = NoteTable::new(vault.entries());
    match table.resolve(id(&vault, from), &target(link)) {
        Resolution::Note(found) => vault
            .entries()
            .find(|entry| entry.id == found)
            .map(|entry| entry.path.as_path().to_string_lossy().into_owned()),
        Resolution::Unresolved => None,
        other => panic!("{link} resolved to {other:?}"),
    }
}

#[test]
fn a_name_finds_a_note_in_any_folder() {
    let files = ["src.md", "work/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[Plan]]").as_deref(),
        Some("work/Plan.md")
    );
}

#[test]
fn a_name_ignores_case_extension_and_normal_form() {
    let files = ["src.md", "Café.MD"];
    assert_eq!(
        resolves(&files, "src.md", "[[cafe\u{301}]]").as_deref(),
        Some("Café.MD")
    );
    assert_eq!(
        resolves(&files, "src.md", "[[CAFÉ.md]]").as_deref(),
        Some("Café.MD")
    );
}

#[test]
fn a_missing_name_is_unresolved() {
    assert_eq!(resolves(&["src.md"], "src.md", "[[Nope]]"), None);
}

#[test]
fn a_folder_is_never_a_candidate() {
    assert_eq!(
        resolves(&["src.md", "Plan/x.md"], "src.md", "[[Plan]]"),
        None
    );
}

#[test]
fn the_fewest_components_win_a_shared_name() {
    let files = ["src.md", "a/b/Plan.md", "c/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[Plan]]").as_deref(),
        Some("c/Plan.md")
    );
}

#[test]
fn vault_order_breaks_the_last_tie() {
    let files = ["src.md", "b/Plan.md", "a/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[Plan]]").as_deref(),
        Some("a/Plan.md")
    );
}

#[test]
fn the_exact_case_wins_before_depth() {
    let files = ["src.md", "plan.md", "deep/er/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[Plan]]").as_deref(),
        Some("deep/er/Plan.md")
    );
    assert_eq!(
        resolves(&files, "src.md", "[[plan]]").as_deref(),
        Some("plan.md")
    );
}

#[test]
fn the_exact_case_is_compared_composed() {
    let files = ["src.md", "cafe\u{301}.md", "x/Café.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[café]]").as_deref(),
        Some("cafe\u{301}.md")
    );
}

#[test]
fn a_suffix_matches_on_a_component_boundary() {
    let files = ["src.md", "work/Plan.md", "homework/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[work/Plan]]").as_deref(),
        Some("work/Plan.md")
    );
    assert_eq!(resolves(&files, "src.md", "[[ork/Plan]]"), None);
}

#[test]
fn a_suffix_may_be_the_whole_path_and_ignores_case() {
    let files = ["src.md", "a/Work/Plan.md", "Work/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[work/plan]]").as_deref(),
        Some("Work/Plan.md")
    );
    assert_eq!(
        resolves(&files, "src.md", "[[a/work/plan]]").as_deref(),
        Some("a/Work/Plan.md")
    );
}

#[test]
fn a_suffix_with_the_exact_case_wins_on_its_tail() {
    let files = ["src.md", "work/plan.md", "x/Work/Plan.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[Work/Plan]]").as_deref(),
        Some("x/Work/Plan.md")
    );
}

#[test]
fn a_relative_link_starts_from_the_source_folder() {
    let files = ["work/src.md", "work/Plan.md", "Plan.md"];
    assert_eq!(
        resolves(&files, "work/src.md", "[x](Plan.md)").as_deref(),
        Some("work/Plan.md")
    );
    assert_eq!(
        resolves(&files, "work/src.md", "[x](../Plan.md)").as_deref(),
        Some("Plan.md")
    );
    assert_eq!(
        resolves(&files, "work/src.md", "[x](./plan.md)").as_deref(),
        Some("work/Plan.md")
    );
    assert_eq!(
        resolves(&files, "work/src.md", "[[../Plan]]").as_deref(),
        Some("Plan.md")
    );
}

#[test]
fn a_relative_link_does_not_search_by_name() {
    let files = ["src.md", "work/Plan.md"];
    assert_eq!(resolves(&files, "src.md", "[x](Plan.md)"), None);
}

#[test]
fn a_rooted_link_starts_from_the_vault_root() {
    let files = ["work/src.md", "work/Plan.md", "Plan.md"];
    assert_eq!(
        resolves(&files, "work/src.md", "[x](/Plan.md)").as_deref(),
        Some("Plan.md")
    );
    assert_eq!(
        resolves(&files, "work/src.md", "[[/work/Plan]]").as_deref(),
        Some("work/Plan.md")
    );
}

#[test]
fn a_link_ending_in_a_folder_is_unresolved() {
    let files = ["sub/a.md", "sub.md"];
    for link in [
        "[x](../sub/)",
        "[x](../sub/.)",
        "[x](g/..)",
        "[x](.)",
        "[[sub/]]",
        "[[../sub/..]]",
    ] {
        assert_eq!(resolves(&files, "sub/a.md", link), None, "{link}");
    }
}

#[test]
fn a_relative_link_above_the_root_is_unresolved() {
    let files = ["src.md", "Plan.md"];
    assert_eq!(resolves(&files, "src.md", "[x](../Plan.md)"), None);
    assert_eq!(resolves(&files, "src.md", "[x](a/../../Plan.md)"), None);
    assert_eq!(
        resolves(&files, "src.md", "[x](a/../Plan.md)").as_deref(),
        Some("Plan.md")
    );
}

#[test]
fn a_relative_collision_breaks_ties_like_a_name() {
    let files = ["src.md", "Notes.md", "notes.md"];
    assert_eq!(
        resolves(&files, "src.md", "[x](notes.md)").as_deref(),
        Some("notes.md")
    );
    assert_eq!(
        resolves(&files, "src.md", "[x](Notes.md)").as_deref(),
        Some("Notes.md")
    );
    assert_eq!(
        resolves(&files, "src.md", "[[notes]]").as_deref(),
        Some("notes.md")
    );
}

#[test]
fn a_source_outside_the_table_resolves_names_but_not_relatives() {
    let (_fs, vault) = vault(&["Plan.md"]);
    let table = NoteTable::new(vault.entries());
    let outside = inkra_vault::EntryId::DETACHED;
    assert_eq!(
        table.resolve(outside, &target("[[Plan]]")),
        Resolution::Note(id(&vault, "Plan.md"))
    );
    assert_eq!(
        table.resolve(outside, &target("[x](Plan.md)")),
        Resolution::Unresolved
    );
    assert_eq!(
        table.resolve(outside, &target("[x](/Plan.md)")),
        Resolution::Note(id(&vault, "Plan.md"))
    );
}

#[test]
fn urls_and_self_links_pass_through() {
    let (_fs, vault) = vault(&["src.md"]);
    let table = NoteTable::new(vault.entries());
    let source = id(&vault, "src.md");
    assert_eq!(
        table.resolve(source, &target("<https://x.org>")),
        Resolution::External("https://x.org".to_owned())
    );
    assert_eq!(
        table.resolve(source, &target("[[#h]]")),
        Resolution::SelfLink
    );
}

#[test]
fn a_note_named_with_md_is_linked_by_its_full_name() {
    let files = ["src.md", "x.md.md"];
    assert_eq!(
        resolves(&files, "src.md", "[[x.md.md]]").as_deref(),
        Some("x.md.md")
    );
    assert_eq!(resolves(&files, "src.md", "[[x.md]]"), None);
}

#[test]
fn a_backslash_is_part_of_a_name_not_a_separator() {
    let files = ["src.md", "work/Plan.md"];
    assert_eq!(resolves(&files, "src.md", "[[work\\Plan]]"), None);
}
