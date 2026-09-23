use gpui::{FontStyle, FontWeight, Hsla, TextStyle};
use inkra_markdown::Styles;

use super::{
    BODY_LINE_HEIGHT, BODY_TEXT_SIZE, CODE_FONT_FAMILY, code_text, heading_metrics, link_text,
    markdown_style, marker_text, quote_text,
};

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

#[test]
fn heading_sizes_step_down_to_the_body_and_never_below() {
    let sizes: Vec<_> = (1..=6).map(|level| heading_metrics(level).size).collect();
    assert!(sizes.windows(2).all(|pair| pair[0] >= pair[1]));
    assert!(sizes[..4].windows(2).all(|pair| pair[0] > pair[1]));
    assert!(sizes.iter().all(|&size| size >= BODY_TEXT_SIZE));
    let h6 = heading_metrics(6);
    assert_eq!(
        (h6.size, h6.line_height),
        (BODY_TEXT_SIZE, BODY_LINE_HEIGHT)
    );
}

#[test]
fn a_heading_line_is_never_shorter_than_body_or_its_own_text() {
    for level in 1..=6 {
        let metrics = heading_metrics(level);
        assert!(metrics.line_height >= metrics.size && metrics.line_height >= BODY_LINE_HEIGHT);
    }
}
