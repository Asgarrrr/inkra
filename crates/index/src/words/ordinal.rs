use std::iter;

use super::wide;

/// A note's dense number in the word index; never reused, so a dead posting names no later note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Ordinal(u32);

impl Ordinal {
    pub(crate) const fn new(n: u32) -> Self {
        Self(n)
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }

    pub(crate) fn index(self) -> usize {
        wide(self.0)
    }

    fn word(self) -> usize {
        self.index() / 64
    }

    fn bit(self) -> u64 {
        1 << (self.0 % 64)
    }
}

/// The ordinals whose segment postings are live, one bit each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Alive {
    bits: Vec<u64>,
    len: usize,
}

impl Alive {
    /// The ordinals whose bit is set in `bits`, ordinal `n` at bit `n % 64` of word `n / 64`.
    pub(crate) fn from_bits(bits: Vec<u64>) -> Self {
        let len = bits.iter().map(|word| wide(word.count_ones())).sum();
        Self { bits, len }
    }

    pub(crate) fn bits(&self) -> &[u64] {
        &self.bits
    }

    pub(crate) fn contains(&self, ordinal: Ordinal) -> bool {
        self.bits
            .get(ordinal.word())
            .is_some_and(|word| word & ordinal.bit() != 0)
    }

    pub(crate) fn insert(&mut self, ordinal: Ordinal) {
        let at = ordinal.word();
        if at >= self.bits.len() {
            self.bits.resize(at + 1, 0);
        }
        if self.bits[at] & ordinal.bit() == 0 {
            self.bits[at] |= ordinal.bit();
            self.len += 1;
        }
    }

    pub(super) fn remove(&mut self, ordinal: Ordinal) {
        if let Some(word) = self.bits.get_mut(ordinal.word())
            && *word & ordinal.bit() != 0
        {
            *word &= !ordinal.bit();
            self.len -= 1;
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// The set ordinals, ascending.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Ordinal> + '_ {
        self.bits.iter().zip(0_u32..).flat_map(|(&word, at)| {
            let mut rest = word;
            iter::from_fn(move || {
                if rest == 0 {
                    return None;
                }
                let bit = rest.trailing_zeros();
                rest &= rest - 1;
                Some(Ordinal(at * 64 + bit))
            })
        })
    }

    pub(super) fn heap_bytes(&self) -> usize {
        size_of_val(&*self.bits)
    }
}

impl FromIterator<Ordinal> for Alive {
    fn from_iter<I: IntoIterator<Item = Ordinal>>(ordinals: I) -> Self {
        let mut alive = Self::default();
        for ordinal in ordinals {
            alive.insert(ordinal);
        }
        alive
    }
}

#[cfg(test)]
mod tests;
