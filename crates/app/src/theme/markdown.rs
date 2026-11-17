use gpui::{FontStyle, FontWeight, StrikethroughStyle, TextStyle, px};
use inkra_markdown::Styles;

use super::{CODE_FONT_FAMILY, Theme};

/// `Marker` goes last so a marker stays dimmed inside a link or a heading; it
/// keeps weight and style so its width matches the text around it. A checked
/// task's text is struck and dimmed, its markers only dimmed (task boxes spec, D7).
pub(crate) fn markdown_style(base: &TextStyle, styles: Styles, theme: &Theme) -> TextStyle {
    let colors = &theme.colors;
    let mut style = base.clone();
    if styles.heading_level().is_some() {
        style.font_weight = FontWeight::BOLD;
        style.color = colors.md_heading.into();
    }
    if styles.contains(Styles::STRONG) {
        style.font_weight = FontWeight::BOLD;
    }
    if styles.contains(Styles::EMPHASIS) {
        style.font_style = FontStyle::Italic;
    }
    let done = styles.contains(Styles::DONE) && !styles.contains(Styles::MARKER);
    if styles.contains(Styles::STRIKETHROUGH) || done {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        });
    }
    if styles.contains(Styles::CODE) {
        // `HighlightStyle` has no family field, so this cannot go through `TextStyle::highlight`.
        style.font_family = CODE_FONT_FAMILY.into();
        style.color = colors.md_code.into();
    }
    if styles.contains(Styles::LINK) {
        style.color = colors.md_link.into();
    }
    if styles.contains(Styles::QUOTE) {
        style.color = colors.md_quote.into();
    }
    if styles.contains(Styles::CODE_BLOCK) {
        style.color = colors.md_code_block.into();
    }
    if done || styles.contains(Styles::FRONTMATTER) || styles.contains(Styles::MARKER) {
        style.color = colors.md_marker.into();
    }
    style
}
