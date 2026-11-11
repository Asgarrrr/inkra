use std::sync::Arc;

use gpui::{TestAppContext, px, size};
use inkra_vault::mem_fs::MemFs;

use super::tests::{drawn, show};

#[gpui::test]
fn a_context_landing_moves_no_row(cx: &mut TestAppContext) {
    const A: &str = "/vault/a.md";
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, format!("{} [[b]]", "word ".repeat(29)));
    fs.insert_file("/vault/b.md", "");
    fs.insert_file("/vault/c.md", "[[b]]");
    let (notes, panel, mut visual) = drawn(cx, &fs);
    visual.simulate_resize(size(px(220.), px(400.)));
    cx.run_until_parked();
    let gate = fs.block_load(A);

    show(cx, &notes, &panel, "b");
    let held = visual
        .debug_bounds("backlink-1")
        .expect("`backlink-1` is drawn");

    gate.release();
    visual.run_until_parked();

    assert_eq!(visual.debug_bounds("backlink-1"), Some(held));
}

#[gpui::test]
fn a_narrow_panel_cuts_the_text_before_the_link_not_the_link(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", format!("{} [[b]] after", "word ".repeat(11)));
    fs.insert_file("/vault/b.md", "");
    let (notes, panel, mut visual) = drawn(cx, &fs);
    visual.simulate_resize(size(px(800.), px(400.)));
    show(cx, &notes, &panel, "b");
    visual.run_until_parked();
    let whole = visual
        .debug_bounds("backlink-link")
        .expect("the link is drawn");

    visual.simulate_resize(size(px(220.), px(400.)));
    visual.run_until_parked();

    let row = visual
        .debug_bounds("backlink-0")
        .expect("`backlink-0` is drawn");
    let link = visual
        .debug_bounds("backlink-link")
        .expect("the link is drawn");
    assert!(whole.left() > row.right(), "{whole:?} fits {row:?} whole");
    assert_eq!(link.size, whole.size);
    assert!(link.right() <= row.right(), "{link:?} in {row:?}");
}
