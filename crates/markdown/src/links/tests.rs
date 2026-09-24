use proptest::prelude::*;

use super::{Link, LinkKind, link_at, links};
use crate::{Styles, parse};
use inkra_text::ByteOffset;

const W: LinkKind = LinkKind::Wiki;
const I: LinkKind = LinkKind::Inline;

/// Each link as the text it covers, its kind, whether it embeds, and its target.
fn linked(text: &str) -> Vec<(String, LinkKind, bool, String)> {
    links(text)
        .into_iter()
        .map(|link| {
            let Link {
                range,
                kind,
                embed,
                target,
            } = link;
            (
                text[range.start.0..range.end.0].to_owned(),
                kind,
                embed,
                target,
            )
        })
        .collect()
}

fn one(
    written: &str,
    kind: LinkKind,
    embed: bool,
    target: &str,
) -> (String, LinkKind, bool, String) {
    (written.to_owned(), kind, embed, target.to_owned())
}

#[test]
fn a_piped_wikilink_keeps_its_fragment_and_drops_its_alias() {
    assert_eq!(
        linked("see [[Note#Head|Alias]] here"),
        vec![one("[[Note#Head|Alias]]", W, false, "Note#Head")]
    );
}

#[test]
fn a_wiki_embed_includes_its_bang() {
    assert_eq!(
        linked("![[img.png]]"),
        vec![one("![[img.png]]", W, true, "img.png")]
    );
}

#[test]
fn an_inline_embed_is_inline() {
    assert_eq!(
        linked("![alt](pic.png)"),
        vec![one("![alt](pic.png)", I, true, "pic.png")]
    );
}

#[test]
fn an_inline_link_is_not_percent_decoded() {
    assert_eq!(
        linked("[x](my%20note.md#h)"),
        vec![one("[x](my%20note.md#h)", I, false, "my%20note.md#h")]
    );
}

#[test]
fn an_escaped_pipe_leaves_no_backslash_on_the_target() {
    let table = "| a | [[t\\|al]] |\n|---|---|\n| 1 | 2 |\n";
    assert_eq!(linked(table), vec![one("[[t\\|al]]", W, false, "t")]);
    assert_eq!(
        linked("[[a\\|b|c]]"),
        vec![one("[[a\\|b|c]]", W, false, "a")]
    );
}

#[test]
fn an_inline_target_keeps_a_trailing_backslash() {
    assert_eq!(
        linked("<https://x.org/a\\>"),
        vec![one("<https://x.org/a\\>", I, false, "https://x.org/a\\")]
    );
}

#[test]
fn a_target_is_trimmed() {
    assert_eq!(
        linked("[[ spaced ]]"),
        vec![one("[[ spaced ]]", W, false, "spaced")]
    );
}

#[test]
fn everything_after_the_first_pipe_is_the_alias() {
    assert_eq!(linked("[[a|b]]"), vec![one("[[a|b]]", W, false, "a")]);
}

#[test]
fn a_repeated_link_is_two_links() {
    assert_eq!(
        linked("[[Note]] [[Note]]"),
        vec![
            one("[[Note]]", W, false, "Note"),
            one("[[Note]]", W, false, "Note")
        ]
    );
}

#[test]
fn code_html_and_frontmatter_hold_no_links() {
    assert_eq!(linked("`[[code]]`"), vec![]);
    assert_eq!(linked("```\n[[fence]]\n```\n"), vec![]);
    assert_eq!(linked("<div>[[x]]</div>\n"), vec![]);
    assert_eq!(linked("---\nup: \"[[a]]\"\n---\nbody\n"), vec![]);
}

#[test]
fn an_email_autolink_gets_its_scheme() {
    assert_eq!(
        linked("<a@b.com>"),
        vec![one("<a@b.com>", I, false, "mailto:a@b.com")]
    );
}

#[test]
fn a_url_autolink_is_inline() {
    assert_eq!(
        linked("<https://x.org>"),
        vec![one("<https://x.org>", I, false, "https://x.org")]
    );
}

#[test]
fn a_reference_link_takes_its_definition_and_the_definition_is_none() {
    assert_eq!(
        linked("[x][r]\n\n[r]: note.md\n"),
        vec![one("[x][r]", I, false, "note.md")]
    );
}

#[test]
fn an_empty_target_is_no_link() {
    assert_eq!(linked("[x]()"), vec![]);
    assert_eq!(linked("[[ ]]"), vec![]);
    assert_eq!(linked("[[\\|x]]"), vec![]);
}

