//! Section 1: each loaded note's stamp, identity and links, keyed by its vault path (P2).

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use foldhash::fast::RandomState;
use inkra_text::ByteOffset;
use inkra_vault::{EntryId, FileIdentity, Mtime, Stamp};

use super::Rejected;
use super::bytes::{len, narrow, put_blob, string, take};
use crate::varint;
use crate::words::Ordinal;
use crate::{IndexSnapshot, Source, SourceLink, Style};

/// A record's path length, flags, device, inode, ordinal and link count, at their smallest.
const MIN_RECORD: usize = 1 + 1 + 8 + 8 + 1 + 1;
/// A link's path length, flags, style, 2 range ends, context length, 2 context ends: a byte each.
const MIN_LINK_RECORD: usize = 8;
/// The flags bit of a record holding a stamp.
const STAMPED: u8 = 1;
/// The flag bits of a link holding a fragment, and of one written with `.md`; then every bit known.
const FRAGMENT: u8 = 0b01;
const MD: u8 = 0b10;
const KNOWN_LINK_FLAGS: u8 = 0b11;

/// A note as the cache holds it: the file its words came from, and its links.
#[derive(Debug, PartialEq, Eq)]
pub struct CachedNote {
    pub path: Arc<str>,
    /// `None` when the stamp cannot be written: the note always reloads.
    pub stamp: Option<Stamp>,
    pub identity: FileIdentity,
    /// Its words' ordinal in section 2; `None` when it has no words there.
    pub(crate) ordinal: Option<Ordinal>,
    pub source: Source,
}

/// Each loaded note of `snapshot`, in vault order, with its words' `ordinal` in section 2.
pub(super) fn encode(
    snapshot: &IndexSnapshot,
    ordinal: impl Fn(EntryId) -> Option<Ordinal>,
) -> Vec<u8> {
    let notes: Vec<_> = snapshot.loaded().collect();
    let mut out = Vec::new();
    let mut links = Vec::new();
    varint::push(&mut out, narrow(notes.len()));
    for (id, path, read, source) in notes {
        let Some(()) = put_blob(&mut out, path.as_bytes()) else {
            panic!("a note path of {} bytes", path.len());
        };
        links.clear();
        let since = read.stamp.mtime.0.duration_since(SystemTime::UNIX_EPOCH);
        match since {
            Ok(since) if put_links(&mut links, &source.links).is_some() => {
                out.push(STAMPED);
                out.extend_from_slice(&since.as_secs().to_le_bytes());
                out.extend_from_slice(&since.subsec_nanos().to_le_bytes());
                out.extend_from_slice(&read.stamp.size.to_le_bytes());
            }
            _ => {
                out.push(0);
                links.clear();
                varint::push(&mut links, 0);
            }
        }
        out.extend_from_slice(&read.identity.device.to_le_bytes());
        out.extend_from_slice(&read.identity.inode.to_le_bytes());
        varint::push(&mut out, ordinal(id).map_or(0, |ordinal| ordinal.get() + 1));
        out.extend_from_slice(&links);
    }
    out
}

/// `None` when a length or an offset exceeds `u32::MAX`; `out` then holds a part.
pub(super) fn put_links(out: &mut Vec<u8>, links: &[SourceLink]) -> Option<()> {
    varint::push(out, u32::try_from(links.len()).ok()?);
    for link in links {
        put_blob(out, link.path.as_bytes())?;
        out.push(link_flags(link.md, link.fragment.is_some()));
        if let Some(fragment) = &link.fragment {
            put_blob(out, fragment.as_bytes())?;
        }
        out.push(style_code(link.style));
        put_range(out, link.range.start.get()..link.range.end.get())?;
        put_blob(out, link.context.as_bytes())?;
        put_range(out, link.context_link.clone())?;
    }
    Some(())
}

fn put_range(out: &mut Vec<u8>, range: Range<usize>) -> Option<()> {
    for at in [range.start, range.end] {
        varint::push(out, u32::try_from(at).ok()?);
    }
    Some(())
}

fn link_flags(md: bool, fragment: bool) -> u8 {
    let bit = |set: bool, bit: u8| if set { bit } else { 0 };
    bit(md, MD) | bit(fragment, FRAGMENT)
}

pub(super) fn style_code(style: Style) -> u8 {
    match style {
        Style::Name => 0,
        Style::Suffix => 1,
        Style::Relative => 2,
    }
}

/// The inverse of `style_code`; `None` for a code no build writes.
pub(super) fn style_from_code(code: u8) -> Option<Style> {
    match code {
        0 => Some(Style::Name),
        1 => Some(Style::Suffix),
        2 => Some(Style::Relative),
        _ => None,
    }
}

