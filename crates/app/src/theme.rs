pub(crate) mod builtin;
mod colors;
mod file;
mod markdown;
pub(crate) mod picker;
pub(crate) mod store;

use std::sync::Arc;

use gpui::{App, Global, Pixels, Point, WindowAppearance, point, px};
use inkra_markdown::HeadingLevel;

pub(crate) use colors::ThemeColors;
pub(crate) use markdown::markdown_style;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub(crate) appearance: Appearance,
    pub(crate) colors: ThemeColors,
}

/// The flag beside the theme, so every writer sets both and the system cannot override a named theme.
struct GlobalTheme {
    theme: Arc<Theme>,
    follows_system: bool,
}

impl Global for GlobalTheme {}

/// The applied theme, else the built-in dark one, so a test needs no setup.
pub(crate) fn current(cx: &App) -> &Arc<Theme> {
    cx.try_global::<GlobalTheme>()
        .map_or(builtin::dark(), |global| &global.theme)
}

pub(crate) fn follows_system(cx: &App) -> bool {
    cx.try_global::<GlobalTheme>()
        .is_none_or(|global| global.follows_system)
}

/// Whether a code block wraps at the column rather than scrolling; `settings.json` sets it.
struct CodeBlockWrap(bool);

impl Global for CodeBlockWrap {}

/// Off unless set, so a test needs no setup.
pub(crate) fn code_block_wrap(cx: &App) -> bool {
    cx.try_global::<CodeBlockWrap>().is_some_and(|wrap| wrap.0)
}

/// Redraws every window, and only on a change: the layouts shape differently.
pub(crate) fn set_code_block_wrap(wrap: bool, cx: &mut App) {
    if code_block_wrap(cx) == wrap {
        return;
    }
    cx.set_global(CodeBlockWrap(wrap));
    cx.refresh_windows();
}

/// Redraws every window; an equal theme keeps the current pointer, so its layouts stay cached.
fn set(theme: Arc<Theme>, follows_system: bool, cx: &mut App) {
    let same = **current(cx) == *theme;
    if same && self::follows_system(cx) == follows_system {
        return;
    }
    let theme = if same { current(cx).clone() } else { theme };
    cx.set_global(GlobalTheme {
        theme,
        follows_system,
    });
    if !same {
        cx.refresh_windows();
    }
}

/// A named theme pins the windows' appearance to its own; `None` follows the system's.
pub(crate) fn install(theme: Option<Arc<Theme>>, cx: &mut App) {
    if let Some(theme) = theme {
        cx.set_window_appearance(Some(theme.appearance.into()));
        set(theme, false, cx);
    } else {
        // Unpinned first, or `window_appearance` reads the last pin rather than the system's.
        cx.set_window_appearance(None);
        set(builtin::of(cx.window_appearance().into()).clone(), true, cx);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Appearance {
    Light,
    Dark,
}

impl From<WindowAppearance> for Appearance {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
        }
    }
}

impl From<Appearance> for WindowAppearance {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self::Light,
            Appearance::Dark => Self::Dark,
        }
    }
}

/// The built-in theme of the system's appearance, unless a named theme is installed.
pub(crate) fn follow_system(appearance: WindowAppearance, cx: &mut App) {
    if follows_system(cx) {
        set(builtin::of(appearance.into()).clone(), true, cx);
    }
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

/// A code block's text: smaller than the body, in the body's row.
pub(crate) const CODE_BLOCK_TEXT_SIZE: Pixels = px(16.);

/// How far a code block's background reaches past its text on the left and past the column on the right.
pub(crate) const CODE_BLOCK_OUTSET: Pixels = px(8.);

/// A code block background's corners.
pub(crate) const CODE_BLOCK_RADIUS: Pixels = px(6.);

/// A code block's language label, in the UI font.
pub(crate) const CODE_BLOCK_LABEL_SIZE: Pixels = px(12.);

/// Past a block's widest line, so its last glyph never touches the edge.
pub(crate) const CODE_BLOCK_SCROLL_MARGIN: Pixels = px(16.);

/// A scrollable block's position bar: its thickness.
pub(crate) const CODE_BLOCK_BAR_HEIGHT: Pixels = px(3.);

/// The gap between the bar and the block background's bottom edge.
pub(crate) const CODE_BLOCK_BAR_INSET: Pixels = px(1.);

pub(crate) const CODE_BLOCK_BAR_RADIUS: Pixels = px(1.5);

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

/// The icons' `stroke-width="2.571"` paints 1.5 px at this size, regular text's weight: change both together.
pub(crate) const ICON_SIZE: Pixels = px(14.);

pub(crate) const DOT_SIZE: Pixels = px(6.);

/// A list item's left and right padding.
pub(crate) const ROW_INSET: Pixels = px(8.);

/// A tree row's height, and the pitch `Sidebar::row_center` reads.
pub(crate) const TREE_ROW_HEIGHT: Pixels = px(27.);

pub(crate) const ROW_RADIUS: Pixels = px(8.);

/// The corners of an elevated surface: the menu, the modal, the tooltip.
pub(crate) const ELEVATED_RADIUS: Pixels = px(8.);

/// A menu row's gap to its list's sides; its padding gives the gap back, so its text stays at `ROW_INSET`.
pub(crate) const MENU_INSET: Pixels = px(4.);

/// `ELEVATED_RADIUS - MENU_INSET`, so a row's corners are concentric with its surface's.
pub(crate) const MENU_ROW_RADIUS: Pixels = px(4.);

/// The reference's gap above the tree's first row.
pub(crate) const TREE_TOP_GAP: Pixels = px(4.);

/// One level of the tree.
pub(crate) const INDENT_STEP: Pixels = px(12.);

/// The line between two groups of a picker's rows.
pub(crate) const RULE_WIDTH: Pixels = px(1.);

/// The mock's backlinks header (D4).
pub(crate) const PANEL_HEADER_HEIGHT: Pixels = px(34.);

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

/// A lower group's strip row, a tab tall.
pub(crate) const GROUP_HEADER_HEIGHT: Pixels = TAB_HEIGHT;

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

pub(crate) const DROP_EDGE: Pixels = px(2.);

pub(crate) const QUOTE_BAR_WIDTH: Pixels = px(2.);

#[cfg(test)]
mod tests;
