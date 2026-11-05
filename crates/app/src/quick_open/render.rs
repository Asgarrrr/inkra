use gpui::{AnyElement, IntoElement, ParentElement, SharedString, Styled, div};
use inkra_index::{Excerpt, NoteMatch};

use super::list::Row;
use crate::row_text::{LitText, PathText, name_row, styled};
use crate::theme::Theme;
use crate::ui::Color;

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
    pub(super) path: PathText,
    /// Empty when the note has no excerpt, so every row keeps its second line.
    pub(super) excerpt: LitText,
    /// `path: excerpt`, both on one line; the path alone until the excerpt lands.
    pub(super) label: SharedString,
}

impl RowText {
    pub(super) fn new(found: &NoteMatch, excerpt: Option<&Excerpt>) -> Self {
        let mut text = Self {
            path: PathText::new(&found.path, &found.name_highlights),
            excerpt: LitText::default(),
            label: SharedString::default(),
        };
        text.set_excerpt(excerpt);
        text
    }

    pub(super) fn set_excerpt(&mut self, excerpt: Option<&Excerpt>) {
        #[cfg(test)]
        BUILDS.with(|builds| builds.set(builds.get() + 1));
        self.excerpt = excerpt.map_or_else(LitText::default, |excerpt| {
            LitText::line(&excerpt.text, excerpt.highlights.iter().cloned())
        });
        let path = self.path.label();
        self.label = if self.excerpt.text().is_empty() {
            path
        } else {
            format!("{path}: {}", self.excerpt.text()).into()
        };
    }
}

/// Clones the stored text's `SharedString`s and highlights; builds nothing.
pub(super) fn render_row(row: &Row, theme: &Theme) -> AnyElement {
    #[cfg(test)]
    DRAWS.with(|draws| draws.set(draws.get() + 1));
    let text = &row.text;
    // An empty text still lays out one line, so a row without an excerpt keeps its height.
    let excerpt = div()
        .w_full()
        .overflow_hidden()
        .text_ellipsis()
        .text_color(Color::Placeholder.rgba(theme))
        .child(styled(&text.excerpt, Color::Highlight.rgba(theme).into()));
    div()
        .flex()
        .flex_col()
        .w_full()
        .whitespace_nowrap()
        .child(name_row(&text.path, theme))
        .child(excerpt)
        .into_any_element()
}
