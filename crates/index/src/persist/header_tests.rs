use std::path::Path;

use super::fingerprint::{derivation_fingerprint, fold_fingerprint};
use super::{Rejected, decode, encode};

const ROOT: &str = "/v";

fn bytes() -> Vec<u8> {
    encode(Path::new(ROOT))
}

/// `bytes` with its crc over the body set again, so a later check sees the change.
fn resealed(mut bytes: Vec<u8>) -> Vec<u8> {
    let crc = crc32fast::hash(&bytes[24..]);
    bytes[20..24].copy_from_slice(&crc.to_le_bytes());
    bytes
}

fn rejected(bytes: &[u8]) -> (Rejected, &'static str) {
    let Err(rejected) = decode(bytes, Path::new(ROOT)) else {
        panic!("{bytes:?} decodes");
    };
    (rejected, rejected.reason())
}

#[test]
fn golden_bytes() {
    assert_eq!(fold_fingerprint(), 0xC6D3_150D);
    assert_eq!(derivation_fingerprint(), 0xBA5E_B88C);
    let golden: [u8; 28] = [
        b'I', b'N', b'K', b'R', b'A', b'I', b'X', 0, // magic
        1, 0, 0, 0, // version
        0x0D, 0x15, 0xD3, 0xC6, // fold
        0x8C, 0xB8, 0x5E, 0xBA, // derivation
        0x73, 0x80, 0x06, 0x31, // crc32 of the body
        1, 2, b'/', b'v', // one section of two bytes: the root
    ];
    assert_eq!(bytes(), golden);
}

#[test]
fn round_trip_ok() {
    assert_eq!(decode(&bytes(), Path::new(ROOT)), Ok(()));
    let root = Path::new("/Users/me/Notes/Été 🌱");
    assert_eq!(decode(&encode(root), root), Ok(()));
}

#[test]
fn twenty_three_is_short_twenty_four_is_not() {
    let bytes = bytes();
    assert_eq!(rejected(&bytes[..23]), (Rejected::Short, "short"));
    assert_eq!(rejected(&bytes[..24]).0, Rejected::Checksum);
    assert_eq!(
        rejected(&resealed(bytes[..24].to_vec())).0,
        Rejected::Corrupt("sections")
    );
}

#[test]
fn each_header_field_rejects_by_name() {
    for (at, expected) in [
        (8, (Rejected::Version, "version")),
        (12, (Rejected::Fold, "fold")),
        (16, (Rejected::Derivation, "derivation")),
    ] {
        let mut bytes = bytes();
        bytes[at] ^= 1;
        assert_eq!(rejected(&resealed(bytes)), expected, "byte {at}");
    }
}

#[test]
fn version_is_checked_before_the_crc() {
    let mut bytes = bytes();
    bytes[8] = 2;
    bytes[24] ^= 1;
    assert_eq!(rejected(&bytes).0, Rejected::Version);
}

#[test]
fn every_body_flip_is_checksum() {
    let bytes = bytes();
    for at in 20..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            flipped[at] ^= 1 << bit;
            assert_eq!(
                rejected(&flipped),
                (Rejected::Checksum, "checksum"),
                "byte {at} bit {bit}"
            );
        }
    }
}

#[test]
fn magic_flip_is_corrupt() {
    for at in 0..8 {
        let mut bytes = bytes();
        bytes[at] ^= 1;
        assert_eq!(
            rejected(&bytes),
            (Rejected::Corrupt("magic"), "magic"),
            "byte {at}"
        );
    }
}

#[test]
fn a_count_other_than_one_is_corrupt() {
    for count in [0, 2] {
        let mut bytes = bytes();
        bytes[24] = count;
        assert_eq!(
            rejected(&resealed(bytes)),
            (Rejected::Corrupt("sections"), "sections"),
            "{count}"
        );
    }
}

#[test]
fn lengths_must_equal_size() {
    let mut short = bytes();
    short.pop();
    let mut long = bytes();
    long.push(b'/');
    let mut huge = bytes()[..25].to_vec();
    huge.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    let mut unterminated = bytes()[..25].to_vec();
    unterminated.push(0x80);
    for bytes in [short, long, huge, unterminated] {
        assert_eq!(
            rejected(&resealed(bytes)),
            (Rejected::Corrupt("lengths"), "lengths")
        );
    }
}

#[test]
fn other_root_is_root() {
    let bytes = bytes();
    for other in ["/w", "/v/", "/", "/vv"] {
        assert_eq!(
            decode(&bytes, Path::new(other)).map_err(|rejected| (rejected, rejected.reason())),
            Err((Rejected::Root, "root")),
            "{other}"
        );
    }
}

#[test]
fn no_truncation_panics() {
    let root = format!("/{}", "v".repeat(200));
    let bytes = encode(Path::new(&root));
    for len in 0..bytes.len() {
        let prefix = &bytes[..len];
        assert!(decode(prefix, Path::new(ROOT)).is_err(), "{len}");
        if len >= 24 {
            assert!(
                decode(&resealed(prefix.to_vec()), Path::new(ROOT)).is_err(),
                "{len}"
            );
        }
    }
}
