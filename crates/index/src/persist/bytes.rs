//! The byte primitives the cache's sections share: varint lengths, blobs, fixed-size fields.

use std::str;

use super::Rejected;
use crate::varint;

/// A varint length, then `blob`; `None` over `u32::MAX` bytes, `out` then unchanged.
pub(super) fn put_blob(out: &mut Vec<u8>, blob: &[u8]) -> Option<()> {
    varint::push(out, u32::try_from(blob.len()).ok()?);
    out.extend_from_slice(blob);
    Some(())
}

pub(super) fn narrow(len: usize) -> u32 {
    let Ok(len) = u32::try_from(len) else {
        panic!("an index cache length of {len}, over u32::MAX");
    };
    len
}

pub(super) fn string<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, Rejected> {
    str::from_utf8(blob(bytes)?).map_err(|_| Rejected::Corrupt("utf8"))
}

/// A varint length, then that many bytes.
pub(super) fn blob<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8], Rejected> {
    let len = len(bytes)?;
    let (blob, rest) = bytes
        .split_at_checked(len)
        .ok_or(Rejected::Corrupt("truncated"))?;
    *bytes = rest;
    Ok(blob)
}

pub(super) fn len(bytes: &mut &[u8]) -> Result<usize, Rejected> {
    varint::read_checked(bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(Rejected::Corrupt("varint"))
}

pub(super) fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], Rejected> {
    let (taken, rest) = bytes
        .split_first_chunk::<N>()
        .ok_or(Rejected::Corrupt("truncated"))?;
    *bytes = rest;
    Ok(*taken)
}
