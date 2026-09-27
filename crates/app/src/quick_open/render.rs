use std::ops::Range;

use gpui::{
    AnyElement, HighlightStyle, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, StyledText, div,
};
use inkra_index::{Excerpt, NoteMatch};

use super::list::Row;
use crate::one_line::one_line;
use crate::theme;

#[cfg(test)]
mod tests;

#[cfg(test)]
thread_local! {
    /// `RowText` builds and excerpt rebuilds on this thread, so a test can see a draw build none.
    pub(super) static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Rows drawn on this thread, so a test that counts builds knows a draw happened.
    pub(super) static DRAWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Built on landing, never per frame; cut from the match's NFC path, which `name_highlights` index.
pub(super) struct RowText {
    pub(super) stem: SharedString,
    pub(super) stem_highlights: Vec<(Range<usize>, HighlightStyle)>,
    pub(super) folder: SharedString,
    pub(super) folder_highlights: Vec<(Range<usize>, HighlightStyle)>,
    /// Empty when the note has no excerpt, so every row keeps its second line.
    pub(super) excerpt: SharedString,
    pub(super) excerpt_highlights: Vec<(Range<usize>, HighlightStyle)>,
    /// `path: excerpt`, both on one line; the path alone until the excerpt lands.
    pub(super) label: SharedString,
}

fn match_highlight() -> HighlightStyle {
    HighlightStyle {
        color: Some(theme::match_text().into()),
        ..HighlightStyle::default()
    }
}

impl RowText {
    pub(super) fn new(found: &NoteMatch, excerpt: Option<&Excerpt>) -> Self {
        let mut text = Self::name(found);
        text.set_excerpt(&found.path, excerpt);
        text
    }

    pub(super) fn set_excerpt(&mut self, path: &str, excerpt: Option<&Excerpt>) {
        #[cfg(test)]
        BUILDS.with(|builds| builds.set(builds.get() + 1));
        let (line, highlights) = excerpt.map_or_else(
            || (SharedString::default(), Vec::new()),
            |excerpt| {
                let lit = excerpt
                    .highlights
                    .iter()
                    .map(|range| (range.clone(), match_highlight()))
                    .collect();
                (one_line(&excerpt.text).into_owned().into(), lit)
            },
        );
        self.label = if line.is_empty() {
            one_line(path).into_owned().into()
        } else {
            format!("{}: {line}", one_line(path)).into()
        };
        self.excerpt = line;
        self.excerpt_highlights = highlights;
    }

    fn name(found: &NoteMatch) -> Self {
        let path = &*found.path;
        let stem_start = path.rfind('/').map_or(0, |slash| slash + 1);
        // The folder drops its last `/`, and a match on it.
        let folder_end = stem_start.saturating_sub(1);
        let mut stem_highlights = Vec::new();
        let mut folder_highlights = Vec::new();
        // A grapheme across the last `/` gives one range to both halves: each keeps its own part.
        for Range { start, end } in found.name_highlights.iter().cloned() {
            if start < folder_end {
                folder_highlights.push((start..end.min(folder_end), match_highlight()));
            }
            if end > stem_start {
                stem_highlights.push((
                    start.max(stem_start) - stem_start..end - stem_start,
                    match_highlight(),
                ));
            }
        }
        Self {
            stem: one_line(&path[stem_start..]).into_owned().into(),
            stem_highlights,
            folder: one_line(&path[..folder_end]).into_owned().into(),
            folder_highlights,
            excerpt: SharedString::default(),
            excerpt_highlights: Vec::new(),
            label: SharedString::default(),
        }
    }
}

/// Clones the stored text's `SharedString`s and highlights; builds nothing.
pub(super) fn render_row(row: &Row) -> AnyElement {
    #[cfg(test)]
    DRAWS.with(|draws| draws.set(draws.get() + 1));
    let text = &row.text;
    let name = div()
        .flex()
        .flex_row()
        .gap_2()
        .w_full()
        .overflow_hidden()
        .child(
            div()
                .debug_selector(|| "quick-open-stem".into())
                // Shrinks only once the folder, whose flex basis is 0, has none left to give.
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(
                    StyledText::new(text.stem.clone())
                        .with_highlights(text.stem_highlights.iter().cloned()),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                // Cut from the start, so the nearest folder stays visible.
                .text_ellipsis_start()
                .text_color(theme::placeholder_text())
                .child(
                    StyledText::new(text.folder.clone())
                        .with_highlights(text.folder_highlights.iter().cloned()),
                ),
        );
    // An empty text still lays out one line, so a row without an excerpt keeps its height.
    let excerpt = div()
        .w_full()
        .overflow_hidden()
        .text_ellipsis()
        .text_color(theme::placeholder_text())
        .child(
            StyledText::new(text.excerpt.clone())
                .with_highlights(text.excerpt_highlights.iter().cloned()),
        );
    div()
        .flex()
        .flex_col()
        .w_full()
        .whitespace_nowrap()
        .child(name)
        .child(excerpt)
        .into_any_element()
}
