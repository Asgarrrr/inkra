use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use gpui::{HighlightStyle, Hsla};
use inkra_index::{NoteContent, NoteIndex, NoteQuery, NoteSet, excerpt, find_notes, merge_notes};
use inkra_vault::{EntryId, Kind, Mtime};

use super::RowText;
use crate::quick_open::list::Row;
use crate::theme;

/// `NoteMatch` has a private rank, so the row tests take it from the public matcher.
fn rows(path: &str, text: &str, query: &str) -> Vec<Row> {
    // Any id: the row reads only the match's path, its highlights and the excerpt.
    let id = EntryId::DETACHED;
    let mtime = Mtime(SystemTime::UNIX_EPOCH);
    let mut index = NoteIndex::default();
    let _ = index.set_notes(NoteSet::build(vec![(
        id,
        format!("{path}.md"),
        Kind::Note,
        mtime,
    )]));
    index.insert(id, NoteContent::parse(text.into(), mtime));
    let snapshot = index.snapshot();
    let query = NoteQuery::new(query, None, &[]);
    let cancel = AtomicBool::new(false);
    let Some(lookup) = snapshot.lookup(&query, &cancel) else {
        return Vec::new();
    };
    let kept = find_notes(
        &snapshot,
        &lookup,
        0..index.names().len(),
        &query,
        &cancel,
        1,
    );
    merge_notes(&snapshot, vec![kept], &query, 1)
        .into_iter()
        .map(|found| {
            let excerpt = excerpt(&snapshot, &found, &query, &cancel);
            Row::new(found, excerpt)
        })
        .collect()
}

fn ranges(highlights: &[(Range<usize>, HighlightStyle)]) -> Vec<Range<usize>> {
    highlights.iter().map(|(range, _)| range.clone()).collect()
}

#[test]
fn a_name_highlight_splits_into_stem_and_folder() {
    let found = rows("projects/2026/plan", "", "p 2 plan");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    assert_eq!(text.folder.as_ref(), "projects/2026");
    assert_eq!(text.stem.as_ref(), "plan");
    let color = Some(Hsla::from(theme::match_text()));
    assert!(
        text.stem_highlights
            .iter()
            .chain(&text.folder_highlights)
            .all(|(_, style)| style.color == color)
    );
    // `p` lights the start of `projects` and of `plan`, which `plan` lights whole.
    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 4 }]);
    assert_eq!(ranges(&text.folder_highlights), [0..1, 9..10]);
}

#[test]
fn a_highlight_across_the_last_slash_lights_the_stem_only() {
    // `/` and U+0301 form one grapheme, so the highlight of `\u{301}b` starts on the `/`.
    let found = rows("a/\u{301}b", "", "b");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].found.name_highlights, [Range { start: 1, end: 5 }]);

    let text = &found[0].text;

    assert_eq!(text.stem.as_ref(), "\u{301}b");
    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 3 }]);
    assert_eq!(text.folder.as_ref(), "a");
    assert_eq!(text.folder_highlights.len(), 0);
}

#[test]
fn a_highlight_ending_at_the_stem_lights_the_folder_only() {
    let found = rows("ab/plan", "", "ab");
    assert_eq!(found.len(), 1);
    let mut found = found[0].found.clone();
    // Through the last `/`, so it ends where the stem starts.
    found.name_highlights = vec![Range { start: 0, end: 3 }];

    let text = RowText::name(&found);

    assert_eq!(text.stem_highlights.len(), 0);
    assert_eq!(
        ranges(&text.folder_highlights),
        [Range { start: 0, end: 2 }]
    );
}

#[test]
fn a_note_at_the_root_has_no_folder() {
    let found = rows("plan", "", "plan");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    assert_eq!(text.folder.as_ref(), "");
    assert_eq!(text.stem.as_ref(), "plan");
}

#[test]
fn a_highlight_covers_the_whole_grapheme_it_lands_in() {
    // The word `\u{301}x` starts inside the grapheme `🎉\u{301}`; lighting part of it splits its glyph.
    let found = rows("\u{1f389}\u{301}x", "", "x");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 7 }]);
}

#[test]
fn a_match_on_a_decomposed_char_covers_its_mark() {
    // No precomposed form: `q` + U+0307 stays two chars under NFC.
    let found = rows("q\u{307}uiz", "", "q");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 3 }]);
}

#[test]
fn a_newline_in_a_file_name_is_a_space_that_keeps_the_highlights() {
    let found = rows("plan\nb", "", "b");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    assert_eq!(text.stem.as_ref(), "plan b");
    assert_eq!(ranges(&text.stem_highlights), [Range { start: 5, end: 6 }]);
}

#[test]
fn an_excerpt_highlight_is_kept() {
    let found = rows("travel", "Notes\nWe plan\tthe trip.", "trip");
    assert_eq!(found.len(), 1);

    let text = &found[0].text;

    // The tab is a space, one byte for one, so the range still lands on `trip`.
    assert_eq!(text.excerpt.as_ref(), "We plan the trip.");
    assert_eq!(
        ranges(&text.excerpt_highlights),
        [Range { start: 12, end: 16 }]
    );
    let color = Some(Hsla::from(theme::match_text()));
    assert!(
        text.excerpt_highlights
            .iter()
            .all(|(_, style)| style.color == color)
    );
}
