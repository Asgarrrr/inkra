#![allow(clippy::unreadable_literal)]

use std::sync::{Arc, LazyLock};

use gpui::{rgb, rgba};

use super::{Appearance, Theme, ThemeColors};

/// Flexoki dark, the theme with nothing installed.
static DARK: LazyLock<Arc<Theme>> = LazyLock::new(|| {
    Arc::new(Theme {
        appearance: Appearance::Dark,
        colors: ThemeColors {
            // At 85 % over the window's blur: the desktop's colours show through, never its shapes.
            background: rgba(0x181717d9),
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
            md_code_block: rgb(0xcecdc3),
            md_code_block_background: rgb(0x1c1b1a),
            md_link: rgb(0x4ea1ff),
            md_quote: rgb(0xa8a8a8),
            md_quote_bar: rgb(0x4a4a4a),
            md_marker: rgb(0x6a6a6a),
            text: rgb(0xcecdc3),
            text_selected: rgb(0xf0efe8),
            text_muted: rgb(0x878580),
            text_placeholder: rgb(0x6a6a6a),
            text_highlight: rgb(0xe5c07b),
            error: rgb(0xd16969),
            warning: rgb(0xe0b27a),
        },
    })
});

pub(crate) fn dark() -> &'static Arc<Theme> {
    &DARK
}

/// Flexoki light, paper over ink.
static LIGHT: LazyLock<Arc<Theme>> = LazyLock::new(|| {
    Arc::new(Theme {
        appearance: Appearance::Light,
        colors: ThemeColors {
            background: rgba(0xf2f0e5f2),
            editor_background: rgb(0xfffcf0),
            elevated_background: rgb(0xfffcf0),
            border: rgb(0xdad8ce),
            border_variant: rgb(0xe6e4d9),
            element_hover: rgba(0x0000000a),
            element_selected: rgba(0x00000014),
            chrome_element_hover: rgba(0x100f0f0d),
            ghost_element_hover: rgba(0x100f0f1a),
            accent: rgb(0x205ea6),
            accent_background: rgba(0x205ea626),
            selection: rgba(0x205ea633),
            error_background: rgb(0xffe1d5),
            warning_background: rgb(0xffe7ce),
            md_text: rgb(0x100f0f),
            md_heading: rgb(0x100f0f),
            md_code: rgb(0xad8301),
            md_code_block: rgb(0x403e3c),
            md_code_block_background: rgb(0xf2f0e5),
            md_link: rgb(0x205ea6),
            md_quote: rgb(0x6f6e69),
            md_quote_bar: rgb(0xcecdc3),
            md_marker: rgb(0xb7b5ac),
            text: rgb(0x100f0f),
            text_selected: rgb(0x100f0f),
            text_muted: rgb(0x6f6e69),
            text_placeholder: rgb(0x878580),
            text_highlight: rgb(0xad8301),
            error: rgb(0xaf3029),
            warning: rgb(0xbc5215),
        },
    })
});

pub(crate) fn of(appearance: Appearance) -> &'static Arc<Theme> {
    match appearance {
        Appearance::Light => &LIGHT,
        Appearance::Dark => &DARK,
    }
}

/// Every built-in by the name `settings.json` gives it, in the picker's order.
pub(crate) fn all() -> [(&'static str, &'static Arc<Theme>); 2] {
    [("Flexoki Dark", &*DARK), ("Flexoki Light", &*LIGHT)]
}

pub(crate) fn named(name: &str) -> Option<&'static Arc<Theme>> {
    all()
        .into_iter()
        .find_map(|(builtin, theme)| (builtin == name).then_some(theme))
}
