//! Links to assets: resolution after the notes miss, and backlinks of an asset (K6).

use super::NoteIndex;
use super::tests::{backlinks, pair};
use crate::Resolution;
use crate::test_support::{id, load, note_set, target, vault};

/// What `link`, written in `from`, resolves to in an index of `files` with nothing loaded.
fn resolved(files: &[&str], from: &str, link: &str) -> (inkra_vault::Vault, Resolution) {
    let (_fs, vault) = vault(files);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    let resolution = index.resolve(Some(id(&vault, from)), &target(link));
    (vault, resolution)
}

#[test]
fn an_embed_resolves_to_the_asset_after_the_notes_miss() {
    let (vault, resolution) = resolved(&["a.md", "photo.png"], "a.md", "![[photo.png]]");
    assert_eq!(resolution, Resolution::File(id(&vault, "photo.png")));
}

#[test]
fn an_inline_relative_link_resolves_to_an_asset() {
    let (vault, resolution) = resolved(&["a.md", "sub/x.png", "x.png"], "a.md", "[a](sub/x.png)");
    assert_eq!(resolution, Resolution::File(id(&vault, "sub/x.png")));
}

#[test]
fn an_inline_image_resolves_to_an_asset() {
    let (vault, resolution) = resolved(
        &["sub/a.md", "sub/x.png", "x.png"],
        "sub/a.md",
        "![a](x.png)",
    );
    assert_eq!(resolution, Resolution::File(id(&vault, "sub/x.png")));
}

#[test]
fn an_unknown_asset_is_unresolved() {
    let (_, resolution) = resolved(&["a.md", "photo.png"], "a.md", "![[other.png]]");
    assert_eq!(resolution, Resolution::Unresolved);
}

#[test]
fn a_bare_name_does_not_hit_an_asset() {
    let (_, resolution) = resolved(&["a.md", "photo.png"], "a.md", "[[photo]]");
    assert_eq!(resolution, Resolution::Unresolved);
}

#[test]
fn a_note_named_like_a_file_stays_a_note_link() {
    let (vault, resolution) = resolved(&["a.md", "Node.js", "Node.js.md"], "a.md", "[[Node.js]]");
    assert_eq!(resolution, Resolution::File(id(&vault, "Node.js.md")));
}

#[test]
fn backlinks_of_an_asset_list_the_notes_that_embed_it() {
    let (_fs, vault) = vault(&["a.md", "b.md", "c.md", "pics/photo.png"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "b.md"), "![[photo.png]] [[photo]]");
    load(&mut index, id(&vault, "a.md"), "![a](pics/photo.png)");
    load(&mut index, id(&vault, "c.md"), "![[other.png]]");

    assert_eq!(
        backlinks(&index, &vault, "pics/photo.png"),
        [
            pair("a.md", &["![a](pics/photo.png)"]),
            pair("b.md", &["![[photo.png]]"])
        ]
    );
}

#[test]
fn a_link_to_photo_png_md_is_not_a_backlink_of_photo_png() {
    let (_fs, vault) = vault(&["a.md", "photo.png", "photo.png.md"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(
        &mut index,
        id(&vault, "a.md"),
        "![[photo.png]] [[photo.png.md]]",
    );

    assert_eq!(backlinks(&index, &vault, "photo.png"), []);
    assert_eq!(
        backlinks(&index, &vault, "photo.png.md"),
        [pair("a.md", &["![[photo.png]]", "[[photo.png.md]]"])]
    );
}

#[test]
fn a_wikilink_naming_a_missing_md_note_does_not_reach_the_asset() {
    let (_, resolution) = resolved(&["a.md", "photo.png"], "a.md", "[[photo.png.md]]");
    assert_eq!(resolution, Resolution::Unresolved);
}

#[test]
fn an_inline_link_naming_a_missing_md_note_does_not_reach_the_asset() {
    let (_, resolution) = resolved(&["a.md", "photo.png"], "a.md", "[a](photo.png.md)");
    assert_eq!(resolution, Resolution::Unresolved);
}

#[test]
fn a_link_to_a_missing_photo_png_md_is_not_a_backlink_of_photo_png() {
    let (_fs, vault) = vault(&["a.md", "photo.png"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "a.md"), "[[photo.png.md]]");
    assert_eq!(backlinks(&index, &vault, "photo.png"), []);
}

#[test]
fn photo_png_and_photo_png_md_do_not_share_a_backlink_answer() {
    let (_fs, vault) = vault(&["a.md", "b.md", "photo.png"]);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(note_set(&vault));
    load(&mut index, id(&vault, "a.md"), "[[photo.png.md]]");
    load(&mut index, id(&vault, "b.md"), "[[photo.png]]");
    let sources: Vec<_> = backlinks(&index, &vault, "photo.png")
        .into_iter()
        .map(|(source, _)| source)
        .collect();
    assert_eq!(sources, ["b.md"]);
}
