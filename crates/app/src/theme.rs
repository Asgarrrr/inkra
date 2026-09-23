#![allow(clippy::unreadable_literal)]

use gpui::{Pixels, Rgba, px, rgb, rgba};

pub(crate) const BODY_TEXT_SIZE: Pixels = px(18.);
pub(crate) const BODY_LINE_HEIGHT: Pixels = px(28.);

pub(crate) const BODY_COLUMN_CHARS: f32 = 70.;

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
