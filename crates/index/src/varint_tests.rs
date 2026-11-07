use crate::varint::{push, read, read_checked};

fn encoded(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    for &value in values {
        push(&mut out, value);
    }
    out
}

fn decoded(mut bytes: &[u8]) -> Vec<u32> {
    let mut values = Vec::new();
    while !bytes.is_empty() {
        values.push(read(&mut bytes));
    }
    values
}

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
fn each_edge_value_round_trips_in_its_length() {
    for (value, len) in [
        (0, 1),
        (1, 1),
        (0x0F, 1),
        (0x10, 1),
        (127, 1),
        (128, 2),
        (16_383, 2),
        (16_384, 3),
        (1 << 28, 5),
        (u32::MAX, 5),
    ] {
        let bytes = encoded(&[value]);
        assert_eq!(bytes.len(), len, "{value}");
        assert_eq!(checked(&bytes), Some(value), "{value}");
        assert_eq!(decoded(&bytes), [value]);
    }
}

#[test]
fn a_value_leaves_the_next_one_in_place() {
    let bytes = encoded(&[300, 7]);
    let mut rest = &bytes[..];
    assert_eq!(read_checked(&mut rest), Some(300));
    assert_eq!(read_checked(&mut rest), Some(7));
    assert_eq!(read_checked(&mut rest), None);
}

#[test]
fn a_gap_sequence_round_trips() {
    let gaps = [
        0,
        1,
        1,
        127,
        128,
        300,
        65_535,
        1 << 21,
        1 << 28,
        u32::MAX,
        5,
    ];
    assert_eq!(decoded(&encoded(&gaps)), gaps);
}

#[test]
fn truncation_is_none() {
    assert_eq!(checked(&[]), None);
    assert_eq!(checked(&[0x80]), None);
    assert_eq!(checked(&[0xFF, 0xFF, 0xFF, 0xFF]), None);
}

#[test]
#[should_panic(expected = "a varint the index wrote does not decode")]
fn a_truncated_value_is_a_caller_bug() {
    let mut bytes = &[0x80_u8][..];
    let _ = read(&mut bytes);
}

#[test]
#[should_panic(expected = "a varint the index wrote does not decode")]
fn a_sixth_byte_is_a_caller_bug() {
    let mut bytes = &[0x80_u8, 0x80, 0x80, 0x80, 0x80, 0x00][..];
    let _ = read(&mut bytes);
}
