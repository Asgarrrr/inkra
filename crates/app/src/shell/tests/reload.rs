//! A clean note showing in the editor follows the disk, its caret and composition on their text
//! (watcher spec, F8).

use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle};
use inkra_text::ByteOffset;
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use crate::buffer_events::Reloaded;
use crate::editor::layout::shape_count;
use crate::keymap::key_bindings;
use crate::shell::AppShell;
use crate::test_support::{conflicts, entry_id};
use crate::vault::{AUTOSAVE_DELAY, NoteStore};

use super::{activate, editor, open_vault};
use crate::text_input::InputHost;

const A: &str = "/vault/a.md";

/// `a` showing with `text`, its caret at 0.
fn showing_a(
    cx: &mut TestAppContext,
    text: &str,
) -> (
    Arc<MemFs>,
    WindowHandle<AppShell>,
    Entity<NoteStore>,
    VisualTestContext,
) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    fs.insert_file(A, text);
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-up");
    (fs, window, notes, visual)
}

fn shown_text(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> String {
    editor(window, cx).read_with(cx, |editor, cx| editor.buffer().read(cx).text().to_owned())
}

fn marked(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> Option<Range<usize>> {
    let editor = editor(window, cx);
    window
        .update(cx, |_, window, cx| {
            editor.update(cx, |editor, cx| editor.marked_text_range(window, cx))
        })
        .expect("the shell is open")
}

#[gpui::test]
fn a_clean_note_changed_on_disk_keeps_the_caret_in_its_word_and_undoes_in_one_step(
    cx: &mut TestAppContext,
) {
    let old = "alpha\nthe quick brown fox\n";
    let new = "zero\nalpha\nthe quick red fox\n";
    let (fs, window, notes, mut visual) = showing_a(cx, old);
    let (preserved, _conflicts) = conflicts(&notes, cx);
    // Inside `quick`, after `qu`.
    visual.simulate_keystrokes("down right right right right right right");
    assert_eq!(
        editor(&window, cx).read_with(cx, |e, _| e.cursor().get()),
        12
    );

    fs.insert_file(A, new);
    cx.run_until_parked();

    assert_eq!(shown_text(&window, cx), new);
    let caret = editor(&window, cx).read_with(cx, |e, _| e.cursor().get());
    assert_eq!(&new[caret - 2..caret + 3], "quick");

    visual.simulate_keystrokes("cmd-z");
    assert_eq!(shown_text(&window, cx), old);

    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(fs.read_to_string(Path::new(A)).as_deref(), Some(old));
    assert!(preserved.borrow().is_empty());
}

#[gpui::test]
fn a_composition_saved_clean_keeps_its_text_across_a_reload(cx: &mut TestAppContext) {
    let (fs, window, _notes, mut visual) = showing_a(cx, "alpha");
    visual.simulate_keystrokes("cmd-down");
    let composing = editor(&window, cx);
    window
        .update(cx, |_, window, cx| {
            composing.update(cx, |editor, cx| {
                editor.replace_and_mark_text_in_range(None, "ka", Some(2..2), window, cx);
            });
        })
        .expect("the shell is open");
    cx.executor().advance_clock(AUTOSAVE_DELAY);
    cx.run_until_parked();
    assert_eq!(
        fs.read_to_string(Path::new(A)).as_deref(),
        Some("alphaka"),
        "the autosave took the composition, so the note is clean"
    );
    assert_eq!(marked(&window, cx), Some(5..7));

    fs.insert_file(A, "zero\nalphaka");
    cx.run_until_parked();
    assert_eq!(shown_text(&window, cx), "zero\nalphaka");
    assert_eq!(marked(&window, cx), Some(10..12));

    fs.insert_file(A, "zero\nalphaXY");
    cx.run_until_parked();
    assert_eq!(shown_text(&window, cx), "zero\nalphaXY");
    assert_eq!(marked(&window, cx), None);
}

fn numbered(prefix: &str, count: usize) -> String {
    (0..count).fold(String::new(), |mut text, n| {
        // Writing to a `String` cannot fail.
        let _ = writeln!(text, "{prefix} {n}");
        text
    })
}

#[gpui::test]
fn a_reload_keeps_the_top_lines_text_at_the_top(cx: &mut TestAppContext) {
    let old = numbered("line", 200);
    let (fs, window, _notes, _visual) = showing_a(cx, &old);
    let view = editor(&window, cx);
    let line_120 = ByteOffset::new(old.find("line 120\n").expect("line 120"));
    view.update(cx, |view, cx| view.scroll_to_top(line_120, cx));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.offset_at_top()), line_120);

    let new = format!("{}{old}", numbered("new", 30));
    fs.insert_file(A, new.as_str());
    cx.run_until_parked();

    assert_eq!(shown_text(&window, cx), new);
    assert_eq!(
        view.read_with(cx, |view, _| view.offset_at_top()),
        ByteOffset::new(new.find("line 120\n").expect("line 120"))
    );
}

