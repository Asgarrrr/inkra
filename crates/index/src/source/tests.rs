use super::{BEFORE, CONTEXT, Source};

fn contexts(text: &str) -> Vec<(String, String)> {
    Source::parse(text)
        .links
        .into_iter()
        .map(|link| {
            let lit = link.context[link.context_link.clone()].to_owned();
            (link.context, lit)
        })
        .collect()
}

#[test]
fn only_note_links_are_kept() {
    let source = Source::parse("[[a]] <https://x.org> [[#h]] [x](b.md)");
    let paths: Vec<_> = source.links.iter().map(|link| &*link.path).collect();
    assert_eq!(paths, ["a", "b"]);
}

#[test]
fn a_short_line_is_the_whole_context() {
    assert_eq!(
        contexts("first\nsee [[a]] here\nlast"),
        [("see [[a]] here".to_owned(), "[[a]]".to_owned())]
    );
}

#[test]
fn a_long_line_is_cut_around_the_link() {
    let text = format!("{}[[a]]{}", "x".repeat(100), "y".repeat(300));
    let found = contexts(&text);
    let (context, lit) = &found[0];
    assert_eq!(context.len(), CONTEXT);
    assert!(context.starts_with(&"x".repeat(BEFORE)));
    assert_eq!(&context[BEFORE..BEFORE + 5], "[[a]]");
    assert_eq!(lit, "[[a]]");
}

#[test]
fn a_cut_lands_on_char_boundaries() {
    let text = format!("{}[[a]]{}", "é".repeat(50), "ü".repeat(200));
    for (context, lit) in contexts(&text) {
        assert!(context.len() <= CONTEXT);
        assert_eq!(lit, "[[a]]");
    }
}

#[test]
fn a_cut_inside_a_char_moves_toward_the_link() {
    let text = format!("{}é[[a]]{}", "😀".repeat(20), "ü".repeat(200));
    let link = &Source::parse(&text).links[0];
    assert!(
        link.context_link.start <= BEFORE,
        "{} bytes before",
        link.context_link.start
    );
    assert!(link.context.len() <= CONTEXT);
    assert_eq!(&link.context[link.context_link.clone()], "[[a]]");
}

#[test]
fn a_link_longer_than_the_window_is_clamped() {
    let text = format!("[[{}]]", "a".repeat(300));
    let found = contexts(&text);
    let (context, lit) = &found[0];
    assert_eq!(context.len(), CONTEXT);
    assert_eq!(lit.len(), CONTEXT);
}

#[test]
fn a_link_across_lines_keeps_its_first_line() {
    assert_eq!(
        contexts("go [a\nb](c.md) on"),
        [("go [a".to_owned(), "[a".to_owned())]
    );
}
