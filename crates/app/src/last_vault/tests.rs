use std::path::Path;

use futures::executor::block_on;
use inkra_vault::mem_fs::MemFs;

use super::{read, write};
use crate::app_files::last_vault_file;

const HOME: &str = "/home";

#[test]
fn a_written_last_vault_reads_back() {
    let fs = MemFs::new();
    block_on(write(&fs, Path::new(HOME), Path::new("/vault")));

    assert_eq!(
        block_on(read(&fs, Path::new(HOME))).as_deref(),
        Some(Path::new("/vault"))
    );
}

/// The file's shape, read from text: version 1 only, anything else silently nothing.
#[test]
fn only_a_version_1_file_names_a_vault() {
    for (text, expected) in [
        (
            r#"{"version":1,"root":"/vault"}"#,
            Some(Path::new("/vault")),
        ),
        (r#"{"version":2,"root":"/vault"}"#, None),
        ("garbage", None),
    ] {
        let fs = MemFs::new();
        fs.insert_file(last_vault_file(Path::new(HOME)), text);
        assert_eq!(
            block_on(read(&fs, Path::new(HOME))).as_deref(),
            expected,
            "{text}"
        );
    }
}
