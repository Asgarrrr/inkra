use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::TestAppContext;
use inkra_index::Group;
use inkra_vault::Fs;

use super::{Opened, empty_text, footer, listed, open, search, with_texts};
use crate::keymap::key_bindings;
use crate::open::FileActivated;
use crate::picker::{Picker, PickerDelegate as _};
use crate::quick_open::QuickOpenDelegate;
use crate::quick_open::render::{BUILDS, DRAWS};
use crate::test_support::{entry_id, index_sync, unscanned_notes, vault};
use crate::theme;

fn groups(opened: &mut Opened) -> Vec<(String, Group)> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .list
            .rows
            .iter()
            .map(|row| (row.found.path.to_string(), row.found.group))
            .collect()
    })
}

fn excerpts(opened: &mut Opened) -> Vec<Option<(String, Vec<Range<usize>>)>> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .list
            .rows
            .iter()
            .map(|row| {
                row.excerpt
                    .as_ref()
                    .map(|excerpt| (excerpt.text.clone(), excerpt.highlights.clone()))
            })
            .collect()
    })
}

#[gpui::test]
fn rows_come_in_three_groups(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("travel.md", "We plan the trip."),
        ("plan.md", "A trip to Rome."),
        ("plan trip.md", ""),
        ("other.md", "Nothing here."),
    ]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n space t r i p");

    assert_eq!(
        groups(&mut opened),
        [
            ("plan trip".to_owned(), Group::Name),
            ("plan".to_owned(), Group::NameAndText),
            ("travel".to_owned(), Group::Text),
        ]
    );
}

#[gpui::test]
fn a_text_row_shows_its_excerpt(cx: &mut TestAppContext) {
    let fs = with_texts(&[("travel.md", "# Travel\nWe plan the trip in May.\n")]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "t r i p");

    assert_eq!(
        excerpts(&mut opened),
        [Some((
            "We plan the trip in May.".to_owned(),
            vec![Range { start: 12, end: 16 }]
        ))]
    );
}

#[gpui::test]
fn a_name_row_shows_its_first_line(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "\n\n  Ship the release.\nThen rest.\n")]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");

    assert_eq!(
        excerpts(&mut opened),
        [Some(("Ship the release.".to_owned(), Vec::new()))]
    );
}

#[gpui::test]
fn a_note_not_loaded_has_no_second_line(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Ship it."), ("plan b.md", "Then rest.")]);
    let _gate = fs.block_load("/vault/plan.md");
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");

    assert_eq!(listed(&mut opened), ["plan", "plan b"]);
    assert_eq!(
        excerpts(&mut opened),
        [None, Some(("Then rest.".to_owned(), Vec::new()))]
    );
    let heights = ["picker-row-0", "picker-row-1"]
        .map(|row| opened.visual.debug_bounds(row).map(|row| row.size.height));
    let two_lines = Some(theme::UI_LINE_HEIGHT * 2.);
    assert_eq!(
        heights,
        [two_lines, two_lines],
        "an empty second line keeps its height"
    );
}

#[gpui::test]
fn the_footer_counts_the_notes_indexed_while_they_load(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md", "c.md"]);
    let _gate = fs.block_load("/vault/b.md");
    let mut opened = open(cx, &fs, None, &[]);

    assert_eq!(
        footer(&mut opened).as_deref(),
        Some("Indexing 2 of 3 notes")
    );
    assert!(opened.visual.debug_bounds("picker-footer").is_some());
}

#[gpui::test]
fn no_footer_once_the_notes_are_indexed(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "b.md"]);
    let mut opened = open(cx, &fs, None, &[]);

    assert_eq!(footer(&mut opened), None);
    assert!(opened.visual.debug_bounds("picker-footer").is_none());
}

#[gpui::test]
fn the_scanning_state_shows_no_footer(cx: &mut TestAppContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = vault(&["a.md", "f/b.md"]);
    let _gate = fs.block_read_dir("/vault/f");
    let notes = unscanned_notes(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    let links = index_sync(&notes, cx);
    let (picker, visual) = cx.add_window_view(move |window, cx| {
        let delegate = QuickOpenDelegate::new(&links, None, Vec::new(), cx);
        Picker::new(delegate, window, cx)
    });
    let mut opened = Opened {
        notes,
        picker,
        visual: visual.clone(),
    };
    opened.visual.run_until_parked();

    assert_eq!(empty_text(&mut opened), Some("Scanning the vault…".into()));
    assert_eq!(footer(&mut opened), None);
    assert!(opened.visual.debug_bounds("picker-footer").is_none());
}

#[gpui::test]
fn each_row_label_holds_the_path_and_the_excerpt(cx: &mut TestAppContext) {
    let fs = with_texts(&[("work/travel.md", "We plan\tthe trip."), ("trip.md", "")]);
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "t r i p");

    let labels = opened.picker.read_with(&opened.visual, |picker, _| {
        [0, 1].map(|ix| picker.delegate.row_label(ix).to_string())
    });
    assert_eq!(labels, ["trip", "work/travel: We plan the trip."]);
}

