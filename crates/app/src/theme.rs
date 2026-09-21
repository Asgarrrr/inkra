//! Every colour and text size the window draws with, named once.
//!
//! Colours are functions rather than constants because gpui's `rgb` and `rgba`
//! are not `const` (`vendor/zed-reference/crates/gpui/src/color.rs:14`, `:20`).
//! Functions are the better shape regardless: a bare `u32` does not say whether
//! its low byte is blue or alpha, and the two constructors read the same hex
//! differently. Naming each colour takes that choice away from the call site,
//! and with it the three separate `unreadable_literal` allows the inline
//! literals used to need.
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

/// Chrome: the sidebar today, a status line or a tab bar when they land.
///
/// Stated rather than inherited, which is the defect this pair closes. gpui's
/// default is `rems(1.)` on a `rem_size` of 16 px, with a line box of `phi()` —
/// 25.89 px (`window.rs:2032`, `style.rs`, `geometry.rs:3722`). The sidebar drew
/// that inside a 22 px row under `overflow_hidden`, so every row clipped a line
/// box taller than itself.
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

/// Translucent: it paints over the text rather than under it.
pub(crate) fn selection() -> Rgba {
    rgba(0x3311ff55)
}

pub(crate) fn cursor() -> Rgba {
    rgba(0x4ea1ffff)
}
