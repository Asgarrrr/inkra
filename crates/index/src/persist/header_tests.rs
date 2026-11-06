use std::path::Path;

use super::fingerprint::{derivation_fingerprint, fold_fingerprint};
use super::{Rejected, decode, encode};
use crate::NoteIndex;

const ROOT: &str = "/v";

fn bytes() -> Vec<u8> {
    encode(&NoteIndex::default(), Path::new(ROOT))
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
    assert_eq!(derivation_fingerprint(), 0x2F13_6D1F);
    let golden: [u8; 55] = [
        b'I', b'N', b'K', b'R', b'A', b'I', b'X', 0, // magic
        1, 0, 0, 0, // version
        0x0D, 0x15, 0xD3, 0xC6, // fold
        0x1F, 0x6D, 0x13, 0x2F, // derivation
        0xE9, 0x63, 0xF8, 0x38, // crc32 of the body
        3, 2, 1, 24, // three sections: two bytes, one, then 24
        b'/', b'v', // the root
        0,    // no note
        0,    // next
        0, 0, 0, 0, 0, 0, 0, 0, // total
        0, // no alive bit
        0, // no held note
        0, // no word byte
        1, 0, 0, 0, 0, // one bound, 0
        1, 0, 0, 0, 0, // one start, 0
        0, // no posting
        0, // no length
    ];
    assert_eq!(bytes(), golden);
}

#[test]
fn round_trip_ok() {
    assert_eq!(
        decode(&bytes(), Path::new(ROOT)).map(|decoded| decoded.notes),
        Ok(Vec::new())
    );
    let root = Path::new("/Users/me/Notes/Été 🌱");
    let bytes = encode(&NoteIndex::default(), root);
    assert_eq!(
        decode(&bytes, root).map(|decoded| decoded.notes.len()),
        Ok(0)
    );
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
fn a_count_other_than_three_is_corrupt() {
    for count in [2, 4] {
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
            decode(&bytes, Path::new(other))
                .map(|decoded| decoded.notes)
                .map_err(|rejected| (rejected, rejected.reason())),
            Err((Rejected::Root, "root")),
            "{other}"
        );
    }
}

#[test]
fn no_truncation_panics() {
    let root = format!("/{}", "v".repeat(200));
    let bytes = encode(&NoteIndex::default(), Path::new(&root));
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
