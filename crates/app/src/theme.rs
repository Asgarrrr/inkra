#![allow(clippy::unreadable_literal)]

use gpui::{
    FontStyle, FontWeight, Pixels, Point, Rgba, StrikethroughStyle, TextStyle, point, px, rgb, rgba,
};
use inkra_markdown::{HeadingLevel, Styles};

pub(crate) const BODY_TEXT_SIZE: Pixels = px(18.);
pub(crate) const BODY_LINE_HEIGHT: Pixels = px(28.);

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeadingMetrics {
    pub(crate) size: Pixels,
    pub(crate) line_height: Pixels,
}

/// Zed's preview scale over our body (`markdown.rs` `apply_heading_style`); H5 and H6 floor at the body size.
pub(crate) fn heading_metrics(level: HeadingLevel) -> HeadingMetrics {
    let (size, line_height) = match level {
        HeadingLevel::H1 => (px(34.), px(44.)),
        HeadingLevel::H2 => (px(27.), px(36.)),
        HeadingLevel::H3 => (px(22.), px(30.)),
        HeadingLevel::H4 => (px(20.), px(28.)),
        HeadingLevel::H5 | HeadingLevel::H6 => (BODY_TEXT_SIZE, BODY_LINE_HEIGHT),
    };
    HeadingMetrics { size, line_height }
}

pub(crate) const BODY_COLUMN_CHARS: f32 = 70.;

/// Ships with macOS, the only platform today; a second platform makes this per-platform.
pub(crate) const CODE_FONT_FAMILY: &str = "Menlo";

/// Chrome: the sidebar today, a status line or tab bar later. Stated, not
/// inherited — gpui's default line box (`phi()` on 16px) is taller than a 22px row.
pub(crate) const UI_TEXT_SIZE: Pixels = px(14.);
/// One line of chrome text, and therefore exactly one picker or backlink row: a
/// row that is its own line box cannot clip it.
pub(crate) const UI_LINE_HEIGHT: Pixels = px(22.);

/// A digit of `UI_TEXT_SIZE`, at least: a number column is this per digit, as Zed's rems per digit.
pub(crate) fn ui_digit_width() -> Pixels {
    UI_TEXT_SIZE * 0.6
}

pub(crate) fn editor_background() -> Rgba {
    rgb(0x131212)
}

pub(crate) fn editor_text() -> Rgba {
    rgb(0xffffff)
}

pub(crate) fn sidebar_background() -> Rgba {
    rgb(0x2a2929)
}

pub(crate) fn sidebar_text() -> Rgba {
    rgb(0xd7d6ce)
}

