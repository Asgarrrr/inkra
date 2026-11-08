//! The session keeps the window's frame and its display, so the next launch opens it there.

use gpui::{px, size};

use super::open;
use super::session_writes::{saved, saving, wait};
use crate::shell::session::THROTTLE;

#[gpui::test]
fn a_resize_alone_saves_the_new_frame_and_its_display(cx: &mut gpui::TestAppContext) {
    let (fs, window, mut cx, a, _) = saving(cx);
    open(&window, &mut cx, a);
    wait(&mut cx, THROTTLE);

    cx.simulate_resize(size(px(1200.), px(760.)));
    wait(&mut cx, THROTTLE);

    let saved = saved(&fs).and_then(|session| session.window);
    let saved = saved.expect("the window's frame");
    assert_eq!((saved.width, saved.height), (1200., 760.));
    let display = cx.update(|window, cx| window.display(cx).unwrap().uuid().unwrap().to_string());
    assert_eq!(saved.display, display);
}
