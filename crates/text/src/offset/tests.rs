use super::*;

#[test]
fn byte_and_utf16_offsets_are_distinct_types_with_value_equality() {
    assert_eq!(ByteOffset(3), ByteOffset(3));
    assert_eq!(Utf16Offset(3).0, ByteOffset(3).0);
}

#[test]
fn offsets_order_by_value() {
    assert!(ByteOffset(1) < ByteOffset(2));
    assert!(Utf16Offset(5) > Utf16Offset(4));
}

#[test]
fn offsets_display_as_their_value() {
    assert_eq!(ByteOffset(7).to_string(), "7");
    assert_eq!(Utf16Offset(7).to_string(), "7");
}
