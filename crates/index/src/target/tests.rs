use inkra_markdown::{LinkKind, links};

use super::{Style, Target};

fn parsed(text: &str) -> Target {
    let found = links(text);
    assert_eq!(found.len(), 1, "one link in {text:?}");
    Target::parse(&found[0])
}

fn note(path: &str, fragment: Option<&str>, style: Style) -> Target {
    Target::Note {
        path: path.to_owned(),
        fragment: fragment.map(str::to_owned),
        style,
    }
}

#[test]
fn a_bare_wikilink_is_a_name() {
    assert_eq!(parsed("[[Plan]]"), note("Plan", None, Style::Name));
}

#[test]
fn a_wikilink_with_a_folder_is_a_suffix() {
    assert_eq!(
        parsed("[[work/Plan]]"),
        note("work/Plan", None, Style::Suffix)
    );
}

#[test]
fn a_dotted_or_rooted_wikilink_is_relative() {
    assert_eq!(parsed("[[./Plan]]"), note("./Plan", None, Style::Relative));
    assert_eq!(
        parsed("[[../Plan]]"),
        note("../Plan", None, Style::Relative)
    );
    assert_eq!(
        parsed("[[/work/Plan]]"),
        note("/work/Plan", None, Style::Relative)
    );
}

#[test]
fn a_wikilink_starting_with_a_dot_name_is_a_name() {
    assert_eq!(parsed("[[.Plan]]"), note(".Plan", None, Style::Name));
    assert_eq!(
        parsed("[[..Plan/x]]"),
        note("..Plan/x", None, Style::Suffix)
    );
}

#[test]
fn the_fragment_is_split_at_the_first_hash() {
    assert_eq!(
        parsed("[[Plan#Goals#Q1]]"),
        note("Plan", Some("Goals#Q1"), Style::Name)
    );
}

#[test]
fn the_extension_is_stripped_in_any_case() {
    assert_eq!(parsed("[[Plan.MD]]"), note("Plan", None, Style::Name));
    assert_eq!(
        parsed("[x](Plan.md#h)"),
        note("Plan", Some("h"), Style::Relative)
    );
    assert_eq!(parsed("[[v1.2]]"), note("v1.2", None, Style::Name));
}

#[test]
fn only_an_inline_target_is_percent_decoded() {
    assert_eq!(
        parsed("[x](my%20note.md)"),
        note("my note", None, Style::Relative)
    );
    assert_eq!(parsed("[x](a%23b.md)"), note("a#b", None, Style::Relative));
    assert_eq!(
        parsed("[[my%20note]]"),
        note("my%20note", None, Style::Name)
    );
}

#[test]
fn invalid_utf8_after_decoding_is_replaced_not_dropped() {
    assert_eq!(
        parsed("[x](a%FF.md)"),
        note("a\u{fffd}", None, Style::Relative)
    );
}

#[test]
fn an_inline_link_is_relative_even_with_a_folder() {
    assert_eq!(
        parsed("[x](../work/Plan.md)"),
        note("../work/Plan", None, Style::Relative)
    );
    assert_eq!(
        parsed("[x](/Plan.md)"),
        note("/Plan", None, Style::Relative)
    );
}

#[test]
fn a_url_is_external() {
    assert_eq!(
        parsed("[x](https://x.org/a.md)"),
        Target::External("https://x.org/a.md".to_owned())
    );
    assert_eq!(
        parsed("<a@b.com>"),
        Target::External("mailto:a@b.com".to_owned())
    );
    assert_eq!(
        parsed("[x](x-y.z+1:rest)"),
        Target::External("x-y.z+1:rest".to_owned())
    );
}

#[test]
fn a_drive_letter_or_odd_prefix_is_not_a_scheme() {
    assert_eq!(
        parsed("[x](C:notes.md)"),
        note("C:notes", None, Style::Relative)
    );
    assert_eq!(
        parsed("[x](1a:notes.md)"),
        note("1a:notes", None, Style::Relative)
    );
    assert_eq!(
        parsed("[x](a_b:notes.md)"),
        note("a_b:notes", None, Style::Relative)
    );
    assert_eq!(
        parsed("[x](:notes.md)"),
        note(":notes", None, Style::Relative)
    );
}

#[test]
fn a_wikilink_is_never_external() {
    assert_eq!(parsed("[[https:x]]"), note("https:x", None, Style::Name));
}

#[test]
fn an_empty_path_is_the_source_itself() {
    assert_eq!(parsed("[[#Heading]]"), Target::SelfLink);
    assert_eq!(parsed("[x](#h)"), Target::SelfLink);
    assert_eq!(parsed("[[.md]]"), Target::SelfLink);
}

#[test]
fn the_kind_decides_the_style_not_the_text() {
    let found = links("[[a/b]] [x](a/b)");
    assert_eq!(found[0].kind, LinkKind::Wiki);
    assert_eq!(Target::parse(&found[0]), note("a/b", None, Style::Suffix));
    assert_eq!(Target::parse(&found[1]), note("a/b", None, Style::Relative));
}
