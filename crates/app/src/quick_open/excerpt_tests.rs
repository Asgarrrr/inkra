//! `⌘P` excerpts read their text through the readers, never from the index.

use std::path::Path;
use std::sync::Arc;

use gpui::TestAppContext;
use inkra_vault::mem_fs::MemFs;

use super::tests::{Opened, open, search, with_texts};

fn plans(count: usize) -> (Arc<MemFs>, Vec<String>) {
    let files: Vec<String> = (0..count).map(|n| format!("plan {n:03}")).collect();
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|name| (format!("{name}.md"), format!("Indexed {name}.")))
        .collect();
    let texts: Vec<(&str, &str)> = texts
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    (with_texts(&texts), files)
}

fn loads(fs: &MemFs, files: &[String]) -> usize {
    let loads = files.iter().map(|name| {
        let path = format!("/vault/{name}.md");
        fs.loads(Path::new(&path))
    });
    loads.sum()
}

fn second_lines(opened: &mut Opened) -> Vec<Option<String>> {
    opened.picker.read_with(&opened.visual, |picker, _| {
        let rows = picker.delegate.list.rows.iter();
        rows.map(|row| row.excerpt.as_ref().map(|excerpt| excerpt.text.clone()))
            .collect()
    })
}

#[gpui::test]
fn reads_at_most_a_hundred(cx: &mut TestAppContext) {
    let (fs, files) = plans(300);
    let mut opened = open(cx, &fs, None, &[]);
    let before = loads(&fs, &files);

    search(&mut opened, "p l a n");

    let read = loads(&fs, &files) - before;
    assert!(read > 0, "the excerpts read the files");
    assert!(read <= 100, "{read} reads for at most 100 rows");
}

#[gpui::test]
fn excerpt_comes_from_the_file(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan.md", "Indexed text.")]);
    let mut opened = open(cx, &fs, None, &[]);
    // The watcher's reload takes the gate, so the index keeps the old text and the excerpt's read does not.
    let _gate = fs.block_load("/vault/plan.md");
    fs.insert_file("/vault/plan.md", "On disk only.");

    search(&mut opened, "p l a n");

    assert_eq!(
        second_lines(&mut opened),
        [Some("On disk only.".to_owned())]
    );
}

#[gpui::test]
fn unloaded_row_shows_name_alone(cx: &mut TestAppContext) {
    let fs = with_texts(&[("plan a.md", "Ship it."), ("plan b.md", "Then rest.")]);
    let _gate = fs.block_load("/vault/plan b.md");
    let mut opened = open(cx, &fs, None, &[]);

    search(&mut opened, "p l a n");

    assert_eq!(
        second_lines(&mut opened),
        [None, Some("Ship it.".to_owned())],
        "plan b, the newest, is not loaded"
    );
    assert_eq!(
        fs.loads(Path::new("/vault/plan b.md")),
        1,
        "only the index's load, still held"
    );
}
