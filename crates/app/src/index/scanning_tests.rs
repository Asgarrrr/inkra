//! No build before the vault's first full scan, and one after it (open-first spec, O5).

use std::sync::Arc;
use std::time::{Instant, SystemTime};

use gpui::{AppContext as _, TestAppContext};
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{Fs, Mtime, Stamp};

use super::IndexSync;
use crate::test_support::{entry_id, unscanned_notes};
use crate::timeline::Timeline;
use crate::vault::NoteSaved;

#[gpui::test]
fn the_startup_load_runs_one_build(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let notes = unscanned_notes(cx, fs as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    assert!(links.read_with(cx, |links, _| links.awaits_first_build()));

    cx.run_until_parked();

    links.read_with(cx, |links, _| {
        assert_eq!(
            links.generation, 1,
            "the root pass and the full pass, one build"
        );
        assert!(!links.awaits_first_build());
        assert_eq!(links.index().names().len(), 2);
    });
}

#[gpui::test]
fn a_save_during_the_initial_scan_marks_no_indexed_step(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Timeline::new(Instant::now())));
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/f/b.md", "beta");
    let gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let _links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    cx.run_until_parked();

    let a = entry_id(&notes, cx, "a");
    notes.update(cx, |_, cx| {
        cx.emit(NoteSaved {
            id: a,
            text: Arc::from("alpha"),
            stamp: Stamp {
                mtime: Mtime(SystemTime::UNIX_EPOCH),
                size: 5,
            },
        });
    });
    cx.run_until_parked();
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.ends_with("indexed -"), "{line}");

    gate.release();
    cx.run_until_parked();
    let line = cx.read(|cx| cx.global::<Timeline>().line());
    assert!(line.contains("(2 notes"), "{line}");
}
