use gpui::{FontStyle, FontWeight, Hsla, TextStyle};
use inkra_markdown::Styles;

use super::{CODE_FONT_FAMILY, code_text, link_text, markdown_style, marker_text, quote_text};

#[test]
fn no_tag_keeps_the_base_style() {
    let base = TextStyle::default();
    assert_eq!(markdown_style(&base, Styles::NONE), base);
}

#[test]
fn strong_is_bold_and_emphasis_is_italic() {
    let base = TextStyle::default();
    assert_eq!(
        markdown_style(&base, Styles::STRONG).font_weight,
        FontWeight::BOLD
    );
    assert_eq!(
        markdown_style(&base, Styles::EMPHASIS).font_style,
        FontStyle::Italic
    );
}

#[test]
fn strikethrough_is_struck() {
    let style = markdown_style(&TextStyle::default(), Styles::STRIKETHROUGH);
    assert!(style.strikethrough.is_some());
}

#[test]
fn a_heading_is_bold_at_the_body_size() {
    let base = TextStyle::default();
    let style = markdown_style(&base, Styles::heading(1));
    assert_eq!(style.font_weight, FontWeight::BOLD);
    assert_eq!(style.font_size, base.font_size);
}

#[test]
fn code_is_monospace() {
    let style = markdown_style(&TextStyle::default(), Styles::CODE);
    assert_eq!(style.font_family.as_ref(), CODE_FONT_FAMILY);
    assert_eq!(style.color, Hsla::from(code_text()));
}

#[test]
fn links_and_quotes_take_their_colours() {
    let base = TextStyle::default();
    assert_eq!(
        markdown_style(&base, Styles::LINK).color,
        Hsla::from(link_text())
    );
    assert_eq!(
        markdown_style(&base, Styles::QUOTE).color,
        Hsla::from(quote_text())
    );
}

#[test]
fn a_marker_is_dimmed_but_keeps_its_weight() {
    let style = markdown_style(&TextStyle::default(), Styles::STRONG | Styles::MARKER);
    assert_eq!(style.color, Hsla::from(marker_text()));
    assert_eq!(style.font_weight, FontWeight::BOLD);
}

#[test]
fn a_marker_inside_a_link_is_dimmed_not_linked() {
    let style = markdown_style(&TextStyle::default(), Styles::LINK | Styles::MARKER);
    assert_eq!(style.color, Hsla::from(marker_text()));
}

#[test]
fn frontmatter_is_dimmed() {
    let style = markdown_style(&TextStyle::default(), Styles::FRONTMATTER);
    assert_eq!(style.color, Hsla::from(marker_text()));
}