/// Every field checked before it is used, and the count before it allocates.
pub(super) fn decode(mut section: &[u8]) -> Result<Vec<CachedNote>, Rejected> {
    let bytes = &mut section;
    let count = len(bytes)?;
    if count
        .checked_mul(MIN_RECORD)
        .is_none_or(|least| least > bytes.len())
    {
        return Err(Rejected::Corrupt("count"));
    }
    let mut notes = Vec::with_capacity(count);
    let mut paths = HashSet::with_capacity_and_hasher(count, RandomState::default());
    // Each link path as written, shared by every link of this decode that writes it.
    let mut written = HashMap::with_hasher(RandomState::default());
    for _ in 0..count {
        let note = record(bytes, &mut written)?;
        if !paths.insert(Arc::clone(&note.path)) {
            return Err(Rejected::Corrupt("duplicate"));
        }
        notes.push(note);
    }
    if !bytes.is_empty() {
        return Err(Rejected::Corrupt("trailing"));
    }
    Ok(notes)
}

fn record<'a>(
    bytes: &mut &'a [u8],
    written: &mut HashMap<&'a str, Arc<str>, RandomState>,
) -> Result<CachedNote, Rejected> {
    let path = Arc::from(string(bytes)?);
    let flags = byte(bytes)?;
    if flags & !STAMPED != 0 {
        return Err(Rejected::Corrupt("flags"));
    }
    let stamp = if flags == STAMPED {
        Some(stamp(bytes)?)
    } else {
        None
    };
    let device = u64::from_le_bytes(take(bytes)?);
    let inode = u64::from_le_bytes(take(bytes)?);
    let ordinal = varint::read_checked(bytes)
        .ok_or(Rejected::Corrupt("varint"))?
        .checked_sub(1)
        .map(Ordinal::new);
    let count = len(bytes)?;
    if count
        .checked_mul(MIN_LINK_RECORD)
        .is_none_or(|least| least > bytes.len())
    {
        return Err(Rejected::Corrupt("links"));
    }
    let mut links = Vec::with_capacity(count);
    for _ in 0..count {
        links.push(link(bytes, written)?);
    }
    Ok(CachedNote {
        path,
        stamp,
        identity: FileIdentity { device, inode },
        ordinal,
        source: Source {
            links: Arc::from(links),
        },
    })
}

fn stamp(bytes: &mut &[u8]) -> Result<Stamp, Rejected> {
    let secs = u64::from_le_bytes(take(bytes)?);
    let nanos = u32::from_le_bytes(take(bytes)?);
    let size = u64::from_le_bytes(take(bytes)?);
    if nanos >= 1_000_000_000 {
        return Err(Rejected::Corrupt("nanos"));
    }
    let mtime = SystemTime::UNIX_EPOCH
        .checked_add(Duration::new(secs, nanos))
        .ok_or(Rejected::Corrupt("mtime"))?;
    Ok(Stamp {
        mtime: Mtime(mtime),
        size,
    })
}

fn link<'a>(
    bytes: &mut &'a [u8],
    written: &mut HashMap<&'a str, Arc<str>, RandomState>,
) -> Result<SourceLink, Rejected> {
    let path = string(bytes)?;
    let path = Arc::clone(written.entry(path).or_insert_with(|| Arc::from(path)));
    let flags = byte(bytes)?;
    if flags & !KNOWN_LINK_FLAGS != 0 {
        return Err(Rejected::Corrupt("flags"));
    }
    let fragment = if flags & FRAGMENT == 0 {
        None
    } else {
        Some(Box::from(string(bytes)?))
    };
    let style = style_from_code(byte(bytes)?).ok_or(Rejected::Corrupt("style"))?;
    let within_note = range(bytes)?;
    let context = string(bytes)?.to_owned();
    let context_link = range(bytes)?;
    if !context.is_char_boundary(context_link.start) || !context.is_char_boundary(context_link.end)
    {
        return Err(Rejected::Corrupt("context"));
    }
    Ok(SourceLink {
        path,
        fragment,
        md: flags & MD != 0,
        style,
        range: ByteOffset::new(within_note.start)..ByteOffset::new(within_note.end),
        context,
        context_link,
    })
}

fn range(bytes: &mut &[u8]) -> Result<Range<usize>, Rejected> {
    let (start, end) = (len(bytes)?, len(bytes)?);
    if start > end {
        return Err(Rejected::Corrupt("range"));
    }
    Ok(start..end)
}

fn byte(bytes: &mut &[u8]) -> Result<u8, Rejected> {
    take(bytes).map(|[byte]| byte)
}
