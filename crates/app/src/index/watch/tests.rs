use std::sync::Arc;

use gpui::{AppContext as _, Entity, Task, TestAppContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::{IndexWatch, Refresh};
use crate::index::IndexSync;
use crate::test_support::{scan_notes, vault};

fn sync(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> Entity<IndexSync> {
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    let links = cx.update(|cx| cx.new(|cx| IndexSync::new(&notes, cx)));
    cx.run_until_parked();
    links
}

fn on_notify(watch: &IndexWatch, links: &Entity<IndexSync>, cx: &TestAppContext) -> Refresh {
    links.read_with(cx, |links, _| watch.on_notify(links))
}

#[gpui::test]
fn a_seen_revision_is_skipped(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    let links = sync(cx, &fs);
    let mut watch = IndexWatch::default();
    links.read_with(cx, |links, _| watch.seen(links));

    assert_eq!(on_notify(&watch, &links, cx), Refresh::Skip);

    links.update(cx, IndexSync::simulate_load);
    assert_eq!(on_notify(&watch, &links, cx), Refresh::Now);
}

#[gpui::test]
fn a_notify_while_building_waits_once(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md"]);
    let _gate = fs.block_load("/vault/b.md");
    let links = sync(cx, &fs);
    let mut watch = IndexWatch::default();

    assert_eq!(on_notify(&watch, &links, cx), Refresh::Later);
    watch.throttle(Task::ready(()));
    links.update(cx, IndexSync::simulate_load);
    assert_eq!(
        on_notify(&watch, &links, cx),
        Refresh::Skip,
        "one refresh is already pending"
    );

    links.read_with(cx, |links, _| watch.seen(links));
    links.update(cx, IndexSync::simulate_load);
    assert_eq!(
        on_notify(&watch, &links, cx),
        Refresh::Later,
        "the refresh ran, so the next notify waits again"
    );
}

#[gpui::test]
fn a_notify_after_building_is_immediate(cx: &mut TestAppContext) {
    let fs = vault(&["a.md"]);
    let links = sync(cx, &fs);
    let mut watch = IndexWatch::default();
    watch.throttle(Task::ready(()));

    assert_eq!(on_notify(&watch, &links, cx), Refresh::Now);
}
