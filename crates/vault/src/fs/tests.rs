use super::normalize_line_endings;

#[test]
fn a_stamp_differs_when_only_the_size_does() {
    use super::{Mtime, Stamp};
    use std::time::{Duration, SystemTime};

    let at = Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
    assert_ne!(Stamp { mtime: at, size: 1 }, Stamp { mtime: at, size: 2 });
}

/// Every other conflict test would still pass if `!=` were relaxed to `>`.
#[test]
fn a_stamp_differs_when_the_mtime_moves_backwards() {
    use super::{Mtime, Stamp};
    use std::time::{Duration, SystemTime};

    let newer = Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(20)),
        size: 4,
    };
    let older = Stamp {
        mtime: Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(10)),
        size: 4,
    };
    assert_ne!(newer, older);
}

#[test]
fn crlf_becomes_lf() {
    assert_eq!(normalize_line_endings("a\r\nb"), "a\nb");
}

#[test]
fn a_lone_cr_becomes_lf() {
    assert_eq!(normalize_line_endings("a\rb"), "a\nb");
}

#[test]
fn text_without_cr_is_unchanged() {
    assert_eq!(normalize_line_endings("a\nb"), "a\nb");
}

#[test]
fn a_trailing_cr_becomes_lf() {
    assert_eq!(normalize_line_endings("a\r"), "a\n");
}