#[gpui::test]
fn enter_opens_a_text_only_match(cx: &mut TestAppContext) {
    let fs = with_texts(&[("travel.md", "We plan the trip."), ("other.md", "")]);
    let mut opened = open(cx, &fs, None, &[]);
    let travel = entry_id(&opened.notes, &mut opened.visual, "travel");
    let activated = Rc::new(RefCell::new(Vec::new()));
    let _guard = opened.visual.update(|_, cx| {
        let activated = Rc::clone(&activated);
        cx.subscribe(&opened.picker, move |_, event: &FileActivated, _| {
            activated.borrow_mut().push(event.file);
        })
    });

    search(&mut opened, "t r i p");
    opened.visual.simulate_keystrokes("enter");

    assert_eq!(*activated.borrow(), [travel]);
}

#[gpui::test]
fn an_nfd_file_name_is_listed_and_highlighted_on_its_nfc_path(cx: &mut TestAppContext) {
    let fs = vault(&["cafe\u{301}/e\u{301}te\u{301}.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "e t e");

    let texts = opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .list
            .rows
            .iter()
            .map(|row| {
                let text = &row.text;
                (
                    text.path.name.text().clone(),
                    text.path.folder.text().clone(),
                    text.path.name.ranges().to_vec(),
                )
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(texts.len(), 1);
    let (stem, folder, lit) = &texts[0];
    assert_eq!(stem.as_ref(), "\u{e9}t\u{e9}");
    assert_eq!(folder.as_ref(), "caf\u{e9}");
    assert_eq!(
        *lit,
        [Range { start: 0, end: 5 }],
        "byte ranges of the NFC word, not the NFD one"
    );
}

#[gpui::test]
fn the_picker_names_itself_and_each_row(cx: &mut TestAppContext) {
    let fs = vault(&["a.md", "work/b.md"]);
    let opened = open(cx, &fs, None, &["b", "a"]);
    opened.picker.read_with(&opened.visual, |picker, _| {
        let delegate = &picker.delegate;
        assert_eq!(delegate.key_context(), "QuickOpen");
        assert_eq!(delegate.label(), "Quick open");
        assert_eq!(delegate.placeholder(), "Open a note…");
        // A row reads as its NFC path, the text a screen reader announces.
        assert_eq!(delegate.row_label(0).as_ref(), "work/b");
        assert_eq!(delegate.row_label(1).as_ref(), "a");
        assert_eq!(delegate.row_label(2).as_ref(), "");
    });
}

/// A file name may hold a newline on macOS; `uniform_list` sizes every row from row 0.
#[gpui::test]
fn a_newline_in_a_file_name_keeps_the_name_on_one_line(cx: &mut TestAppContext) {
    let fs = vault(&["plan\nb.md", "plan c.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened).len(), 2);
    let row = opened.visual.debug_bounds("picker-row-0");

    assert_eq!(
        row.map(|row| row.size.height),
        Some(theme::UI_LINE_HEIGHT * 2.),
        "the name's line and the excerpt's"
    );
    assert!(
        opened.visual.debug_bounds("picker-row-1").is_some(),
        "the list is two rows high, so a taller row 0 hides row 1"
    );
}

#[gpui::test]
fn a_newline_in_a_file_name_is_a_space_in_the_row_label(cx: &mut TestAppContext) {
    let fs = vault(&["work/plan\nb.md"]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "b");

    let label = opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.delegate.row_label(0));

    assert_eq!(label.as_ref(), "work/plan b");
}

#[gpui::test]
fn a_stem_wider_than_the_row_is_cut_inside_it(cx: &mut TestAppContext) {
    let long = format!("work/{}.md", "a".repeat(400));
    let fs = vault(&[&long]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "a");
    assert_eq!(listed(&mut opened).len(), 1);

    let row = opened.visual.debug_bounds("picker-row-0");
    let stem = opened.visual.debug_bounds("row-name");

    assert!(
        row.zip(stem)
            .is_some_and(|(row, stem)| stem.right() <= row.right()),
        "the stem {stem:?} ends inside the row {row:?}"
    );
}

#[gpui::test]
fn a_draw_builds_no_row_text(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan a.md", "Ship it."), ("plan b.md", "Then rest.")]);
    let mut opened = open(cx, &fs, None, &[]);
    search(&mut opened, "p l a n");
    let (builds, draws) = (BUILDS.with(Cell::get), DRAWS.with(Cell::get));

    // Each move redraws both rows.
    opened.visual.simulate_keystrokes("down up down up");

    assert!(
        DRAWS.with(Cell::get) >= draws + 8,
        "four moves, two rows each"
    );
    assert_eq!(BUILDS.with(Cell::get), builds);
}
