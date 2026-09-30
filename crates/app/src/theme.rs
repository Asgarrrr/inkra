#![allow(clippy::unreadable_literal)]

use gpui::{FontStyle, FontWeight, Pixels, Rgba, StrikethroughStyle, TextStyle, px, rgb, rgba};
use inkra_markdown::Styles;

pub(crate) const BODY_TEXT_SIZE: Pixels = px(18.);
pub(crate) const BODY_LINE_HEIGHT: Pixels = px(28.);

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeadingMetrics {
    pub(crate) size: Pixels,
    pub(crate) line_height: Pixels,
}

/// Zed's preview scale over our body (`markdown.rs` `apply_heading_style`); H5 and H6 floor at the body size.
pub(crate) fn heading_metrics(level: u8) -> HeadingMetrics {
    let (size, line_height) = match level {
        1 => (px(34.), px(44.)),
        2 => (px(27.), px(36.)),
        3 => (px(22.), px(30.)),
        4 => (px(20.), px(28.)),
        _ => (BODY_TEXT_SIZE, BODY_LINE_HEIGHT),
    };
    HeadingMetrics { size, line_height }
}

pub(crate) const BODY_COLUMN_CHARS: f32 = 70.;

/// Ships with macOS, the only platform today; a second platform makes this per-platform.
pub(crate) const CODE_FONT_FAMILY: &str = "Menlo";

/// Chrome: the sidebar today, a status line or tab bar later. Stated, not
/// inherited — gpui's default line box (`phi()` on 16px) is taller than a 22px row.
pub(crate) const UI_TEXT_SIZE: Pixels = px(14.);
/// One line of chrome text, and therefore exactly one sidebar row: a row that
/// is its own line box cannot clip it.
pub(crate) const UI_LINE_HEIGHT: Pixels = px(22.);

/// A digit of `UI_TEXT_SIZE`, at least: a number column is this per digit, as Zed's rems per digit.
pub(crate) fn ui_digit_width() -> Pixels {
    UI_TEXT_SIZE * 0.6
}

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

pub(crate) fn modal_background() -> Rgba {
    rgb(0x252526)
}

/// The line between two groups of a picker's rows.
pub(crate) const RULE_WIDTH: Pixels = px(1.);

pub(crate) fn group_rule() -> Rgba {
    rgb(0x3c3c3c)
}

pub(crate) fn modal_border() -> Rgba {
    rgb(0x3c3c3c)
}

/// The one line over a note deleted on disk (watcher spec, F9).
pub(crate) fn gone_banner_background() -> Rgba {
    rgb(0x2b2622)
}

pub(crate) fn gone_banner_text() -> Rgba {
    rgb(0xe0b27a)
}

/// The tab bar over the note column (tabs spec, A5): the sidebar's shade, the active tab the editor's.
pub(crate) fn tab_bar_background() -> Rgba {
    rgb(0x181818)
}

pub(crate) fn tab_active_background() -> Rgba {
    rgb(0x1e1e1e)
}

pub(crate) fn tab_text() -> Rgba {
    rgb(0xd0d0d0)
}

pub(crate) fn tab_inactive_text() -> Rgba {
    rgb(0x8a8a8a)
}

pub(crate) fn tab_rule() -> Rgba {
    rgb(0x3c3c3c)
}

/// A long note name truncates rather than crowd out the other tabs.
pub(crate) const TAB_MAX_WIDTH: Pixels = px(200.);

/// The tab a dragged tab is over: the caret's blue, faint behind it and full on the edge it lands at.
pub(crate) fn tab_drop_background() -> Rgba {
    rgba(0x4ea1ff26)
}

pub(crate) fn tab_drop_border() -> Rgba {
    rgb(0x4ea1ff)
}

pub(crate) const TAB_DROP_EDGE: Pixels = px(2.);

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

pub(crate) fn match_text() -> Rgba {
    rgb(0xe5c07b)
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
