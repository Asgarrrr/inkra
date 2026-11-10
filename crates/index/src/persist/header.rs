use std::path::Path;

use super::bytes::narrow;
use super::fingerprint::{derivation_fingerprint, fold_fingerprint};
use super::notes::{self, CachedNote};
use super::words::{self, CachedWords};
use crate::IndexSnapshot;
use crate::varint;

pub const FORMAT_VERSION: u32 = 2;
/// The reader refuses a larger file from its stat, before reading it.
pub const MAX_BYTES: u64 = 256 << 20;

const MAGIC: [u8; 8] = *b"INKRAIX\0";
/// The magic, then the version, both fingerprints and the body's crc, each a u32 LE.
const HEADER_BYTES: usize = 24;
const SECTIONS: usize = 3;

/// What a cache file holds, once every check passed.
#[derive(Debug)]
pub struct Decoded {
    pub notes: Vec<CachedNote>,
    pub words: CachedWords,
    /// The body's crc32, as the header holds it: the scrub's seed (P16).
    pub crc: u32,
}

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

/// The cache file of `snapshot`, the vault at `root`.
///
/// `None` when the file would exceed `MAX_BYTES`, which no reader accepts.
#[must_use]
pub fn encode(snapshot: &IndexSnapshot, root: &Path) -> Option<Vec<u8>> {
    let words = snapshot.words();
    let (cached, to) = words::cached(words);
    let ordinal = |id| *to.get(words.ordinal(id)?.index())?;
    seal([
        root.as_os_str().as_encoded_bytes(),
        &notes::encode(snapshot, ordinal),
        &words::encode(&cached),
    ])
}

/// The header, then `sections` as given, so a test can seal a section it built by hand;
/// `None` over `MAX_BYTES`.
pub(super) fn seal(sections: [&[u8]; SECTIONS]) -> Option<Vec<u8>> {
    seal_capped(sections, MAX_BYTES)
}

pub(super) fn seal_capped(sections: [&[u8]; SECTIONS], cap: u64) -> Option<Vec<u8>> {
    let mut lengths = Vec::new();
    varint::push(&mut lengths, narrow(sections.len()));
    for section in sections {
        varint::push(&mut lengths, narrow(section.len()));
    }
    let len = sections
        .iter()
        .fold(HEADER_BYTES + lengths.len(), |len, section| {
            len.saturating_add(section.len())
        });
    if !u64::try_from(len).is_ok_and(|len| len <= cap) {
        return None;
    }
    let mut bytes = Vec::with_capacity(len);
    bytes.extend_from_slice(&MAGIC);
    bytes.resize(HEADER_BYTES, 0);
    bytes.extend_from_slice(&lengths);
    for section in sections {
        bytes.extend_from_slice(section);
    }
    let (header, body) = bytes.split_at_mut(HEADER_BYTES);
    for (slot, field) in header[MAGIC.len()..]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(fields(body))
    {
        slot.copy_from_slice(&field.to_le_bytes());
    }
    Some(bytes)
}

/// Checks `bytes` as the cache file of the vault at `root`; no input panics.
///
/// # Errors
///
/// The first check that fails, in the header's order: a newer format is `Version`, not `Checksum`.
pub fn decode(bytes: &[u8], root: &Path) -> Result<Decoded, Rejected> {
    let Some((header, mut body)) = bytes.split_first_chunk::<HEADER_BYTES>() else {
        return Err(Rejected::Short);
    };
    if header[..MAGIC.len()] != MAGIC {
        return Err(Rejected::Corrupt("magic"));
    }
    let (found, _) = header[MAGIC.len()..].as_chunks::<4>();
    let found = found.iter().map(|&field| u32::from_le_bytes(field));
    let expected = fields(body);
    let [.., crc] = expected;
    let rejections = [
        Rejected::Version,
        Rejected::Fold,
        Rejected::Derivation,
        Rejected::Checksum,
    ];
    for ((found, expected), rejected) in found.zip(expected).zip(rejections) {
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
    let [stored_root, stored_notes, stored_words] = lengths.map(|length| {
        let (section, rest) = body.split_at(length);
        body = rest;
        section
    });
    if stored_root != root.as_os_str().as_encoded_bytes() {
        return Err(Rejected::Root);
    }
    let notes = notes::decode(stored_notes)?;
    let words = words::decode(stored_words, &notes)?;
    Ok(Decoded { notes, words, crc })
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
