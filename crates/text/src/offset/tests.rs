use super::*;

#[test]
fn byte_and_utf16_offsets_are_distinct_types_with_value_equality() {
    assert_eq!(ByteOffset::new(3), ByteOffset::new(3));
    assert_eq!(Utf16Offset::new(3).get(), ByteOffset::new(3).get());
}

#[test]
fn offsets_order_by_value() {
    assert!(ByteOffset::new(1) < ByteOffset::new(2));
    assert!(Utf16Offset::new(5) > Utf16Offset::new(4));
}

#[test]
fn offsets_display_as_their_value() {
    assert_eq!(ByteOffset::new(7).to_string(), "7");
    assert_eq!(Utf16Offset::new(7).to_string(), "7");
}

#[test]
fn get_returns_the_value_new_took() {
    assert_eq!(ByteOffset::new(7).get(), 7);
    assert_eq!(Utf16Offset::new(7).get(), 7);
}

#[test]
fn byte_offsets_move_by_a_length_and_differ_by_one() {
    assert_eq!(ByteOffset::new(3) + 2, ByteOffset::new(5));
    assert_eq!(ByteOffset::new(5) - 2, ByteOffset::new(3));
    assert_eq!(ByteOffset::new(5) - ByteOffset::new(3), 2);
}

#[test]
fn utf16_offsets_move_by_a_length_and_differ_by_one() {
    assert_eq!(Utf16Offset::new(3) + 2, Utf16Offset::new(5));
    assert_eq!(Utf16Offset::new(5) - 2, Utf16Offset::new(3));
    assert_eq!(Utf16Offset::new(5) - Utf16Offset::new(3), 2);
}
