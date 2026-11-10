use std::sync::Arc;

use inkra_vault::Stamp;

use crate::test_support::{id, load, mtime, resolver, vault};

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
