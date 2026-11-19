use gpui::px;

use super::widest_digit;

/// `1` advances widest, and no digit after it as wide, so neither the last digit nor the widest
/// `x` wins.
#[test]
fn the_widest_digit_has_the_largest_advance() {
    let starts = [0., 5., 15., 20., 25., 30., 35., 40., 45., 50., 55.].map(px);
    assert_eq!(widest_digit(&starts), '1');
}
