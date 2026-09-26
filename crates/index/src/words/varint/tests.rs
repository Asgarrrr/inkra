use super::{push, read};

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

#[test]
fn each_edge_value_round_trips_in_its_length() {
    for (value, len) in [
        (0, 1),
        (127, 1),
        (128, 2),
        (16_383, 2),
        (16_384, 3),
        (u32::MAX, 5),
    ] {
        let bytes = encoded(&[value]);
        assert_eq!(bytes.len(), len, "{value}");
        assert_eq!(decoded(&bytes), [value]);
    }
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
#[should_panic(expected = "a varint ends past its bytes")]
fn a_truncated_value_is_a_caller_bug() {
    let mut bytes = &[0x80_u8][..];
    let _ = read(&mut bytes);
}
