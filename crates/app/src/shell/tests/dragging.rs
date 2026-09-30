use gpui::{
    Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, WindowHandle, point,
    px, size,
};
use inkra_vault::EntryId;

use super::{
    AppShell, ab_focused, check, four_notes, keys, open, showing, start_open, tab_notes, text_of,
};

/// Presses on `from`, moves past gpui's 2 px threshold (`div.rs:52`), onto `to`, and releases there.
fn drag(cx: &mut VisualTestContext, from: &'static str, to: &'static str) {
    let to = cx.debug_bounds(to).expect("drawn").center();
    drag_to(cx, from, to);
}

fn drag_to(cx: &mut VisualTestContext, from: &'static str, to: Point<Pixels>) {
    let from = cx.debug_bounds(from).expect("drawn").center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        from + point(px(4.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

/// Tabs `[a, b, c]`, each opened after the last: `c` is active and the MRU is `c, b, a`.
fn abc(
    cx: &mut TestAppContext,
) -> (
    std::sync::Arc<inkra_vault::mem_fs::MemFs>,
    WindowHandle<AppShell>,
    VisualTestContext,
    [EntryId; 4],
) {
    let (fs, window, mut cx, [a, b, c, d]) = four_notes(cx);
    open(&window, &mut cx, a);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, b);
    keys(&mut cx, "cmd-t");
    open(&window, &mut cx, c);
    (fs, window, cx, [a, b, c, d])
}

/// Red before the drag listeners exist: it proves the helper too.
#[gpui::test]
fn dragging_a_tab_onto_a_later_one_puts_it_after(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = abc(cx);

    drag(&mut cx, "tab-0", "tab-2");

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(b), Some(c), Some(a)], 1)
    );
    assert_eq!(showing(&window, &mut cx), Some(c));
    check(&window, &mut cx);
}

#[gpui::test]
fn dragging_a_tab_onto_an_earlier_one_puts_it_before(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = abc(cx);

    drag(&mut cx, "tab-2", "tab-0");

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(c), Some(a), Some(b)], 0)
    );
    check(&window, &mut cx);
}

/// R1, R5: the drag fires no click, so `a` is not activated and the MRU keeps `b` next.
#[gpui::test]
fn dragging_an_inactive_tab_keeps_the_active_one_and_the_mru(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [_, b, c, _]) = abc(cx);

    drag(&mut cx, "tab-0", "tab-2");

    assert_eq!(showing(&window, &mut cx), Some(c));
    cx.simulate_input("x");
    assert_eq!(
        text_of(&window, &mut cx, c),
        "gammax",
        "the editor keeps focus"
    );
    keys(&mut cx, "cmd-w");
    assert_eq!(showing(&window, &mut cx), Some(b));
    check(&window, &mut cx);
}

/// R7.
#[gpui::test]
fn a_drop_outside_the_tabs_moves_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = abc(cx);
    let bar = cx.debug_bounds("tab-bar").expect("drawn");
    let editor = point(bar.center().x, bar.bottom() + px(100.));

    drag_to(&mut cx, "tab-0", editor);

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), Some(c)], 2)
    );
    assert!(!cx.update(|_, cx| cx.has_active_drag()), "no drag remains");
    check(&window, &mut cx);
}

/// R4, T4: a drop leaves the active tab, so a pending open still lands in it.
#[gpui::test]
fn a_drag_keeps_a_pending_open(cx: &mut TestAppContext) {
    let (fs, window, mut cx, [a, b, _, d]) = abc(cx);
    let gate = fs.block_load("/vault/d.md");
    start_open(&window, &mut cx, d);

    drag(&mut cx, "tab-0", "tab-2");
    gate.release();
    cx.run_until_parked();

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(b), Some(d), Some(a)], 1)
    );
    check(&window, &mut cx);
}

/// The picker's backdrop takes the press and dismisses the picker, so no drag starts.
#[gpui::test]
fn a_drag_under_a_picker_moves_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = abc(cx);
    keys(&mut cx, "cmd-p");

    drag(&mut cx, "tab-0", "tab-2");

    let modal = window
        .update(&mut cx, |shell, _, _| shell.modal.is_some())
        .unwrap();
    assert!(!modal, "the press dismissed the picker");
    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), Some(c)], 2)
    );
    check(&window, &mut cx);
}

/// R6. Tab 0 is `a`, narrower than "New tab": without the scroll, tab 19 sits left of the edge.
/// Tab 0 is scrolled out of the bar, so this calls the drop's handler.
#[gpui::test]
fn moving_a_tab_from_before_the_active_one_keeps_it_in_view(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, a, _b) = ab_focused(cx);
    cx.simulate_resize(size(px(700.), px(600.)));
    open(&window, &mut cx, a);
    for _ in 1..30 {
        cx.simulate_keystrokes("cmd-t");
    }
    cx.simulate_keystrokes("cmd-9");
    // Walking left leaves tab 20 at the bar's left edge, the scroll short of its end.
    for _ in 20..29 {
        cx.simulate_keystrokes("ctrl-shift-tab");
    }
    cx.run_until_parked();
    assert_eq!(tab_notes(&window, &mut cx).1, 20);

    window
        .update(&mut cx, |shell, _, cx| {
            let first = shell.tabs.id_at(0).unwrap();
            shell.move_tab(first, 29, cx);
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(tab_notes(&window, &mut cx).1, 19);
    let bar = cx.debug_bounds("tab-bar").unwrap();
    let active = cx.debug_bounds("tab-19").unwrap();
    assert!(
        active.left() >= bar.left() && active.right() <= bar.right(),
        "the active tab at {active:?} left the bar at {bar:?}"
    );
    check(&window, &mut cx);
}

/// A drag clears every pending press (gpui `div.rs:2234-2243`), so the ×'s click never fires; the
/// tab's drop onto itself moves nothing.
#[gpui::test]
fn a_drag_started_on_the_x_and_released_on_it_closes_nothing(cx: &mut TestAppContext) {
    let (_fs, window, mut cx, [a, b, c, _]) = abc(cx);

    drag(&mut cx, "tab-close-0", "tab-close-0");

    assert_eq!(
        tab_notes(&window, &mut cx),
        (vec![Some(a), Some(b), Some(c)], 2)
    );
    assert!(!cx.update(|_, cx| cx.has_active_drag()), "no drag remains");
    check(&window, &mut cx);
}
