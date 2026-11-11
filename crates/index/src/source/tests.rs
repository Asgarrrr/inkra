use std::sync::Arc;

use super::Source;
use crate::Style;

#[test]
fn only_note_links_are_kept() {
    let source = Source::parse("[[a]] <https://x.org> [[#h]] [x](b.md)");
    let paths: Vec<_> = source.links.iter().map(|link| &*link.path).collect();
    assert_eq!(paths, ["a", "b"]);
}

#[test]
fn one_parse_shares_a_path() {
    let source = Source::parse("[[a]] [[B]] [[a#h]]");
    let [a, upper, again] = &*source.links else {
        panic!("three links, not {}", source.links.len());
    };
    assert!(
        Arc::ptr_eq(&a.path, &again.path),
        "a reload shares as a decode does"
    );
    assert!(
        !Arc::ptr_eq(&a.path, &upper.path),
        "the written form is the key"
    );
}

#[test]
fn a_link_keeps_its_written_target_and_its_range() {
    let source = Source::parse("see [[a/b#h]]\n[x](./c.md) é[[d]]");
    let links: Vec<_> = source
        .links
        .iter()
        .map(|link| {
            (
                &*link.path,
                link.fragment.as_deref(),
                link.md,
                link.style,
                link.range.start.get()..link.range.end.get(),
            )
        })
        .collect();
    assert_eq!(
        links,
        [
            ("a/b", Some("h"), false, Style::Suffix, 4..13),
            ("./c", None, true, Style::Relative, 14..25),
            ("d", None, false, Style::Name, 28..33),
        ]
    );
}
