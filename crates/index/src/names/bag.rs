/// Which chars a string holds, hashed to 64 bits. A collision only lets a candidate through
/// to the exact check, so the filter never rejects a match.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CharBag(u64);

impl CharBag {
    pub(super) fn insert(&mut self, c: char) {
        self.0 |= 1 << slot(c);
    }

    pub(super) fn contains_all(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

fn slot(c: char) -> u32 {
    match c {
        'a'..='z' => c as u32 - 'a' as u32,
        '0'..='9' => 26 + (c as u32 - '0' as u32),
        _ => 36 + c as u32 % 28,
    }
}

#[cfg(test)]
mod tests;