#[test]
fn links_after_an_empty_alias_keep_their_own_targets() {
    assert_eq!(
        linked("[[a|]][[b|c]]"),
        vec![one("[[a|]]", W, false, "a"), one("[[b|c]]", W, false, "b")]
    );
    assert_eq!(
        linked("- [[a|]] [[b|]] [y](z) ![[c]]"),
        vec![
            one("[[a|]]", W, false, "a"),
            one("[[b|]]", W, false, "b"),
            one("[y](z)", I, false, "z"),
            one("![[c]]", W, true, "c"),
        ]
    );
}

#[test]
fn an_image_inside_a_link_is_two_links_in_order() {
    assert_eq!(
        linked("[![i](a.png)](b.md)"),
        vec![
            one("[![i](a.png)](b.md)", I, false, "b.md"),
            one("![i](a.png)", I, true, "a.png"),
        ]
    );
}

#[test]
fn link_at_finds_the_innermost_link_touching_the_offset() {
    let text = "x [![i](a.png)](b.md) [[c]]";
    let at = |offset| link_at(text, ByteOffset(offset)).map(|link| link.target);
    assert_eq!(at(0), None);
    assert_eq!(at(2), Some("b.md".to_owned()));
    assert_eq!(at(3), Some("a.png".to_owned()));
    assert_eq!(at(13), Some("a.png".to_owned()));
    assert_eq!(at(14), Some("b.md".to_owned()));
    assert_eq!(at(15), Some("b.md".to_owned()));
    assert_eq!(at(21), Some("b.md".to_owned()));
    assert_eq!(at(22), Some("c".to_owned()));
    assert_eq!(at(27), Some("c".to_owned()));
}

#[test]
fn link_at_prefers_a_late_inner_link_over_its_outer_one() {
    let text = "[aaaaaaaa ![i](b.png)](c.md)";
    assert_eq!(
        link_at(text, ByteOffset(12)).map(|link| link.target),
        Some("b.png".to_owned())
    );
}

#[test]
fn link_at_between_two_links_follows_the_second() {
    let text = "[[a]][[b]]";
    let at = |offset| link_at(text, ByteOffset(offset)).map(|link| link.target);
    assert_eq!(at(4), Some("a".to_owned()));
    assert_eq!(at(5), Some("b".to_owned()));
    assert_eq!(at(10), Some("b".to_owned()));
    assert_eq!(at(11), None);
}

fn check_invariants(text: &str) {
    let found = links(text);
    let code = parse(text).spans;
    for link in &found {
        let (start, end) = (link.range.start.0, link.range.end.0);
        assert!(
            start < end && end <= text.len(),
            "{link:?} outside {text:?}"
        );
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert!(!link.target.is_empty(), "empty target {link:?} in {text:?}");
        assert_eq!(link.target, link.target.trim(), "untrimmed {link:?}");
        assert!(
            !code.iter().any(|span| span.styles.contains(Styles::CODE)
                && span.range.start <= link.range.start
                && link.range.start < span.range.end),
            "{link:?} starts inside code in {text:?}"
        );
    }
    for pair in found.windows(2) {
        assert!(
            pair[0].range.start <= pair[1].range.start,
            "unsorted: {pair:?}"
        );
        assert_ne!(pair[0].range, pair[1].range, "repeated: {pair:?}");
    }
}

fn wikilink() -> impl Strategy<Value = (String, String)> {
    ("[a-z]{1,4}", prop_oneof![Just(""), Just("|"), Just("|x")])
        .prop_map(|(name, alias)| (format!("[[{name}{alias}]]"), name))
}

proptest! {
    #[test]
    fn links_hold_their_invariants_on_any_text(text in any::<String>()) {
        check_invariants(&text);
    }

    #[test]
    fn links_hold_their_invariants_on_syntax_heavy_text(
        text in "[a-zé *_`\\[\\]()#>!|~:=+.<>@\\\\0-9\n-]{0,120}"
    ) {
        check_invariants(&text);
    }

    #[test]
    fn every_wikilink_in_a_paragraph_is_found_once(
        written in prop::collection::vec(wikilink(), 1..8),
        separator in prop_oneof![Just(""), Just(" "), Just("\n")],
    ) {
        let text = written.iter().map(|(link, _)| link.as_str()).collect::<Vec<_>>().join(separator);
        let targets: Vec<String> = links(&text).into_iter().map(|link| link.target).collect();
        let expected: Vec<String> = written.into_iter().map(|(_, name)| name).collect();
        prop_assert_eq!(targets, expected, "in {:?}", text);
    }
}
