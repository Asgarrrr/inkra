use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px};

use super::refresh::{advance, hold_scans, notify, runs, selected};
use super::{Opened, listed, open, search};
use crate::index::BUILDING_REFRESH;
use crate::open::NoteActivated;
use crate::picker::runs::NOTES_CHANGED_REFRESH;
use crate::test_support::{entry_id, vault};
use crate::theme;

fn logged(opened: &mut Opened) -> usize {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.logged)
}

fn plans() -> Vec<String> {
    (0..15)
        .map(|n| format!("plan {n:02}.md"))
        .chain(["other.md".to_owned()])
        .collect()
}

/// A load applying while the vault indexes: the throttled refresh starts.
fn refresh_in_background(opened: &mut Opened) {
    notify(opened);
    advance(opened, BUILDING_REFRESH);
}

#[gpui::test]
fn enter_during_a_background_refresh_confirms_the_row_shown_at_once(cx: &mut TestAppContext) {
    let fs = vault(&["plan a.md", "plan b.md", "other.md"]);
    let _indexing = fs.block_load("/vault/other.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    opened.visual.simulate_keystrokes("down");
    assert_eq!(listed(&mut opened), ["plan b", "plan a"]);
    let a = entry_id(&opened.notes, &mut opened.visual, "plan a");
    let activated = Rc::new(RefCell::new(Vec::new()));
    let _guard = opened.visual.update(|_, cx| {
        let activated = Rc::clone(&activated);
        cx.subscribe(&opened.picker, move |_, event: &NoteActivated, _| {
            activated.borrow_mut().push(event.note);
        })
    });
    let release = hold_scans(&mut opened);
    let before = runs(&mut opened);
    refresh_in_background(&mut opened);
    assert_eq!(runs(&mut opened), before + 1, "a refresh is in flight");

    opened.visual.simulate_keystrokes("enter");
    assert_eq!(*activated.borrow(), [a], "at once, the row on screen");

    let _ = release.send(());
    opened.visual.run_until_parked();
    assert_eq!(
        *activated.borrow(),
        [a],
        "the refresh landing confirms nothing"
    );
}

#[gpui::test]
fn a_background_refresh_keeps_the_selection_and_the_scroll(cx: &mut TestAppContext) {
    let files = plans();
    let fs = vault(&files.iter().map(String::as_str).collect::<Vec<_>>());
    let _indexing = fs.block_load("/vault/other.md");
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened).len(), 15);
    opened.visual.simulate_keystrokes("down down down");
    let list_bounds = opened.visual.debug_bounds("picker-list");
    assert!(list_bounds.is_some(), "the list is drawn");
    let row_height = theme::UI_LINE_HEIGHT * 2.;
    // Past the end: the list stops at its last row, with row 3 out of view.
    opened.visual.simulate_event(ScrollWheelEvent {
        position: list_bounds.map(|list| list.center()).unwrap_or_default(),
        delta: ScrollDelta::Pixels(point(px(0.), -(row_height * 6.))),
        modifiers: Modifiers::none(),
        ..ScrollWheelEvent::default()
    });
    assert!(opened.visual.debug_bounds("picker-row-3").is_none());
    let last = opened.visual.debug_bounds("picker-row-14");
    assert!(last.is_some(), "scrolled to the end");
    let before = runs(&mut opened);

    refresh_in_background(&mut opened);

    assert_eq!(runs(&mut opened), before + 1, "the refresh ran");
    assert_eq!(selected(&mut opened), Some(3));
    assert!(
        opened.visual.debug_bounds("picker-row-3").is_none(),
        "the selection is not scrolled to"
    );
    assert_eq!(opened.visual.debug_bounds("picker-row-14"), last);
}

#[gpui::test]
fn only_a_keystroke_s_run_is_logged(cx: &mut TestAppContext) {
    let fs = vault(&["plan.md", "other.md"]);
    let _indexing = fs.block_load("/vault/other.md");
    let mut opened = open(cx, &fs, None, &[]);
    let (runs_before, logged_before) = (runs(&mut opened), logged(&mut opened));

    refresh_in_background(&mut opened);
    assert_eq!(runs(&mut opened), runs_before + 1, "the refresh ran");
    assert_eq!(
        logged(&mut opened),
        logged_before,
        "a refresh is not logged"
    );

    search(&mut opened, "p");
    assert_eq!(logged(&mut opened), logged_before + 1);
}

#[gpui::test]
fn a_refresh_that_drops_the_kept_note_selects_and_reveals_the_first_row(cx: &mut TestAppContext) {
    let files = plans();
    let fs = vault(&files.iter().map(String::as_str).collect::<Vec<_>>());
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    opened.visual.simulate_keystrokes(&["down"; 12].join(" "));
    assert_eq!(selected(&mut opened), Some(12));
    assert!(opened.visual.debug_bounds("picker-row-0").is_none());
    let name = listed(&mut opened).swap_remove(12);
    let kept = entry_id(&opened.notes, &mut opened.visual, &name);

    let trashed = opened
        .notes
        .update(&mut opened.visual, |notes, cx| notes.trash(kept, cx));
    opened.visual.run_until_parked();
    drop(trashed);
    advance(&mut opened, NOTES_CHANGED_REFRESH);

    assert_eq!(listed(&mut opened).len(), 14);
    assert_eq!(selected(&mut opened), Some(0));
    assert!(
        opened.visual.debug_bounds("picker-row-0").is_some(),
        "scrolled to"
    );
}
