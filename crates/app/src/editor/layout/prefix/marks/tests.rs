use gpui::{Pixels, px};
use inkra_markdown::WidthKey;

use super::{Marks, standard_slot};
use crate::theme::LIST_SLOT_GAP;

/// Glyph advances where `7` is widest, and no digit after it as wide.
fn advances(text: &str) -> Vec<Pixels> {
    let advance = |glyph| px(if glyph == '7' { 20. } else { 5. });
    text.chars().map(advance).collect()
}

/// A number slot measures its widest digit, not the last nor the first, repeated per digit.
#[test]
fn a_number_slot_holds_its_widest_digits_and_the_gap() {
    let mut marks = Marks::default();
    let two = WidthKey {
        digits: 2,
        delimiter: b'.',
    };
    let slot = marks.slot(two, &mut advances);
    assert_eq!(slot, px(20. + 20. + 5.) + LIST_SLOT_GAP);
    assert!(slot > standard_slot());
}
