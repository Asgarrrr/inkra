use inkra_vault::Stamp;

use super::NoteContent;
use crate::Source;
use crate::test_support::{content, mtime};

#[test]
fn parse_computes_the_links_and_the_words_of_the_text() {
    let content = content("See [[plan]], Été", mtime(0));
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
}

#[test]
fn parse_keeps_the_stamp() {
    let stamp = Stamp {
        mtime: mtime(7),
        size: 3,
    };
    assert_eq!(NoteContent::parse("", stamp).stamp, stamp);
}
