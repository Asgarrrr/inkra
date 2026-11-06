//! Section 2: the words segment, its alive bits, `next` and `total` (P2).

use super::Rejected;
use super::header::narrow;
use super::notes::{CachedNote, blob, len, string, take};
use crate::words::{Alive, Ordinal, Segment, WordsSnapshot, varint, wide};

/// The words a cache file holds, checked so any lookup or merge over them is safe.
#[derive(Debug)]
pub struct CachedWords {
    pub(crate) segment: Segment,
    pub(crate) alive: Alive,
    pub(crate) next: u32,
    pub(crate) total: u64,
}

/// `words` with its fresh notes merged in, renumbered in order when over half its ordinals are
/// dead; and per ordinal of `words`, the one the file holds, `None` for a dead one.
pub(super) fn cached(words: &WordsSnapshot) -> (CachedWords, Vec<Option<Ordinal>>) {
    let (segment, live) = words.merged();
    let compact = wide(words.next) > 2 * live.len();
    let mut to = Vec::new();
    for (rank, old) in (0..).zip(live.iter()) {
        to.resize(old.index() + 1, None);
        to[old.index()] = Some(if compact { Ordinal::new(rank) } else { old });
    }
    let cached = if compact {
        CachedWords {
            segment: segment.renumbered(&to),
            alive: to.iter().flatten().copied().collect(),
            next: narrow(live.len()),
            total: words.total,
        }
    } else {
        CachedWords {
            segment,
            alive: live,
            next: words.next,
            total: words.total,
        }
    };
    (cached, to)
}

pub(super) fn encode(words: &CachedWords) -> Vec<u8> {
    let parts = words.segment.parts();
    let mut out = Vec::new();
    varint::push(&mut out, words.next);
    out.extend_from_slice(&words.total.to_le_bytes());
    varint::push(&mut out, words.next);
    let bits = words.alive.bits();
    for at in 0..wide(words.next).div_ceil(64) {
        let word = bits.get(at).copied().unwrap_or(0);
        out.extend_from_slice(&word.to_le_bytes());
    }
    varint::push(&mut out, narrow(parts.notes));
    put_blob(&mut out, parts.bytes.as_bytes());
    put_u32s(&mut out, parts.bounds);
    put_u32s(&mut out, parts.starts);
    put_blob(&mut out, parts.postings);
    put_u32s(&mut out, parts.lengths);
    out
}

fn put_blob(out: &mut Vec<u8>, blob: &[u8]) {
    varint::push(out, narrow(blob.len()));
    out.extend_from_slice(blob);
}

