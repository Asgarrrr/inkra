use std::sync::Arc;

use super::NoteContent;
use crate::Source;
use crate::test_support::mtime;

#[test]
fn parse_computes_the_links_and_the_words_of_the_text() {
    let content = NoteContent::parse(Arc::from("See [[plan]], Été"), mtime(0));
    assert_eq!(content.source, Source::parse("See [[plan]], Été"));
    assert_eq!(
        content
            .words
            .iter()
            .map(|(word, count)| (word.to_owned(), count))
            .collect::<Vec<_>>(),
        [
            ("ete".to_owned(), 1),
            ("plan".to_owned(), 1),
            ("see".to_owned(), 1)
        ]
    );
    assert_eq!(&*content.text, "See [[plan]], Été");
}

#[test]
fn parse_keeps_the_mtime() {
    assert_eq!(
        NoteContent::parse(Arc::from(""), mtime(7)).modified,
        mtime(7)
    );
}
