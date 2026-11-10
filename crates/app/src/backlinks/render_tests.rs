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
