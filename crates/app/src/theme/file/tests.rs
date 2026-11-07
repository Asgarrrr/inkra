use gpui::{Rgba, rgba};
use proptest::prelude::*;

use super::{describe, parse, parse_color};
use crate::theme::{Appearance, ThemeColors, builtin};

#[test]
fn every_token_name_round_trips_through_set_alone() {
    assert_eq!(ThemeColors::NAMES.len(), 30);
    let base = builtin::dark().colors;
    let marked = rgba(0x01_02_03_04);
    for name in ThemeColors::NAMES {
        let mut colors = base;
        assert!(colors.set(name, marked), "{name}");
        for other in ThemeColors::NAMES {
            let expected = if other == name {
                marked
            } else {
                base.get(other).unwrap()
            };
            assert_eq!(
                colors.get(other),
                Some(expected),
                "{name} set, {other} read"
            );
        }
    }
}

#[test]
fn an_unknown_name_sets_nothing() {
    let mut colors = builtin::dark().colors;
    assert!(!colors.set("nope", rgba(0x01_02_03_04)));
    assert_eq!(colors, builtin::dark().colors);
    assert_eq!(colors.get("nope"), None);
}

#[test]
fn a_partial_file_inherits_the_base_of_its_appearance() {
    let parsed = parse(r##"{"appearance":"light","colors":{"accent":"#112233"}}"##).unwrap();
    let expected = ThemeColors {
        accent: rgba(0x11_22_33_ff),
        ..builtin::of(Appearance::Light).colors
    };
    assert_eq!(parsed.theme.appearance, Appearance::Light);
    assert_eq!(parsed.theme.colors, expected);
    assert!(parsed.skipped.is_empty());
}

#[test]
fn an_invalid_colour_and_an_unknown_key_are_skipped_and_the_rest_applies() {
    let parsed = parse(
        r##"{"appearance":"dark","colors":{"accent":"red","nope":"#fff","text":"#010203"}}"##,
    )
    .unwrap();
    assert_eq!(parsed.skipped, ["accent", "nope"]);
    let dark = builtin::of(Appearance::Dark).colors;
    assert_eq!(
        parsed.theme.colors,
        ThemeColors {
            text: rgba(0x01_02_03_ff),
            ..dark
        }
    );
}

#[test]
fn a_syntax_error_reports_its_line() {
    let Err(error) = parse("{\n  \"appearance\": \"dark\",\n  \"colors\": {,\n}") else {
        panic!("a syntax error parses");
    };
    assert_eq!(error.line(), 3);
    let text = describe("theme \"x.json\"", &error);
    assert!(text.starts_with("theme \"x.json\": line 3: "), "{text}");
    assert!(!text.contains(" at line "), "{text}");
}

#[test]
fn a_file_without_an_appearance_is_rejected() {
    assert!(parse(r##"{"colors":{"accent":"#112233"}}"##).is_err());
}

#[test]
fn short_and_upper_case_colours_read_as_their_long_form() {
    assert_eq!(parse_color("#abc"), parse_color("#aabbcc"));
    assert_eq!(parse_color("#AABBCC"), parse_color("#aabbcc"));
    assert_eq!(parse_color("#aabbcc"), Some(rgba(0xaa_bb_cc_ff)));
}

#[test]
fn a_malformed_colour_is_none() {
    for text in [
        "", "#", "#ab", "#abcd", "#abcde", "aabbcc", "#gggggg", "#+f0000", " #abc",
    ] {
        assert_eq!(parse_color(text), None, "{text:?}");
    }
}

proptest! {
    #[test]
    fn every_eight_digit_colour_round_trips(value: u32) {
        prop_assert_eq!(parse_color(&format!("#{value:08x}")), Some::<Rgba>(rgba(value)));
    }
}
