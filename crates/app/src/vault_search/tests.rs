use std::sync::Arc;
use std::time::Duration;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::rows::Row;
use super::{VaultSearchDelegate, search_line};
use crate::keymap::key_bindings;
use crate::picker::runs::SEARCH_DEBOUNCE;
use crate::picker::{Picker, PickerDelegate as _};
use crate::test_support::{index_sync, scan_notes, vault_of};
use crate::vault::NoteStore;

mod keys;
mod refresh;
mod rows;
mod states;

pub(super) struct Opened {
    pub(super) notes: Entity<NoteStore>,
    pub(super) picker: Entity<Picker<VaultSearchDelegate>>,
    pub(super) visual: VisualTestContext,
}

pub(super) fn with_texts(files: &[(&str, &str)]) -> Arc<MemFs> {
    let fs = Arc::new(MemFs::new());
    for (file, text) in files {
        fs.insert_file(format!("/vault/{file}"), *text);
    }
    fs
}

pub(super) fn open_over(cx: &mut TestAppContext, notes: Entity<NoteStore>) -> Opened {
    let links = index_sync(&notes, cx);
    let vault = vault_of(&notes, cx);
    let (picker, visual) = cx.add_window_view(move |window, cx| {
        let delegate = VaultSearchDelegate::new(&links, vault, cx);
        Picker::new(delegate, window, cx)
    });
    let mut visual = visual.clone();
    visual.update(|window, cx| window.focus(&picker.focus_handle(cx), cx));
    visual.run_until_parked();
    Opened {
        notes,
        picker,
        visual,
    }
}

/// The index has built before the picker opens.
pub(super) fn open(cx: &mut TestAppContext, fs: &Arc<MemFs>) -> Opened {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let notes = scan_notes(cx, Arc::clone(fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    open_over(cx, notes)
}

/// `# name folder (count)` for a header, `line: context` for a hit.
pub(super) fn listed(opened: &mut Opened) -> Vec<String> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        picker
            .delegate
            .results
            .rows
            .iter()
            .map(|row| match row {
                Row::Header(header) => {
                    format!(
                        "# {} {}({})",
                        header.path.name.text(),
                        header.path.folder.text(),
                        header.count
                    )
                }
                Row::Hit(hit) => format!("{}: {}", hit.line, hit.context.text()),
            })
            .collect()
    })
}

pub(super) fn labels(opened: &mut Opened) -> Vec<String> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        (0..picker.delegate.match_count())
            .map(|ix| picker.delegate.row_label(ix).to_string())
            .collect()
    })
}

pub(super) fn selected(opened: &mut Opened) -> Option<usize> {
    opened
        .picker
        .read_with(&opened.visual, |picker, _| picker.selected())
}

pub(super) fn empty_text(opened: &mut Opened) -> Option<String> {
    opened.picker.read_with(&opened.visual, |picker, cx| {
        picker.delegate.empty_text(cx).map(String::from)
    })
}

pub(super) fn footer(opened: &mut Opened) -> Option<String> {
    opened.picker.read_with(&opened.visual, |picker, cx| {
        picker.delegate.footer(cx).map(String::from)
    })
}

pub(super) fn scans(opened: &mut Opened) -> usize {
    opened.picker.read_with(&opened.visual, |picker, _| {
        picker.delegate.runs.begun().len()
    })
}

/// Types `keys` and lets a debounced search answer.
pub(super) fn search(opened: &mut Opened, keys: &str) {
    opened.visual.simulate_keystrokes(keys);
    advance(opened, SEARCH_DEBOUNCE);
}

pub(super) fn advance(opened: &mut Opened, by: std::time::Duration) {
    opened.visual.executor().advance_clock(by);
    opened.visual.run_until_parked();
}

#[gpui::test]
fn typing_lists_a_header_then_the_hits_of_each_note(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("a.md", "plan one\nother\n  planning two"),
        ("f/b.md", "x\nthe plan"),
        ("c.md", "nothing here"),
    ]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    assert_eq!(
        listed(&mut opened),
        [
            "# b f(1)",
            "2: the plan",
            "# a (2)",
            "1: plan one",
            "3: planning two"
        ],
        "vault order: folders first"
    );
    assert_eq!(
        selected(&mut opened),
        Some(1),
        "the first hit, not its header"
    );
    assert_eq!(footer(&mut opened), None);
}

#[gpui::test]
fn any_core_count_lists_the_same_rows(cx: &mut TestAppContext) {
    let texts: Vec<(String, String)> = (0..23)
        .map(|n| {
            (
                format!("dir {}/note {n}.md", n % 4),
                format!("line\n{}note {n}\n", "x\n".repeat(n)),
            )
        })
        .collect();
    let texts: Vec<(&str, &str)> = texts
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    let fs = with_texts(&texts);
    let mut lists = Vec::new();
    for cpus in [1, 3, 7] {
        cx.executor().set_num_cpus(cpus);
        let mut opened = open(cx, &fs);
        search(&mut opened, "n o t e");
        lists.push(listed(&mut opened));
    }
    assert_eq!(lists[0].len(), 46);
    assert_eq!(lists[0], lists[1]);
    assert_eq!(lists[0], lists[2]);
}

#[test]
fn the_search_line_names_both_timings_and_the_reads() {
    assert_eq!(
        search_line(Duration::from_millis(1), Duration::from_millis(2), 0),
        "search: first hits 1ms, landed 2ms, read 0"
    );
}
