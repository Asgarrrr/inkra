//! LEB128: seven bits per byte, low first, the top bit set on every byte but the last.

/// Appends `value` in one to five bytes.
pub(crate) fn push(out: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        out.push(value.to_le_bytes()[0] | 0x80);
        value >>= 7;
    }
    out.push(value.to_le_bytes()[0]);
}

/// Reads one value off the front of untrusted `bytes`: `None` when it is truncated, runs past
/// five bytes, or exceeds `u32::MAX`.
pub(crate) fn read_checked(bytes: &mut &[u8]) -> Option<u32> {
    let mut value = 0;
    for shift in (0..32).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        if shift == 28 && byte > 0x0F {
            return None;
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some(value);
        }
    }
    None
}

/// Reads one value off the front of `bytes`. A truncated value is a caller bug.
pub(super) fn read(bytes: &mut &[u8]) -> u32 {
    let mut value = 0;
    let mut shift = 0;
    loop {
        let Some((&byte, rest)) = bytes.split_first() else {
            panic!("a varint ends past its bytes");
        };
        *bytes = rest;
        value |= u32::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return value;
        }
        shift += 7;
    }
}

#[cfg(test)]
mod tests;
