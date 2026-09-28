use gpui::TestAppContext;

use super::{footer, labels, listed, open, search, with_texts};
use crate::vault_search::rows::Row;

#[gpui::test]
fn each_row_label_reads_as_its_row(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan\nx\tplan b"), ("b.md", "plan")]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    assert_eq!(
        labels(&mut opened),
        [
            "a, 2 matches",
            "line 1: plan",
            "line 2: x plan b",
            "b, 1 match",
            "line 1: plan"
        ]
    );
}

#[gpui::test]
fn a_header_splits_its_path_like_quick_open(cx: &mut TestAppContext) {
    let fs = with_texts(&[("work/a\nb.md", "plan")]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    let split = opened.picker.read_with(&opened.visual, |picker, _| {
        match &picker.delegate.results.rows[0] {
            Row::Header(header) => Some((
                header.path.name.text().to_string(),
                header.path.folder.text().to_string(),
            )),
            Row::Hit(_) => None,
        }
    });
    assert_eq!(split, Some(("a b".to_owned(), "work".to_owned())));
}

#[gpui::test]
fn the_hit_is_lit_in_its_context(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "\t  we plan it")]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    let lit = opened.picker.read_with(&opened.visual, |picker, _| {
        match &picker.delegate.results.rows[1] {
            Row::Hit(hit) => hit.context.text()[hit.context.ranges()[0].clone()].to_owned(),
            Row::Header(_) => String::new(),
        }
    });
    assert_eq!(lit, "plan", "still on the word once the indent is cut");
}

#[gpui::test]
fn a_note_past_the_per_note_cap_shows_n_plus(cx: &mut TestAppContext) {
    let fs = with_texts(&[
        ("a.md", &"plan\n".repeat(150)),
        ("b.md", &"plan\n".repeat(100)),
    ]);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    let listed = listed(&mut opened);
    assert_eq!(listed.len(), 202);
    assert_eq!(listed[0], "# a (100+)");
    assert_eq!(listed[101], "# b (100)", "exactly 100 is not cut");
    assert_eq!(labels(&mut opened)[0], "a, 100+ matches");
}

#[gpui::test]
fn past_the_limit_the_footer_says_so(cx: &mut TestAppContext) {
    let texts: Vec<(String, String)> = (0..11)
        .map(|n| (format!("n{n:02}.md"), "plan\n".repeat(100)))
        .collect();
    let texts: Vec<(&str, &str)> = texts
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    let fs = with_texts(&texts);
    let mut opened = open(cx, &fs);

    search(&mut opened, "p l a n");

    let hits = listed(&mut opened)
        .iter()
        .filter(|row| !row.starts_with('#'))
        .count();
    assert_eq!(hits, 1_000);
    assert_eq!(
        footer(&mut opened).as_deref(),
        Some("1,000+ results; refine the query")
    );
}
