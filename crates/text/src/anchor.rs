use crate::offset::ByteOffset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}

/// A handle to a position the creating buffer keeps up to date across edits.
/// Not `PartialOrd`/`Ord`: the index is creation order, not document order; comparing needs resolving first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Anchor(pub(crate) usize);

pub(crate) struct AnchorState {
    pub(crate) offset: ByteOffset,
    pub(crate) bias: Bias,
}

pub(crate) fn transform(
    offset: ByteOffset,
    bias: Bias,
    at: ByteOffset,
    removed_len: usize,
    inserted_len: usize,
) -> ByteOffset {
    let touched_end = at.0 + removed_len;
    if offset.0 < at.0 {
        offset
    } else if offset.0 == at.0 || (removed_len > 0 && offset.0 < touched_end) {
        match bias {
            Bias::Left => at,
            Bias::Right => ByteOffset(at.0 + inserted_len),
        }
    } else {
        // No underflow: this branch only runs where offset.0 >= touched_end >= removed_len.
        ByteOffset(offset.0 - removed_len + inserted_len)
    }
}

#[cfg(test)]
mod tests;
