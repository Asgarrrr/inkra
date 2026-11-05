pub(crate) mod builtin;
mod colors;
mod markdown;

use std::sync::Arc;

use gpui::{App, Global, Pixels, Point, point, px};
use inkra_markdown::HeadingLevel;

pub(crate) use colors::ThemeColors;
pub(crate) use markdown::markdown_style;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub(crate) colors: ThemeColors,
}

struct GlobalTheme(Arc<Theme>);

impl Global for GlobalTheme {}

/// The applied theme, else the built-in dark one, so a test needs no setup.
pub(crate) fn current(cx: &App) -> &Arc<Theme> {
    match cx.try_global::<GlobalTheme>() {
        Some(GlobalTheme(theme)) => theme,
        None => builtin::dark(),
    }
}

/// Redraws every window; an equal theme changes nothing, so its layouts stay cached.
pub(crate) fn apply(theme: Arc<Theme>, cx: &mut App) {
    if **current(cx) == *theme {
        return;
    }
    cx.set_global(GlobalTheme(theme));
    cx.refresh_windows();
}

/// The blur's stand-in in a shot: the chrome's own colour, so the shot shows it opaque.
#[cfg(feature = "shot")]
pub(crate) fn shot_backdrop(theme: &Theme) -> gpui::Rgba {
    gpui::Rgba {
        a: 1.0,
        ..theme.colors.background
    }
}

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

pub(crate) const ICON_SIZE: Pixels = px(14.);

pub(crate) const DOT_SIZE: Pixels = px(6.);

/// A list item's left and right padding.
pub(crate) const ROW_INSET: Pixels = px(8.);

/// A tree row's height, and the pitch `Sidebar::row_center` reads.
pub(crate) const TREE_ROW_HEIGHT: Pixels = px(27.);

pub(crate) const ROW_RADIUS: Pixels = px(8.);

/// The reference's gap above the tree's first row.
pub(crate) const TREE_TOP_GAP: Pixels = px(4.);

/// One level of the tree.
pub(crate) const INDENT_STEP: Pixels = px(12.);

/// The line between two groups of a picker's rows.
pub(crate) const RULE_WIDTH: Pixels = px(1.);

/// The mock's backlinks header; the nav row is level with it (D4).
pub(crate) const PANEL_HEADER_HEIGHT: Pixels = px(34.);

/// Level with the backlinks header (D4).
pub(crate) const NAV_ROW_HEIGHT: Pixels = PANEL_HEADER_HEIGHT;

pub(crate) const HEADER_COUNT_TEXT_SIZE: Pixels = px(11.5);

pub(crate) const HEADER_COUNT_LINE_HEIGHT: Pixels = px(16.);

/// The reference's title row.
pub(crate) const TITLE_BAR_HEIGHT: Pixels = px(40.);

/// The reference's inset before the sidebar's first button: the lights and their margins.
pub(crate) const TRAFFIC_LIGHT_PADDING: Pixels = px(88.);

/// `(TITLE_BAR_HEIGHT - 16) / 2`: gpui centres the 16 px buttons in a `16 + 2y` container.
pub(crate) const TRAFFIC_LIGHT_POSITION: Point<Pixels> = point(px(14.), px(12.));

/// An icon button's hit area, the reference's 28 x 26 (Polish rules: hit areas).
pub(crate) const ICON_BUTTON_WIDTH: Pixels = px(28.);

pub(crate) const ICON_BUTTON_HEIGHT: Pixels = px(26.);

pub(crate) const BUTTON_RADIUS: Pixels = px(8.);

/// A dimmed, inert button (the mock's `.off`).
pub(crate) const DISABLED_OPACITY: f32 = 0.45;

/// The empty title row after the tabs never shrinks below this, so the window can always move.
pub(crate) const TITLE_DRAG_MIN_WIDTH: Pixels = px(32.);

/// A long note name truncates rather than crowd out the other tabs.
pub(crate) const TAB_MAX_WIDTH: Pixels = px(200.);

/// Inside the 40 px row, bottom-aligned (D3).
pub(crate) const TAB_HEIGHT: Pixels = px(34.);

/// Between two tabs, and before `+`.
pub(crate) const TAB_GAP: Pixels = px(6.);

pub(crate) const TAB_RADIUS: Pixels = px(10.);

/// The active tab's concave feet at its bottom corners, the reference's `--tab-curve`.
pub(crate) const TAB_CURVE: Pixels = px(10.);

/// An inactive tab's hover pill, inset from the tab's sides and bottom.
pub(crate) const TAB_HOVER_INSET: Pixels = px(4.);

/// The reference's `--tab-radius`, on every corner of the hover pill.
pub(crate) const TAB_HOVER_RADIUS: Pixels = px(8.);

/// The title's inset from the tab's left edge: the hover pill's inset plus the reference's 6 px inner padding.
pub(crate) const TAB_TEXT_INSET: Pixels = px(10.);

/// The ×'s box on a tab, the reference's 20 px: a full-size button would fill the hover pill.
pub(crate) const TAB_CLOSE_SIZE: Pixels = px(20.);

/// Concentric with the hover pill: its radius less the ×'s inset in it.
pub(crate) const TAB_CLOSE_RADIUS: Pixels = px(3.5);

/// Wider than the ×'s box: a tab's centre lies left of its × by the gap.
pub(crate) const TAB_MIN_WIDTH: Pixels = px(28.);

pub(crate) const TAB_DROP_EDGE: Pixels = px(2.);

pub(crate) const QUOTE_BAR_WIDTH: Pixels = px(2.);

#[cfg(test)]
mod tests;
