use std::borrow::Cow;

use super::one_line;

#[test]
fn a_control_char_becomes_one_space() {
    assert_eq!(one_line("a\nb\r\tc\u{7f}é"), "a b  c é");
}

#[test]
fn a_name_without_one_is_borrowed() {
    assert!(matches!(one_line("plan é"), Cow::Borrowed("plan é")));
}
