//! `Rebuild index` from the palette: the cache file removed, every note loaded again (P15).

use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use gpui::TestAppContext;
use inkra_index::persist::MAX_BYTES;
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::tree_focused;
use crate::shell::{AppShell, Left};
use crate::test_support;

const CACHE: &str = "/cache/index.idx";

#[gpui::test]
fn rebuild_index_removes_the_cache_file_and_loads_every_note_again(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/b.md", "beta");
    let notes = test_support::scan_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let window = cx.add_window(move |window, cx| {
        AppShell::new(
            None,
            notes,
            None,
            Some(CACHE.into()),
            Left::default(),
            window,
            cx,
        )
    });
    let sidebar = window
        .update(cx, |shell, _, _| shell.panes().sidebar.clone())
        .expect("the shell is open");
    let mut visual = tree_focused(cx, window, &sidebar);
    let cache = || block_on(fs.read_bytes(Path::new(CACHE), MAX_BYTES));
    let loads = || ["a.md", "b.md"].map(|file| fs.loads(&Path::new("/vault").join(file)));
    assert!(cache().is_ok(), "written at the startup's end");
    assert_eq!(loads(), [1, 1]);

    visual.simulate_keystrokes("cmd-shift-p");
    visual.simulate_input("rebuild index");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();

    assert_eq!(
        cache().err().and_then(|error| error.io_kind()),
        Some(ErrorKind::NotFound)
    );
    assert_eq!(loads(), [2, 2]);
}