fn put_u32s(out: &mut Vec<u8>, values: &[u32]) {
    varint::push(out, narrow(values.len()));
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

/// Every rule `Segment` and `Words` rely on, checked before any is built; `notes` is section 1.
pub(super) fn decode(mut section: &[u8], notes: &[CachedNote]) -> Result<CachedWords, Rejected> {
    let bytes = &mut section;
    let next = varint::read_checked(bytes).ok_or(Rejected::Corrupt("varint"))?;
    let total = u64::from_le_bytes(take(bytes)?);
    let alive = alive(bytes, next)?;
    let held_notes = len(bytes)?;
    let text = string(bytes)?;
    let bounds = u32s(bytes)?;
    check_bounds(text, &bounds)?;
    let starts = u32s(bytes)?;
    let postings = blob(bytes)?;
    check_starts(&starts, bounds.len(), postings.len())?;
    let held = held(postings, &starts, next)?;
    if held.len() != held_notes {
        return Err(Rejected::Corrupt("notes"));
    }
    if !alive.iter().all(|ordinal| held.contains(ordinal)) {
        return Err(Rejected::Corrupt("unheld"));
    }
    let lengths = u32s(bytes)?;
    if lengths.len() > wide(next) {
        return Err(Rejected::Corrupt("lengths"));
    }
    if !bytes.is_empty() {
        return Err(Rejected::Corrupt("trailing"));
    }
    named(notes, &alive)?;
    let live = alive.iter().try_fold(0_u64, |sum, ordinal| {
        sum.checked_add(u64::from(
            lengths.get(ordinal.index()).copied().unwrap_or(0),
        ))
    });
    if live != Some(total) {
        return Err(Rejected::Corrupt("total"));
    }
    Ok(CachedWords {
        segment: Segment::from_parts(
            text.to_owned(),
            bounds,
            starts,
            postings.to_vec(),
            held_notes,
            lengths,
        ),
        alive,
        next,
        total,
    })
}

/// A bit length equal to `next`, then its words, every bit past it clear.
fn alive(bytes: &mut &[u8], next: u32) -> Result<Alive, Rejected> {
    let bits = varint::read_checked(bytes).ok_or(Rejected::Corrupt("varint"))?;
    if bits != next {
        return Err(Rejected::Corrupt("alive"));
    }
    let (words, rest) = bytes
        .split_at_checked(wide(bits).div_ceil(64) * 8)
        .ok_or(Rejected::Corrupt("truncated"))?;
    *bytes = rest;
    let (words, _) = words.as_chunks::<8>();
    let words: Vec<u64> = words.iter().map(|&word| u64::from_le_bytes(word)).collect();
    if bits % 64 != 0 && words.last().is_some_and(|last| last >> (bits % 64) != 0) {
        return Err(Rejected::Corrupt("alive"));
    }
    Ok(Alive::from_bits(words))
}

/// Word `w` is `text[bounds[w]..bounds[w + 1]]`: each one nonempty, on char boundaries, and
/// sorted after the one before.
fn check_bounds(text: &str, bounds: &[u32]) -> Result<(), Rejected> {
    if bounds.first() != Some(&0) || bounds.last().map(|&last| wide(last)) != Some(text.len()) {
        return Err(Rejected::Corrupt("bounds"));
    }
    let mut before: Option<&str> = None;
    for pair in bounds.windows(2) {
        let (start, end) = (wide(pair[0]), wide(pair[1]));
        if start >= end || !text.is_char_boundary(end) {
            return Err(Rejected::Corrupt("bounds"));
        }
        let word = &text[start..end];
        if before.is_some_and(|before| before >= word) {
            return Err(Rejected::Corrupt("sorted"));
        }
        before = Some(word);
    }
    Ok(())
}

/// One posting list per word, each nonempty, together covering `postings` from 0 to its end.
fn check_starts(starts: &[u32], bounds: usize, postings: usize) -> Result<(), Rejected> {
    let increasing = starts.windows(2).all(|pair| pair[0] < pair[1]);
    if starts.len() != bounds
        || starts.first() != Some(&0)
        || starts.last().map(|&last| wide(last)) != Some(postings)
        || !increasing
    {
        return Err(Rejected::Corrupt("starts"));
    }
    Ok(())
}

/// Walks each posting list as `Postings` reads it; the distinct ordinals they hold.
fn held(postings: &[u8], starts: &[u32], next: u32) -> Result<Alive, Rejected> {
    let mut held = Alive::from_bits(vec![0; wide(next).div_ceil(64)]);
    for pair in starts.windows(2) {
        let mut run = &postings[wide(pair[0])..wide(pair[1])];
        let mut last = None;
        while !run.is_empty() {
            let gap = varint::read_checked(&mut run).ok_or(Rejected::Corrupt("varint"))?;
            let count = varint::read_checked(&mut run).ok_or(Rejected::Corrupt("varint"))?;
            let ordinal = match last {
                None => gap,
                Some(_) if gap == 0 => return Err(Rejected::Corrupt("gap")),
                Some(last) => gap.checked_add(last).ok_or(Rejected::Corrupt("overflow"))?,
            };
            if ordinal >= next {
                return Err(Rejected::Corrupt("ordinal"));
            }
            if count == 0 {
                return Err(Rejected::Corrupt("frequency"));
            }
            held.insert(Ordinal::new(ordinal));
            last = Some(ordinal);
        }
    }
    Ok(held)
}

/// Each alive ordinal is one record's, and each record's ordinal is alive.
fn named(notes: &[CachedNote], alive: &Alive) -> Result<(), Rejected> {
    let mut named = Alive::from_bits(vec![0; alive.bits().len()]);
    for ordinal in notes.iter().filter_map(|note| note.ordinal) {
        if !alive.contains(ordinal) {
            return Err(Rejected::Corrupt("dead"));
        }
        if named.contains(ordinal) {
            return Err(Rejected::Corrupt("shared"));
        }
        named.insert(ordinal);
    }
    if named.len() != alive.len() {
        return Err(Rejected::Corrupt("unnamed"));
    }
    Ok(())
}

/// A varint count, then that many u32 LE, the count checked against the bytes left.
fn u32s(bytes: &mut &[u8]) -> Result<Vec<u32>, Rejected> {
    let count = len(bytes)?;
    let need = count.checked_mul(4).ok_or(Rejected::Corrupt("truncated"))?;
    let (values, rest) = bytes
        .split_at_checked(need)
        .ok_or(Rejected::Corrupt("truncated"))?;
    *bytes = rest;
    let (values, _) = values.as_chunks::<4>();
    Ok(values
        .iter()
        .map(|&value| u32::from_le_bytes(value))
        .collect())
}
