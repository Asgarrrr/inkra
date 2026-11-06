use super::varint::{push, read_checked};

fn checked(mut bytes: &[u8]) -> Option<u32> {
    let value = read_checked(&mut bytes)?;
    assert!(bytes.is_empty(), "{bytes:?} left");
    Some(value)
}

#[test]
fn the_fifth_byte_holds_four_bits() {
    assert_eq!(checked(&[0x80, 0x80, 0x80, 0x80, 0x0F]), Some(0x0F << 28));
    assert_eq!(checked(&[0x80, 0x80, 0x80, 0x80, 0x10]), None);
}

#[test]
fn a_sixth_byte_is_rejected() {
    assert_eq!(checked(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x00]), None);
    assert_eq!(checked(&[0xFF, 0xFF, 0xFF, 0xFF, 0x8F, 0x00]), None);
}

#[test]
fn each_edge_value_round_trips() {
    for value in [
        0,
        1,
        0x0F,
        0x10,
        127,
        128,
        16_383,
        16_384,
        1 << 28,
        u32::MAX,
    ] {
        let mut bytes = Vec::new();
        push(&mut bytes, value);
        assert_eq!(checked(&bytes), Some(value), "{value}");
    }
}

#[test]
fn a_value_leaves_the_next_one_in_place() {
    let mut bytes = Vec::new();
    push(&mut bytes, 300);
    push(&mut bytes, 7);
    let mut rest = &bytes[..];
    assert_eq!(read_checked(&mut rest), Some(300));
    assert_eq!(read_checked(&mut rest), Some(7));
    assert_eq!(read_checked(&mut rest), None);
}

#[test]
fn truncation_is_none() {
    assert_eq!(checked(&[]), None);
    assert_eq!(checked(&[0x80]), None);
    assert_eq!(checked(&[0xFF, 0xFF, 0xFF, 0xFF]), None);
}
