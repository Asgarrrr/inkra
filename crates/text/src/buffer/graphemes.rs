use unicode_segmentation::GraphemeCursor;

use crate::anchor::Bias;
use crate::offset::ByteOffset;

use super::Buffer;

impl Buffer {
    /// The last grapheme start before `offset`, or 0. Cost is local to `offset`.
    #[must_use]
    pub fn previous_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        let offset = offset.get().min(self.text.len());
        let floor = (0..=offset)
            .rev()
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(0);
        let mut cursor = GraphemeCursor::new(floor, self.text.len(), true);
        if floor < offset && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset::new(floor);
        }
        ByteOffset::new(
            cursor
                .prev_boundary(&self.text, 0)
                .ok()
                .flatten()
                .unwrap_or(0),
        )
    }

    /// The first grapheme start after `offset`, or the end. Cost is local to `offset`.
    #[must_use]
    pub fn next_grapheme(&self, offset: ByteOffset) -> ByteOffset {
        let len = self.text.len();
        if offset.get() >= len {
            return self.len();
        }
        let ceil = (offset.get()..=len)
            .find(|&i| self.text.is_char_boundary(i))
            .unwrap_or(len);
        let mut cursor = GraphemeCursor::new(ceil, len, true);
        if ceil > offset.get() && cursor.is_boundary(&self.text, 0).unwrap_or(false) {
            return ByteOffset::new(ceil);
        }
        ByteOffset::new(
            cursor
                .next_boundary(&self.text, 0)
                .ok()
                .flatten()
                .unwrap_or(len),
        )
    }

    #[must_use]
    pub fn clip(&self, offset: ByteOffset) -> ByteOffset {
        let mut index = offset.get().min(self.text.len());
        while !self.text.is_char_boundary(index) {
            index -= 1;
        }
        ByteOffset::new(index)
    }

    /// `clip`, then onto the grapheme boundary before (`Left`) or after (`Right`) it.
    #[must_use]
    pub fn clip_grapheme(&self, offset: ByteOffset, bias: Bias) -> ByteOffset {
        let offset = self.clip(offset).get();
        let len = self.text.len();
        let mut cursor = GraphemeCursor::new(offset, len, true);
        if cursor.is_boundary(&self.text, 0).unwrap_or(true) {
            return ByteOffset::new(offset);
        }
        let snapped = match bias {
            Bias::Left => cursor.prev_boundary(&self.text, 0).ok().flatten(),
            Bias::Right => cursor.next_boundary(&self.text, 0).ok().flatten(),
        };
        ByteOffset::new(snapped.unwrap_or(offset))
    }
}
