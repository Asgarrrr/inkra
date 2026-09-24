use inkra_markdown::links;

use super::LinkIndex;
use crate::test_support::{id, rename, vault};
use crate::{Resolution, Source};

/// Each backlink as (source path, the linked text of each occurrence).
fn backlinks(
    index: &LinkIndex,
    vault: &inkra_vault::Vault,
    note: &str,
) -> Vec<(String, Vec<String>)> {
    index
        .backlinks(id(vault, note))
        .into_iter()
        .map(|backlink| {
            let source = vault
                .entries()
                .find(|entry| entry.id == backlink.source)
                .map(|entry| entry.path.as_path().to_string_lossy().into_owned())
                .unwrap_or_default();
            let lits = backlink
                .links
                .iter()
                .map(|link| link.context[link.context_link.clone()].to_owned())
                .collect();
            (source, lits)
        })
        .collect()
}

fn pair(source: &str, lits: &[&str]) -> (String, Vec<String>) {
    (
        source.to_owned(),
        lits.iter().map(|lit| (*lit).to_owned()).collect(),
    )
}

#[test]
fn backlinks_group_occurrences_by_source_in_vault_order() {
    let (_fs, vault) = vault(&["a.md", "b.md", "Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    index.insert(
        id(&vault, "b.md"),
        Source::parse("[[Plan]] and [[plan|again]]"),
    );
    index.insert(id(&vault, "a.md"), Source::parse("[x](Plan.md) [[Other]]"));

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [
            pair("a.md", &["[x](Plan.md)"]),
            pair("b.md", &["[[Plan]]", "[[plan|again]]"])
        ]
    );
}

#[test]
fn a_note_linking_to_itself_is_its_own_backlink() {
    let (_fs, vault) = vault(&["Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    index.insert(id(&vault, "Plan.md"), Source::parse("[[Plan]]"));

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [pair("Plan.md", &["[[Plan]]"])]
    );
}

#[test]
fn a_link_resolving_elsewhere_is_not_a_backlink() {
    let (_fs, vault) = vault(&["src.md", "Plan.md", "work/Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    index.insert(id(&vault, "src.md"), Source::parse("[[work/Plan]]"));

    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert_eq!(
        backlinks(&index, &vault, "work/Plan.md"),
        [pair("src.md", &["[[work/Plan]]"])]
    );
}

#[test]
fn replacing_a_source_with_fewer_links_leaves_no_stale_row() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    let src = id(&vault, "src.md");
    index.insert(src, Source::parse("[[x]] [[y]] [[Plan]] [[Plan]]"));
    assert!(index.is_indexed(src));
    index.insert(src, Source::parse("[[Plan]]"));

    assert_eq!(
        backlinks(&index, &vault, "Plan.md"),
        [pair("src.md", &["[[Plan]]"])]
    );
    index.insert(src, Source::parse("nothing"));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert!(index.by_stem.is_empty());
}

#[test]
fn removing_a_source_drops_its_backlinks() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    let src = id(&vault, "src.md");
    index.insert(src, Source::parse("[[Plan]]"));
    index.remove(src);

    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
    assert!(!index.is_indexed(src));
    assert!(index.by_stem.is_empty());
}

#[test]
fn a_rename_moves_backlinks_without_reinserting() {
    let (fs, mut vault) = vault(&["src.md", "Plan.md", "Draft.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    let draft = id(&vault, "Draft.md");
    index.insert(id(&vault, "src.md"), Source::parse("[[Plan]] [[Final]]"));

    rename(&fs, &mut vault, "Plan.md", "Old.md");
    rename(&fs, &mut vault, "Draft.md", "Final.md");
    index.set_notes(vault.entries());

    assert_eq!(backlinks(&index, &vault, "Old.md"), []);
    assert_eq!(id(&vault, "Final.md"), draft);
    assert_eq!(
        backlinks(&index, &vault, "Final.md"),
        [pair("src.md", &["[[Final]]"])]
    );
}

#[test]
fn a_note_gone_from_the_vault_loses_its_source() {
    let (fs, mut vault) = vault(&["src.md", "Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    let src = id(&vault, "src.md");
    index.insert(src, Source::parse("[[Plan]]"));

    fs.remove(std::path::Path::new(&format!(
        "{}/src.md",
        crate::test_support::ROOT
    )));
    let Ok(()) = futures::executor::block_on(vault.rescan(&fs)) else {
        panic!("rescan");
    };
    index.set_notes(vault.entries());

    assert!(!index.is_indexed(src));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
}

#[test]
fn a_source_for_an_unknown_note_is_ignored() {
    let (_fs, vault) = vault(&["Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    index.insert(inkra_vault::EntryId::DETACHED, Source::parse("[[Plan]]"));

    assert!(!index.is_indexed(inkra_vault::EntryId::DETACHED));
    assert_eq!(backlinks(&index, &vault, "Plan.md"), []);
}

#[test]
fn backlinks_of_a_folder_or_unknown_id_are_empty() {
    let (_fs, vault) = vault(&["work/Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    index.insert(id(&vault, "work/Plan.md"), Source::parse("[[work]]"));

    assert_eq!(index.backlinks(id(&vault, "work")), []);
    assert_eq!(index.backlinks(inkra_vault::EntryId::DETACHED), []);
}

#[test]
fn resolve_link_parses_then_resolves() {
    let (_fs, vault) = vault(&["src.md", "Plan.md"]);
    let mut index = LinkIndex::default();
    index.set_notes(vault.entries());
    let found = links("[[Plan]] <https://x.org>");
    let src = id(&vault, "src.md");

    assert_eq!(
        index.resolve_link(src, &found[0]),
        Resolution::Note(id(&vault, "Plan.md"))
    );
    assert_eq!(
        index.resolve_link(src, &found[1]),
        Resolution::External("https://x.org".to_owned())
    );
}
