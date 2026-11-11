use std::sync::Arc;

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Stamp, Vault};

use crate::NoteIndex;
use crate::test_support::{ROOT, content, id, load, mtime, note_set, rename, resolver, vault};

#[test]
fn a_backlink_carries_its_source_candidate() {
    let (_fs, vault) = vault(&["a.md", "b.md"]);
    let mut index = resolver(&vault);
    let a = id(&vault, "a.md");
    load(&mut index, a, "see [[b]]");

    let backlinks = index.backlinks(id(&vault, "b.md"));

    let candidate = backlinks[0].candidate();
    let stamp = Stamp {
        mtime: mtime(0),
        size: "see [[b]]".len() as u64,
    };
    assert_eq!(
        (candidate.id, &candidate.path, candidate.stamp),
        (a, &Arc::from("a.md"), stamp)
    );
}

type Change = fn(&MemFs, &mut Vault, &mut NoteIndex, EntryId);

#[test]
fn a_backlink_reads_as_another_only_with_its_candidate_slice_and_occurrences() {
    const SOURCE: &str = "[[x]] [[d/x]]";
    let cases: [(&str, Change, bool); 5] = [
        ("rebuilt unchanged", |_, _, _, _| {}, true),
        (
            "reloaded with the same text",
            |_, _, index, s| load(index, s, SOURCE),
            false,
        ),
        (
            "reloaded at another stamp",
            |_, _, index, s| index.insert(s, content(SOURCE, mtime(1))),
            false,
        ),
        (
            "a root x takes [[x]]",
            |fs, vault, index, _| {
                fs.insert_file(format!("{ROOT}/x.md"), "");
                assert!(block_on(vault.rescan(fs)).is_ok());
                let _ = index.set_notes(note_set(vault));
            },
            false,
        ),
        (
            "renamed",
            |fs, vault, index, _| {
                rename(fs, vault, "s.md", "t.md");
                let _ = index.set_notes(note_set(vault));
            },
            false,
        ),
    ];
    for (name, change, reads_as) in cases {
        let (fs, mut vault) = vault(&["s.md", "d/x.md"]);
        let mut index = resolver(&vault);
        let (s, x) = (id(&vault, "s.md"), id(&vault, "d/x.md"));
        load(&mut index, s, SOURCE);
        let before = index.backlinks(x).remove(0);

        change(&fs, &mut vault, &mut index, s);

        let after = index.backlinks(x).remove(0);
        assert_eq!(before.reads_as(&after), reads_as, "{name}");
    }
}
