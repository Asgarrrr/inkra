#![allow(clippy::unreadable_literal)]

use std::sync::{Arc, LazyLock};

use gpui::{rgb, rgba};

use super::{Theme, ThemeColors};

/// Flexoki dark, the theme with nothing installed.
static DARK: LazyLock<Arc<Theme>> = LazyLock::new(|| {
    Arc::new(Theme {
        colors: ThemeColors {
            // At 95 % over the window's blur: a hint of the desktop, never its shapes; matched to the reference by eye.
            background: rgba(0x181717f2),
            editor_background: rgb(0x100f0f),
            elevated_background: rgb(0x252526),
            border: rgb(0x3c3c3c),
            border_variant: rgb(0x282726),
            // White overlays, so they read on any surface: #232323 and #2d2d2d on the sidebar.
            element_hover: rgba(0xffffff0c),
            element_selected: rgba(0xffffff17),
            chrome_element_hover: rgba(0xfefcf00d),
            // Stronger than a row's hover, so an icon button reads as a target within a hovered row.
            ghost_element_hover: rgba(0xffffff1f),
            accent: rgb(0x4ea1ff),
            accent_background: rgba(0x4ea1ff26),
            selection: rgba(0x3311ff55),
            error_background: rgb(0x3b2424),
            warning_background: rgb(0x2b2622),
            md_text: rgb(0xffffff),
            md_heading: rgb(0xffffff),
            md_code: rgb(0xe5c07b),
            md_link: rgb(0x4ea1ff),
            md_quote: rgb(0xa8a8a8),
            md_quote_bar: rgb(0x4a4a4a),
            md_marker: rgb(0x6a6a6a),
        },
    })
});

pub(crate) fn dark() -> &'static Arc<Theme> {
    &DARK
}
