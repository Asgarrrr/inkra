use std::io;
use std::path::PathBuf;

use super::listed;

/// Deleted between the listing and its stat: a normal race, so the folder still reads.
#[test]
fn a_child_gone_before_its_stat_is_skipped() {
    let gone = Err(io::Error::from(io::ErrorKind::NotFound));
    assert!(matches!(
        listed(PathBuf::from("/v/gone.md"), gone),
        Ok(None)
    ));
}

#[test]
fn a_child_that_fails_its_stat_otherwise_fails_the_listing() {
    let denied = Err(io::Error::from(io::ErrorKind::PermissionDenied));
    assert!(listed(PathBuf::from("/v/denied.md"), denied).is_err());
}
