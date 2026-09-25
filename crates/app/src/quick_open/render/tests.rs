use std::ops::Range;
use std::sync::atomic::AtomicBool;

use gpui::{HighlightStyle, Hsla};
use inkra_index::{NameMatch, NameQuery, NoteSet, match_names};
use inkra_vault::{EntryId, Kind};

use super::row_text;
use crate::theme;

/// `NameMatch` has a private rank, so the row tests take it from the public matcher.
fn matches(path: &str, query: &str) -> Vec<NameMatch> {
    // Any id: the row reads only `path` and `positions`.
    let notes = NoteSet::build(vec![(EntryId::DETACHED, format!("{path}.md"), Kind::Note)]);
    let names = notes.names();
    let query = NameQuery::new(query, None, &[]);
    match_names(&names, 0..names.len(), &query, &AtomicBool::new(false), 1)
}

fn ranges(highlights: &[(Range<usize>, HighlightStyle)]) -> Vec<Range<usize>> {
    highlights.iter().map(|(range, _)| range.clone()).collect()
}

#[test]
fn a_row_splits_the_path_into_stem_and_folder_with_their_highlights() {
    let found = matches("projects/2026/plan", "p 2 plan");
    assert_eq!(found.len(), 1);

    let text = row_text(&found[0]);

    assert_eq!(text.folder.as_ref(), "projects/2026");
    assert_eq!(text.stem.as_ref(), "plan");
    let color = Some(Hsla::from(theme::match_text()));
    assert!(
        text.stem_highlights
            .iter()
            .chain(&text.folder_highlights)
            .all(|(_, style)| style.color == color)
    );
    // `p` and `plan` align in the stem, which D2's stem bonuses favour; `2` on the boundary of `2026`.
    assert_eq!(ranges(&text.stem_highlights), [0..1, 1..2, 2..3, 3..4]);
    assert_eq!(text.folder_highlights.len(), 1);
    assert_eq!(text.folder_highlights[0].0, 9..10);
}

#[test]
fn a_note_at_the_root_has_no_folder() {
    let found = matches("plan", "plan");
    assert_eq!(found.len(), 1);

    let text = row_text(&found[0]);

    assert_eq!(text.folder.as_ref(), "");
    assert_eq!(text.stem.as_ref(), "plan");
}

#[test]
fn a_highlight_covers_the_whole_grapheme_it_lands_in() {
    // Lighting only the first char of a ZWJ sequence splits its run, and the glyph with it.
    let found = matches("\u{1f469}\u{200d}\u{1f4bb} dev log", "\u{1f469}");
    assert_eq!(found.len(), 1);

    let text = row_text(&found[0]);

    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 11 }]);
}

#[test]
fn a_match_on_a_decomposed_char_covers_its_mark() {
    // No precomposed form: `q` + U+0307 stays two chars under NFC.
    let found = matches("q\u{307}uiz", "q");
    assert_eq!(found.len(), 1);

    let text = row_text(&found[0]);

    assert_eq!(ranges(&text.stem_highlights), [Range { start: 0, end: 3 }]);
}

#[test]
fn a_newline_in_a_file_name_is_a_space_that_keeps_the_highlights() {
    let found = matches("plan\nb", "b");
    assert_eq!(found.len(), 1);

    let text = row_text(&found[0]);

    assert_eq!(text.stem.as_ref(), "plan b");
    assert_eq!(ranges(&text.stem_highlights), [Range { start: 5, end: 6 }]);
}
