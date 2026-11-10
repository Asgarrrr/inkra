//! A tab keeps its code blocks' offsets while parked and in its history (code block scroll spec, D1).

use std::sync::Arc;

use gpui::{
    Pixels, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase, VisualTestContext, WindowHandle,
    point, px,
};
use inkra_vault::EntryId;
use inkra_vault::mem_fs::MemFs;

use super::splits::close;
use super::{AppShell, editor, keys, open, shell_focused};
use crate::test_support;

/// `a.md` holds one block too wide for the column; `b.md` is prose.
fn block_notes(
    cx: &mut TestAppContext,
) -> (WindowHandle<AppShell>, VisualTestContext, EntryId, EntryId) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file(
        "/vault/a.md",
        format!("```\n{}\n```\nbody", "x".repeat(120)),
    );
    fs.insert_file("/vault/b.md", "beta");
    let (window, notes, mut cx) = shell_focused(cx, &fs);
    let a = test_support::entry_id(&notes, &mut cx, "a");
    let b = test_support::entry_id(&notes, &mut cx, "b");
    (window, cx, a, b)
}

fn swipe(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, x: f32) {
    let at = editor(window, cx).update(cx, |editor, _| editor.block_center(0));
    cx.simulate_event(ScrollEvent {
        position: at.expect("the block is drawn"),
        delta: ScrollDelta::Pixels(point(px(x), px(0.))),
        touch_phase: TouchPhase::Started,
        ..ScrollEvent::default()
    });
    cx.run_until_parked();
}

fn block_x(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Pixels {
    editor(window, cx).update(cx, |editor, _| editor.block_x(0))
}

#[gpui::test]
fn a_parked_tab_keeps_its_block_offsets(cx: &mut TestAppContext) {
    let (window, mut cx, a, _b) = block_notes(cx);
    open(&window, &mut cx, a);
    swipe(&window, &mut cx, -100.);
    assert_eq!(block_x(&window, &mut cx), px(100.));

    keys(&mut cx, "cmd-t");
    keys(&mut cx, "ctrl-tab");

    assert_eq!(block_x(&window, &mut cx), px(100.));
    close(&mut cx);
}

#[gpui::test]
fn back_restores_block_offsets(cx: &mut TestAppContext) {
    let (window, mut cx, a, b) = block_notes(cx);
    open(&window, &mut cx, a);
    swipe(&window, &mut cx, -100.);
    assert_eq!(block_x(&window, &mut cx), px(100.));

    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-[");

    assert_eq!(block_x(&window, &mut cx), px(100.));
    close(&mut cx);
}
