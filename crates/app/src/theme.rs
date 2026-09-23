#![allow(clippy::unreadable_literal)]

use gpui::{FontStyle, FontWeight, Pixels, Rgba, StrikethroughStyle, TextStyle, px, rgb, rgba};
use inkra_markdown::Styles;

pub(crate) const BODY_TEXT_SIZE: Pixels = px(18.);
pub(crate) const BODY_LINE_HEIGHT: Pixels = px(28.);

pub(crate) const BODY_COLUMN_CHARS: f32 = 70.;

/// Ships with macOS, the only platform today; a second platform makes this per-platform.
pub(crate) const CODE_FONT_FAMILY: &str = "Menlo";

/// Chrome: the sidebar today, a status line or tab bar later. Stated, not
/// inherited — gpui's default line box (`phi()` on 16px) is taller than a 22px row.
pub(crate) const UI_TEXT_SIZE: Pixels = px(14.);
/// One line of chrome text, and therefore exactly one sidebar row: a row that
/// is its own line box cannot clip it.
pub(crate) const UI_LINE_HEIGHT: Pixels = px(22.);

pub(crate) fn editor_background() -> Rgba {
    rgb(0x1e1e1e)
}

pub(crate) fn editor_text() -> Rgba {
    rgb(0xffffff)
}

pub(crate) fn sidebar_background() -> Rgba {
    rgb(0x181818)
}

pub(crate) fn sidebar_text() -> Rgba {
    rgb(0xd0d0d0)
}

/// Not reused from `sidebar_text`: that one is calibrated against a different background.
pub(crate) fn placeholder_text() -> Rgba {
    rgb(0x6a6a6a)
}

pub(crate) fn rejected_text() -> Rgba {
    rgb(0xe08a7a)
}

pub(crate) fn selection() -> Rgba {
    rgba(0x3311ff55)
}

pub(crate) fn cursor() -> Rgba {
    rgba(0x4ea1ffff)
}

pub(crate) fn selected_row_background() -> Rgba {
    rgb(0x2d2d2d)
}

pub(crate) fn heading_text() -> Rgba {
    rgb(0xffffff)
}

/// Not reused from `placeholder_text`: a marker sits inside body text, not on its own.
pub(crate) fn marker_text() -> Rgba {
    rgb(0x6a6a6a)
}

pub(crate) fn code_text() -> Rgba {
    rgb(0xe5c07b)
}

pub(crate) fn link_text() -> Rgba {
    rgb(0x4ea1ff)
}

pub(crate) fn quote_text() -> Rgba {
    rgb(0xa8a8a8)
}

/// `Marker` goes last so a marker stays dimmed inside a link or a heading; it
/// keeps weight and style so its width matches the text around it.
pub(crate) fn markdown_style(base: &TextStyle, styles: Styles) -> TextStyle {
    let mut style = base.clone();
    if styles.heading_level().is_some() {
        style.font_weight = FontWeight::BOLD;
        style.color = heading_text().into();
    }
    if styles.contains(Styles::STRONG) {
        style.font_weight = FontWeight::BOLD;
    }
    if styles.contains(Styles::EMPHASIS) {
        style.font_style = FontStyle::Italic;
    }
    if styles.contains(Styles::STRIKETHROUGH) {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        });
    }
    if styles.contains(Styles::CODE) {
        // `HighlightStyle` has no family field, so this cannot go through `TextStyle::highlight`.
        style.font_family = CODE_FONT_FAMILY.into();
        style.color = code_text().into();
    }
    if styles.contains(Styles::LINK) {
        style.color = link_text().into();
    }
    if styles.contains(Styles::QUOTE) {
        style.color = quote_text().into();
    }
    if styles.contains(Styles::FRONTMATTER) || styles.contains(Styles::MARKER) {
        style.color = marker_text().into();
    }
    style
}

#[cfg(test)]
mod tests;
