use crate::offset::{ByteOffset, CharOffset};

pub struct Buffer {
    text: String,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
}

impl Buffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    #[must_use]
    pub fn byte_offset(&self, char_offset: CharOffset) -> ByteOffset {
        debug_assert!(
            char_offset.0 <= self.char_len(),
            "char offset {} out of bounds (buffer has {} chars)",
            char_offset.0,
            self.char_len()
        );
        let byte = self
            .text
            .char_indices()
            .nth(char_offset.0)
            .map_or(self.text.len(), |(b, _)| b);
        ByteOffset(byte)
    }

    fn snapshot_before_edit(&mut self) {
        self.undo_stack.push(self.text.clone());
        self.redo_stack.clear();
    }

    pub fn insert(&mut self, at: CharOffset, s: &str) {
        self.snapshot_before_edit();
        let byte = self.byte_offset(at);
        self.text.insert_str(byte.0, s);
    }

    pub fn delete(&mut self, start: CharOffset, end: CharOffset) {
        debug_assert!(start <= end, "delete range start must not exceed end");
        self.snapshot_before_edit();
        let start_byte = self.byte_offset(start);
        let end_byte = self.byte_offset(end);
        self.text.replace_range(start_byte.0..end_byte.0, "");
    }

    #[must_use]
    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(prev) => {
                self.redo_stack
                    .push(std::mem::replace(&mut self.text, prev));
                true
            }
            None => false,
        }
    }

    #[must_use]
    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(next) => {
                self.undo_stack
                    .push(std::mem::replace(&mut self.text, next));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offset::CharOffset;

    #[test]
    fn new_buffer_holds_initial_text() {
        let buf = Buffer::new("hello");
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn insert_at_char_offset_splits_on_char_boundary_not_byte_boundary() {
        // "héllo": h(0) é(1) l(2) l(3) o(4) — 'é' is 2 bytes, 1 char.
        let mut buf = Buffer::new("héllo");
        buf.insert(CharOffset(2), "X");
        assert_eq!(buf.text(), "héXllo");
    }

    #[test]
    fn insert_at_end_appends() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn delete_range_removes_chars_by_char_offset() {
        let mut buf = Buffer::new("héllo");
        buf.delete(CharOffset(1), CharOffset(3)); // removes "él"
        assert_eq!(buf.text(), "hlo");
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn byte_offset_panics_on_out_of_range_char_offset() {
        let buf = Buffer::new("abc");
        let _ = buf.byte_offset(CharOffset(999));
    }

    #[test]
    fn undo_reverts_last_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        assert_eq!(buf.text(), "abcd");
        assert!(buf.undo());
        assert_eq!(buf.text(), "abc");
    }

    #[test]
    fn undo_with_empty_history_returns_false() {
        let mut buf = Buffer::new("abc");
        assert!(!buf.undo());
    }

    #[test]
    fn redo_reapplies_undone_edit() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        let _ = buf.undo();
        assert!(buf.redo());
        assert_eq!(buf.text(), "abcd");
    }

    #[test]
    fn new_edit_after_undo_clears_redo_stack() {
        let mut buf = Buffer::new("abc");
        buf.insert(CharOffset(3), "d");
        let _ = buf.undo();
        buf.insert(CharOffset(0), "X");
        assert!(!buf.redo());
    }
}
