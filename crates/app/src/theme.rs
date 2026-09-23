//! Every colour and text size the window draws with, named once.
//!
//! Colours are functions rather than constants because gpui's `rgb` and `rgba`
//! are not `const` (`vendor/zed-reference/crates/gpui/src/color.rs:14`, `:20`).
//! Functions are the better shape regardless: a bare `u32` does not say whether
//! its low byte is blue or alpha, and the two constructors read the same hex
//! differently. Naming each colour takes that choice away from the call site.
//!
//! Sizes are `const`: `px` is (`geometry.rs:3748`).
#![allow(clippy::unreadable_literal)]

use gpui::{Pixels, Rgba, px, rgb, rgba};

/// The editor body.
///
/// The chrome scale below is one step down at the same leading ratio — 18/28 is
/// 1.56, 14/22 is 1.57 — so the two are one scale rather than two that happen
/// to coexist.
pub(crate) const BODY_TEXT_SIZE: Pixels = px(18.);
/// See [`BODY_TEXT_SIZE`].
pub(crate) const BODY_LINE_HEIGHT: Pixels = px(28.);

/// The editor's text column, in `ch` — the advance of `0` at the body size,
/// as in CSS. A reading width, not a character count: the body font is
/// proportional, so a row of prose holds somewhat more. The pane wins when it
/// is narrower.
pub(crate) const BODY_COLUMN_CHARS: f32 = 70.;

/// Chrome: the sidebar today, a status line or a tab bar when they land.
///
/// Stated, not inherited (see CLAUDE.md, chrome scale). gpui's default line
/// box is `phi()` on a 16 px `rem_size` — 25.89 px (`window.rs:2032`,
/// `style.rs`, `geometry.rs:3722`) — taller than a 22 px row.
pub(crate) const UI_TEXT_SIZE: Pixels = px(14.);
/// One line of chrome text, and therefore exactly one sidebar row: a row that is
/// its own line box cannot clip it. See [`UI_TEXT_SIZE`].
pub(crate) const UI_LINE_HEIGHT: Pixels = px(22.);

pub(crate) fn editor_background() -> Rgba {
    rgb(0x1e1e1e)
}

pub(crate) fn editor_text() -> Rgba {
    rgb(0xffffff)
}

/// A step darker than the editor, so the two panes read as separate surfaces
/// with no border between them.
pub(crate) fn sidebar_background() -> Rgba {
    rgb(0x181818)
}

/// A step dimmer than the editor's text, for the same reason: chrome sits
/// behind the prose, not beside it.
pub(crate) fn sidebar_text() -> Rgba {
    rgb(0xd0d0d0)
}

/// The empty state's text, on `editor_background`.
///
/// Dimmer than `sidebar_text`, not a reuse of it: that one is calibrated against
/// `sidebar_background`, a different literal.
pub(crate) fn placeholder_text() -> Rgba {
    rgb(0x6a6a6a)
}

/// A refused name's reason, under the field that produced it. Warm rather than
/// dim, unlike every other chrome colour here: it is the one line in the window
/// that has to be read before the user types again.
pub(crate) fn rejected_text() -> Rgba {
    rgb(0xe08a7a)
}

/// Translucent: it paints over the text rather than under it.
pub(crate) fn selection() -> Rgba {
    rgba(0x3311ff55)
}

pub(crate) fn cursor() -> Rgba {
    rgba(0x4ea1ffff)
}

/// Behind the sidebar row a command would act on.
///
/// Lighter than `sidebar_background` rather than an accent colour: the tree's
/// selection says where you are, not what is happening, and a saturated bar
/// would outrank the editor's own caret for attention.
pub(crate) fn selected_row_background() -> Rgba {
    rgb(0x2d2d2d)
}
