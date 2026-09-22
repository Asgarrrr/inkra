//! Copy, cut and paste through the system clipboard.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{ClipboardItem, TestAppContext};

use super::{at, dispatching};

/// `⌘C` then `⌘V`, through the real clipboard rather than a stub: the
/// `ClipboardItem` round trip is the half `inkra-editor` cannot reach.
#[gpui::test]
fn copy_and_paste_round_trip_through_the_clipboard(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");

    cx.simulate_keystrokes("cmd-a cmd-c");
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("abc".to_string())
    );

    cx.simulate_keystrokes("cmd-down cmd-v");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "abcabc");
        })
        .unwrap();
}

/// `⌘X` with nothing selected takes the whole line, and the line marker
/// survives the pasteboard — so pasting it back builds a line rather than
/// welding one onto another.
#[gpui::test]
fn cut_takes_the_whole_line_and_pastes_back_as_one(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc\ndef");

    cx.simulate_keystrokes("cmd-up cmd-x");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "def");
        })
        .unwrap();
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("abc\n".to_string())
    );

    cx.simulate_keystrokes("cmd-down cmd-v");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "abc\ndef");
        })
        .unwrap();
}

/// The second door into a `Buffer`. `Fs::load` normalizes what it reads; the
/// pasteboard is handed over verbatim (`gpui_macos/src/platform.rs:1159-1162`),
/// so `app` normalizes it here instead.
#[gpui::test]
fn pasting_windows_text_inserts_lf_only(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");

    cx.write_to_clipboard(ClipboardItem::new_string("a\r\nb\rc".to_string()));
    cx.simulate_keystrokes("cmd-v");

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).text(), "a\nb\nc");
        })
        .unwrap();
}

/// Both shapes of "nothing to paste": no item at all, and an item holding an
/// empty string. Neither may edit, and — the subtler half — neither may seal
/// the transaction, or it splits the typing run it landed in.
#[gpui::test]
fn pasting_nothing_changes_nothing_and_splits_no_typing_run(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "");

    // Nothing on the clipboard at all. The two cases leave through different
    // early exits, so neither covers the other.
    cx.simulate_input("a");
    cx.simulate_keystrokes("cmd-v");
    cx.simulate_input("b");
    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.buffer.read(cx).text(),
                "",
                "no clipboard item: both keystrokes were one undo step"
            );
        })
        .unwrap();

    // An item, holding an empty string.
    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
    cx.simulate_input("a");
    cx.simulate_keystrokes("cmd-v");
    cx.simulate_input("b");
    cx.simulate_keystrokes("cmd-z");
    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.buffer.read(cx).text(),
                "",
                "an empty item: both keystrokes were one undo step"
            );
        })
        .unwrap();
}

/// The metadata token is matched exactly, so a whole-line marker we did not
/// write cannot make us insert above the caret's line. gpui hands us Zed's own
/// pasteboard type (`gpui_macos/src/pasteboard.rs:48`), and Zed fills it with
/// JSON.
#[gpui::test]
fn a_foreign_line_marker_pastes_as_an_ordinary_span(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc");

    cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
        "XY\n".to_string(),
        r#"[{"len":2,"is_entire_line":true}]"#.to_string(),
    ));
    // Mid-line, so the two readings differ: treated as a line it would insert
    // at offset 0 and give "XY\nabc".
    cx.simulate_keystrokes("cmd-up right cmd-v");

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(
                view.buffer.read(cx).text(),
                "aXY\nbc",
                "inserted at the caret, not above the line"
            );
            assert_eq!(view.cursor(), at(4));
        })
        .unwrap();
}

/// `⌘C` must not reach the buffer at all. Version alone would not prove it:
/// routing `copy` through a mutable lease notifies without moving the version,
/// and the notification is what schedules an autosave for a note nobody edited.
/// The pattern is `saving_does_not_report_the_buffer_as_changed` in `vault.rs`.
#[gpui::test]
fn copying_notifies_no_observer_and_leaves_the_version_alone(cx: &mut TestAppContext) {
    let (window, mut cx) = dispatching(cx, "abc\ndef");

    let notified = Arc::new(AtomicUsize::new(0));
    let (version, _guard) = window
        .update(&mut cx, |view, _, cx| {
            let counter = Arc::clone(&notified);
            let guard = cx.observe(view.buffer(), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
            });
            (view.buffer.read(cx).version(), guard)
        })
        .unwrap();

    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.run_until_parked();

    window
        .update(&mut cx, |view, _, cx| {
            assert_eq!(view.buffer.read(cx).version(), version);
        })
        .unwrap();
    assert_eq!(
        notified.load(Ordering::SeqCst),
        0,
        "a copy changes no text, so nothing may observe one"
    );
}
