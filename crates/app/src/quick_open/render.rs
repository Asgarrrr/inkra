use std::ops::Range;

use gpui::{
    AnyElement, HighlightStyle, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, StyledText, div,
};
use inkra_index::NameMatch;
use unicode_segmentation::UnicodeSegmentation as _;

use crate::one_line::one_line;
use crate::theme;

#[cfg(test)]
mod tests;

/// A row's two halves, cut from the match's own path: `positions` index it, and it is NFC
/// where the vault's path is NFD on macOS.
pub(super) struct RowText {
    pub(super) stem: SharedString,
    pub(super) stem_highlights: Vec<(Range<usize>, HighlightStyle)>,
    pub(super) folder: SharedString,
    pub(super) folder_highlights: Vec<(Range<usize>, HighlightStyle)>,
}

fn match_highlight() -> HighlightStyle {
    HighlightStyle {
        color: Some(theme::match_text().into()),
        ..HighlightStyle::default()
    }
}

pub(super) fn row_text(found: &NameMatch) -> RowText {
    let path = &*found.path;
    let stem_start = path.rfind('/').map_or(0, |slash| slash + 1);
    // The folder drops its last `/`, and a match on it.
    let folder_end = stem_start.saturating_sub(1);
    let mut stem_highlights = Vec::new();
    let mut folder_highlights = Vec::new();
    // A run that ends inside a grapheme splits its glyph, so each highlight covers the whole one.
    let graphemes: Vec<Range<usize>> = path
        .grapheme_indices(true)
        .map(|(start, grapheme)| start..start + grapheme.len())
        .collect();
    let mut lit: Vec<Range<usize>> = found
        .positions
        .iter()
        .filter_map(|&at| graphemes.iter().find(|grapheme| grapheme.contains(&at)))
        .cloned()
        .collect();
    lit.dedup();
    for Range { start: at, end } in lit {
        if at >= stem_start {
            stem_highlights.push((at - stem_start..end - stem_start, match_highlight()));
        } else if end <= folder_end {
            folder_highlights.push((at..end, match_highlight()));
        }
    }
    RowText {
        stem: one_line(&path[stem_start..]).into_owned().into(),
        stem_highlights,
        folder: one_line(&path[..folder_end]).into_owned().into(),
        folder_highlights,
    }
}

pub(super) fn render_row(found: &NameMatch) -> AnyElement {
    let text = row_text(found);
    div()
        .flex()
        .flex_row()
        .gap_2()
        .w_full()
        .overflow_hidden()
        .whitespace_nowrap()
        .child(
            div()
                .debug_selector(|| "quick-open-stem".into())
                // Shrinks only once the folder, whose flex basis is 0, has none left to give.
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(StyledText::new(text.stem).with_highlights(text.stem_highlights)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                // Cut from the start, so the nearest folder stays visible.
                .text_ellipsis_start()
                .text_color(theme::placeholder_text())
                .child(StyledText::new(text.folder).with_highlights(text.folder_highlights)),
        )
        .into_any_element()
}
