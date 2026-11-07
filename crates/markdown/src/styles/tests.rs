use super::{HEADING_LEVELS, Styles};
use crate::HeadingLevel;

#[test]
fn a_union_contains_each_part() {
    let styles = Styles::STRONG | Styles::MARKER;
    assert!(styles.contains(Styles::STRONG));
    assert!(styles.contains(Styles::MARKER));
    assert!(!styles.contains(Styles::EMPHASIS));
}

#[test]
fn heading_levels_round_trip() {
    for level in HEADING_LEVELS {
        assert_eq!(Styles::heading(level).heading_level(), Some(level));
    }
    assert_eq!(Styles::STRONG.heading_level(), None);
    assert_eq!(
        (Styles::heading(HeadingLevel::H2) | Styles::MARKER).heading_level(),
        Some(HeadingLevel::H2)
    );
}

#[test]
fn a_heading_level_does_not_read_as_another_tag() {
    for level in HEADING_LEVELS {
        let heading = Styles::heading(level);
        for tag in [
            Styles::STRONG,
            Styles::EMPHASIS,
            Styles::STRIKETHROUGH,
            Styles::CODE,
            Styles::LINK,
            Styles::MARKER,
            Styles::QUOTE,
            Styles::FRONTMATTER,
            Styles::CODE_BLOCK,
        ] {
            assert!(!heading.contains(tag));
        }
    }
}

#[test]
fn only_none_is_empty() {
    assert!(Styles::NONE.is_empty());
    assert!(!Styles::MARKER.is_empty());
    assert!(!Styles::heading(HeadingLevel::H1).is_empty());
}

#[test]
fn or_assign_matches_or() {
    let mut styles = Styles::CODE;
    styles |= Styles::QUOTE;
    assert_eq!(styles, Styles::CODE | Styles::QUOTE);
}

#[test]
fn or_is_idempotent() {
    assert_eq!(Styles::STRONG | Styles::STRONG, Styles::STRONG);
    let both = Styles::STRONG | Styles::MARKER;
    assert_eq!(both | Styles::STRONG, both);
}
