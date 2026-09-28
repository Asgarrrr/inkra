use crate::test_support::{entries, vault};
use crate::words::NameWords;
use crate::{NamesSnapshot, NoteSet};

fn assert_send_sync<T: Send + Sync + 'static>() {}

#[test]
fn the_name_table_and_note_set_cross_threads() {
    assert_send_sync::<NamesSnapshot>();
    assert_send_sync::<NoteSet>();
}

#[test]
fn heap_bytes_counts_the_name_words() {
    let (_fs, vault) = vault(&["Ab/c.md", "d.md"]);
    assert_eq!(
        NoteSet::build(entries(&vault)).names.heap_bytes(),
        NameWords::build(["Ab/c", "d"]).heap_bytes()
    );
}
