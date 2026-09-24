//! `Markdown::headings`: ranges, levels and opening sequences (live-preview spec, D8 and D12).

use proptest::prelude::*;

use inkra_text::ByteOffset;

use super::super::parse;
use super::syntax_fragment;
use crate::Styles;

proptest! {
    #[test]
    fn headings_are_well_formed_on_any_text(text in any::<String>()) {
        headings_are_well_formed(&text)?;
    }

    #[test]
    fn headings_are_well_formed_on_block_constructs(
        fragments in prop::collection::vec(syntax_fragment(), 0..40)
    ) {
        headings_are_well_formed(&fragments.concat())?;
    }
}

fn headings_are_well_formed(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    let mut previous_end = 0;
    for heading in &markdown.headings {
        let (start, end) = (heading.range.start.0, heading.range.end.0);
        prop_assert!(start >= previous_end && start < end);
        prop_assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        prop_assert!(!text[start..end].ends_with('\n'));
        prop_assert!((1..=6).contains(&heading.level));
        // A setext heading's leading `#` is text, so only an ATX heading starts with a `#` marker.
        let atx = text[start..].starts_with('#')
            && markdown
                .spans
                .iter()
                .any(|span| span.range.start.0 == start && span.styles.contains(Styles::MARKER));
        prop_assert_eq!(heading.marker.is_some(), atx);
        if let Some(marker) = &heading.marker {
            prop_assert!(text[marker.start.0..].starts_with('#'));
            prop_assert_eq!(marker.start.0, start);
            prop_assert!(marker.end.0 <= end);
            prop_assert_eq!(
                markdown
                    .conceals
                    .iter()
                    .filter(|conceal| conceal.marker == *marker)
                    .count(),
                1
            );
            for byte in marker.start.0..marker.end.0 {
                prop_assert!(markdown.spans.iter().any(|span| span.range.start.0 <= byte
                    && byte < span.range.end.0
                    && span.styles.contains(Styles::MARKER)));
            }
        }
        // The app keys a line's size on this: the first byte carries the same level.
        prop_assert!(markdown.spans.iter().any(|span| span.range.start.0 <= start
            && start < span.range.end.0
            && span.styles.heading_level() == Some(heading.level)));
        previous_end = end;
    }
    Ok(())
}

/// Each heading as the text it covers and its level.
fn headings(text: &str) -> Vec<(&str, u8)> {
    parse(text)
        .headings
        .iter()
        .map(|heading| {
            (
                &text[heading.range.start.0..heading.range.end.0],
                heading.level,
            )
        })
        .collect()
}

#[test]
fn an_atx_heading_covers_its_line_without_the_newline() {
    assert_eq!(headings("# T\nx"), vec![("# T", 1)]);
    assert_eq!(headings("## a ##\n"), vec![("## a ##", 2)]);
}

#[test]
fn an_empty_atx_heading_is_still_a_heading() {
    assert_eq!(headings("#"), vec![("#", 1)]);
    assert_eq!(headings("# "), vec![("# ", 1)]);
}

/// Each heading's opening sequence as the text it covers.
fn heading_markers(text: &str) -> Vec<Option<&str>> {
    parse(text)
        .headings
        .iter()
        .map(|heading| {
            heading
                .marker
                .as_ref()
                .map(|marker| &text[marker.start.0..marker.end.0])
        })
        .collect()
}

#[test]
fn an_atx_heading_names_its_opening_sequence() {
    assert_eq!(heading_markers("# T"), vec![Some("# ")]);
    assert_eq!(heading_markers("###### T"), vec![Some("###### ")]);
    assert_eq!(heading_markers("#"), vec![Some("#")]);
    assert_eq!(heading_markers("#\n"), vec![Some("#")]);
    assert_eq!(heading_markers("#\tT"), vec![Some("#\t")]);
    assert_eq!(heading_markers("#   T"), vec![Some("#   ")]);
    assert_eq!(heading_markers("# T ##"), vec![Some("# ")]);
}

#[test]
fn a_setext_heading_has_no_opening_sequence() {
    assert_eq!(heading_markers("T\n==="), vec![None]);
    assert_eq!(heading_markers("#a\n==="), vec![None]);
    assert_eq!(heading_markers("**b**\n==="), vec![None]);
}

#[test]
fn a_heading_inside_a_container_names_its_opening_sequence() {
    let markdown = parse("> # q\n- # a");
    let markers: Vec<_> = markdown
        .headings
        .iter()
        .map(|heading| heading.marker.clone())
        .collect();
    assert_eq!(
        markers,
        vec![
            Some(ByteOffset(2)..ByteOffset(4)),
            Some(ByteOffset(8)..ByteOffset(10))
        ]
    );
}

#[test]
fn a_setext_heading_excludes_its_underline() {
    assert_eq!(headings("T\n==="), vec![("T", 1)]);
    assert_eq!(headings("a\nb\n---\nc"), vec![("a\nb", 2)]);
    assert_eq!(headings("#a\n==="), vec![("#a", 1)]);
}

#[test]
fn a_heading_inside_a_container_starts_at_its_own_syntax() {
    assert_eq!(headings("> # q\n"), vec![("# q", 1)]);
    assert_eq!(headings("- # a\n"), vec![("# a", 1)]);
    assert_eq!(headings("  # ind"), vec![("# ind", 1)]);
    assert_eq!(headings("> a\n> b\n> ==="), vec![("a\n> b", 1)]);
}

#[test]
fn headings_come_in_document_order() {
    assert_eq!(
        headings("# a\n\nb\n\n### c\nd\n==="),
        vec![("# a", 1), ("### c", 3), ("d", 1)]
    );
}