/// A selected tree row's name (the mock's `.row.on`).
pub(crate) fn selected_row_text() -> Rgba {
    rgb(0xf0efe8)
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

/// A white overlay, so it reads on any surface: #2d2d2d on the sidebar, #39393a on a modal.
pub(crate) fn selected_row_background() -> Rgba {
    rgba(0xffffff17)
}

/// A white overlay, fainter than selection on any surface: #232323 on the sidebar, #2f2f30 on a modal.
pub(crate) fn row_hover_background() -> Rgba {
    rgba(0xffffff0c)
}

/// A white overlay stronger than a row's hover, so an icon button reads as a target within a hovered row.
pub(crate) fn button_hover_background() -> Rgba {
    rgba(0xffffff1f)
}

pub(crate) fn icon_muted() -> Rgba {
    rgb(0x8f8d88)
}

/// A toggled icon button's icon: the panel it names is shown.
pub(crate) fn icon_active() -> Rgba {
    rgb(0xd7d6ce)
}

pub(crate) const ICON_SIZE: Pixels = px(14.);

/// A note edited but not written, on its tab and its tree row.
pub(crate) fn dirty_dot() -> Rgba {
    rgb(0x8a8a8a)
}

/// A note whose last write failed, on its tab and its tree row.
pub(crate) fn failed_dot() -> Rgba {
    rgb(0xd16969)
}

pub(crate) const DOT_SIZE: Pixels = px(6.);

/// A hovered or selected tree row on chrome (D1).
pub(crate) fn chrome_row_background() -> Rgba {
    rgb(0x383737)
}

/// A list item's left and right padding.
pub(crate) const ROW_INSET: Pixels = px(8.);

/// A tree row's height, and the pitch `Sidebar::row_center` reads.
pub(crate) const TREE_ROW_HEIGHT: Pixels = px(27.);

pub(crate) const ROW_RADIUS: Pixels = px(6.);

/// The reference's gap above the tree's first row.
pub(crate) const TREE_TOP_GAP: Pixels = px(4.);

/// One level of the tree.
pub(crate) const INDENT_STEP: Pixels = px(12.);

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

/// The one line reporting a failure (quit-and-failures spec).
pub(crate) fn notice_background() -> Rgba {
    rgb(0x3b2424)
}

pub(crate) fn notice_text() -> Rgba {
    rgb(0xe8a0a0)
}

/// The tab bar over the note column (tabs spec, A5): the sidebar's shade, the active tab the editor's.
pub(crate) fn tab_bar_background() -> Rgba {
    rgb(0x2a2929)
}

pub(crate) fn tab_active_background() -> Rgba {
    rgb(0x131212)
}

pub(crate) fn tab_text() -> Rgba {
    rgb(0xd7d6ce)
}

pub(crate) fn tab_inactive_text() -> Rgba {
    rgb(0x8f8d88)
}

pub(crate) fn tab_rule() -> Rgba {
    rgb(0x353332)
}

/// The backlinks count on its pill: the muted text (D6).
pub(crate) fn header_count_text() -> Rgba {
    rgb(0x8f8d88)
}

/// The mock's backlinks header; the nav row is level with it (D4).
pub(crate) const PANEL_HEADER_HEIGHT: Pixels = px(34.);

pub(crate) const HEADER_COUNT_TEXT_SIZE: Pixels = px(11.5);

pub(crate) const HEADER_COUNT_LINE_HEIGHT: Pixels = px(16.);

/// The reference's title row.
pub(crate) const TITLE_BAR_HEIGHT: Pixels = px(39.);

/// The traffic lights' width and margins, on the macOS 26 SDK (Zed's `TRAFFIC_LIGHT_PADDING`).
pub(crate) const TRAFFIC_LIGHT_PADDING: Pixels = px(78.);

/// `(TITLE_BAR_HEIGHT - 16) / 2`: gpui centres the 16 px buttons in a `16 + 2y` container.
pub(crate) const TRAFFIC_LIGHT_POSITION: Point<Pixels> = point(px(9.), px(11.5));

/// An icon button's hit area, the reference's 28 x 26 (Polish rules: hit areas).
pub(crate) const ICON_BUTTON_WIDTH: Pixels = px(28.);

pub(crate) const ICON_BUTTON_HEIGHT: Pixels = px(26.);

pub(crate) const BUTTON_RADIUS: Pixels = px(6.);

/// The empty title row after the tabs never shrinks below this, so the window can always move.
pub(crate) const TITLE_DRAG_MIN_WIDTH: Pixels = px(32.);

/// A long note name truncates rather than crowd out the other tabs.
pub(crate) const TAB_MAX_WIDTH: Pixels = px(200.);

/// Inside the 39 px row, bottom-aligned (D3).
pub(crate) const TAB_HEIGHT: Pixels = px(34.);

/// Before each tab and before `+`.
pub(crate) const TAB_GAP: Pixels = px(6.);

pub(crate) const TAB_RADIUS: Pixels = px(8.);

/// `TAB_RADIUS - (TAB_HEIGHT - ICON_BUTTON_HEIGHT) / 2`: concentric with the tab.
pub(crate) const TAB_CLOSE_RADIUS: Pixels = px(4.);

/// `ICON_BUTTON_WIDTH`, the ×'s box: a tab's centre lies left of its × by the gap.
pub(crate) const TAB_MIN_WIDTH: Pixels = px(28.);

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

/// One per quote level, down the gutter (markdown blocks spec, D3).
pub(crate) fn quote_bar() -> Rgba {
    rgb(0x4a4a4a)
}

pub(crate) const QUOTE_BAR_WIDTH: Pixels = px(2.);

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
