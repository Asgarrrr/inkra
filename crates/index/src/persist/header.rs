use std::path::Path;

use super::fingerprint::{derivation_fingerprint, fold_fingerprint};
use crate::words::varint;

pub const FORMAT_VERSION: u32 = 1;
/// The reader refuses a larger file from its stat, before reading it.
pub const MAX_BYTES: u64 = 256 << 20;

const MAGIC: [u8; 8] = *b"INKRAIX\0";
/// The magic, then the version, both fingerprints and the body's crc, each a u32 LE.
const HEADER_BYTES: usize = 24;
const SECTIONS: usize = 1;

/// Why a cache file is not used: a list of reasons, not a second error enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    Short,
    Version,
    Fold,
    Derivation,
    Root,
    Checksum,
    Corrupt(&'static str),
}

impl Rejected {
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Version => "version",
            Self::Fold => "fold",
            Self::Derivation => "derivation",
            Self::Root => "root",
            Self::Checksum => "checksum",
            Self::Corrupt(what) => what,
        }
    }
}

/// The cache file of the vault at `root`.
///
/// # Panics
///
/// When the file would exceed `MAX_BYTES`, which no reader accepts.
#[must_use]
pub fn encode(root: &Path) -> Vec<u8> {
    let sections: [&[u8]; SECTIONS] = [root.as_os_str().as_encoded_bytes()];
    let mut body = Vec::new();
    varint::push(&mut body, narrow(sections.len()));
    for section in sections {
        varint::push(&mut body, narrow(section.len()));
    }
    for section in sections {
        body.extend_from_slice(section);
    }
    let mut bytes = Vec::with_capacity(HEADER_BYTES + body.len());
    bytes.extend_from_slice(&MAGIC);
    for field in fields(&body) {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&body);
    assert!(
        u64::try_from(bytes.len()).is_ok_and(|len| len <= MAX_BYTES),
        "the index cache is {} bytes, over MAX_BYTES",
        bytes.len()
    );
    bytes
}

/// Checks `bytes` as the cache file of the vault at `root`; no input panics.
///
/// # Errors
///
/// The first check that fails, in the header's order: a newer format is `Version`, not `Checksum`.
pub fn decode(bytes: &[u8], root: &Path) -> Result<(), Rejected> {
    let Some((header, mut body)) = bytes.split_first_chunk::<HEADER_BYTES>() else {
        return Err(Rejected::Short);
    };
    if header[..MAGIC.len()] != MAGIC {
        return Err(Rejected::Corrupt("magic"));
    }
    let (found, _) = header[MAGIC.len()..].as_chunks::<4>();
    let found = found.iter().map(|&field| u32::from_le_bytes(field));
    let rejections = [
        Rejected::Version,
        Rejected::Fold,
        Rejected::Derivation,
        Rejected::Checksum,
    ];
    for ((found, expected), rejected) in found.zip(fields(body)).zip(rejections) {
        if found != expected {
            return Err(rejected);
        }
    }
    if varint::read_checked(&mut body).and_then(|count| usize::try_from(count).ok())
        != Some(SECTIONS)
    {
        return Err(Rejected::Corrupt("sections"));
    }
    let mut lengths = [0; SECTIONS];
    let mut total: usize = 0;
    for length in &mut lengths {
        *length = varint::read_checked(&mut body)
            .and_then(|length| usize::try_from(length).ok())
            .ok_or(Rejected::Corrupt("lengths"))?;
        total = total
            .checked_add(*length)
            .ok_or(Rejected::Corrupt("lengths"))?;
    }
    if total != body.len() {
        return Err(Rejected::Corrupt("lengths"));
    }
    let [stored_root] = lengths.map(|length| {
        let (section, rest) = body.split_at(length);
        body = rest;
        section
    });
    if stored_root != root.as_os_str().as_encoded_bytes() {
        return Err(Rejected::Root);
    }
    Ok(())
}

/// The header's fields after the magic, for a file whose body is `body`.
fn fields(body: &[u8]) -> [u32; 4] {
    [
        FORMAT_VERSION,
        fold_fingerprint(),
        derivation_fingerprint(),
        crc32fast::hash(body),
    ]
}

fn narrow(len: usize) -> u32 {
    let Ok(len) = u32::try_from(len) else {
        panic!("an index cache section of {len} bytes, over 4 GiB");
    };
    len
}