#[gpui::test]
fn a_reload_while_scrolled_shapes_the_note_once(cx: &mut TestAppContext) {
    let old = numbered("line", 200);
    let (fs, window, _notes, _visual) = showing_a(cx, &old);
    let view = editor(&window, cx);
    let line_120 = ByteOffset::new(old.find("line 120\n").expect("line 120"));
    view.update(cx, |view, cx| view.scroll_to_top(line_120, cx));
    cx.run_until_parked();

    let before = shape_count();
    fs.insert_file(A, format!("{}{old}", numbered("new", 30)).as_str());
    cx.run_until_parked();
    assert_eq!(shape_count() - before, 1);
}

/// Both in one update, as two reloads landing between two frames: a test context draws after each.
#[gpui::test]
fn two_reloads_before_a_render_keep_the_top_lines_text_at_the_top(cx: &mut TestAppContext) {
    let old = numbered("line", 200);
    let (_fs, window, _notes, _visual) = showing_a(cx, &old);
    let view = editor(&window, cx);
    let line_120 = ByteOffset::new(old.find("line 120\n").expect("line 120"));
    view.update(cx, |view, cx| view.scroll_to_top(line_120, cx));
    cx.run_until_parked();

    let first = format!("{}{old}", numbered("new", 30));
    let second = format!("{}{first}", numbered("newer", 10));
    let buffer = view.read_with(cx, |view, _| view.buffer().clone());
    cx.update(|cx| {
        for (from, to) in [(&old, &first), (&first, &second)] {
            buffer.update(cx, |buffer, cx| {
                let diff = Arc::new(inkra_text::diff(buffer.version(), from, to));
                assert!(buffer.apply_diff(&diff));
                cx.emit(Reloaded(diff));
                cx.notify();
            });
        }
    });
    cx.run_until_parked();

    assert_eq!(shown_text(&window, cx), second);
    assert_eq!(
        view.read_with(cx, |view, _| view.offset_at_top()),
        ByteOffset::new(second.find("line 120\n").expect("line 120"))
    );
}

/// gpui's clamp decides the offset, not the restore; the claim is that nothing panics.
#[gpui::test]
fn a_reload_that_shrinks_the_note_below_the_viewport_does_not_panic(cx: &mut TestAppContext) {
    let old = numbered("line", 200);
    let (fs, window, _notes, _visual) = showing_a(cx, &old);
    let view = editor(&window, cx);
    let line_190 = ByteOffset::new(old.find("line 190\n").expect("line 190"));
    view.update(cx, |view, cx| view.scroll_to_top(line_190, cx));
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.scroll_y()) < gpui::px(0.));

    fs.insert_file(A, "short\n");
    cx.run_until_parked();

    assert_eq!(shown_text(&window, cx), "short\n");
    assert_eq!(view.read_with(cx, |view, _| view.scroll_y()), gpui::px(0.));
}
