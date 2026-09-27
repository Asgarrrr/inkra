use proptest::prelude::*;

use super::{AFTER, BEFORE, around};

fn window<'a>(text: &'a str, hit: &str) -> &'a str {
    let Some(start) = text.find(hit) else {
        panic!("{hit:?} in {text:?}");
    };
    &text[around(text, start..start + hit.len())]
}

#[test]
fn keeps_60_bytes_before_and_100_after() {
    let text = format!("{}HIT{}", "b".repeat(70), "a".repeat(120));
    assert_eq!(
        window(&text, "HIT"),
        format!("{}HIT{}", "b".repeat(60), "a".repeat(100))
    );
}

#[test]
fn cuts_on_char_boundaries() {
    // 61 bytes before the hit start mid-`é`; 100 after end mid-`é`.
    let text = format!("é{}HIT{}é", "b".repeat(59), "a".repeat(99));
    assert_eq!(
        window(&text, "HIT"),
        format!("{}HIT{}", "b".repeat(59), "a".repeat(99))
    );
    let text = format!("{}HIT{}", "🎉".repeat(20), "ω".repeat(60));
    assert_eq!(
        window(&text, "HIT"),
        format!("{}HIT{}", "🎉".repeat(15), "ω".repeat(50))
    );
}

#[test]
fn stays_within_the_line() {
    assert_eq!(window("first\nsay hit here\nlast", "hit"), "say hit here");
    // The newlines sit 60 bytes before the hit and 99 after it.
    let text = format!("x\n{}HIT{}\ny", "b".repeat(59), "a".repeat(99));
    assert_eq!(
        window(&text, "HIT"),
        format!("{}HIT{}", "b".repeat(59), "a".repeat(99))
    );
}

#[test]
fn a_hit_on_a_line_edge_keeps_the_edge() {
    assert_eq!(window("hit rest\nnext", "hit"), "hit rest");
    assert_eq!(window("before\nlast hit", "hit"), "last hit");
    assert_eq!(window("\nhit\n", "hit"), "hit");
}

#[test]
fn a_window_is_never_shorter_than_its_hit() {
    let long = "x".repeat(300);
    assert_eq!(window(&long, &long), long);
}

#[test]
fn a_ten_megabyte_line_gives_a_short_window() {
    let half = "a".repeat(5 << 20);
    let text = format!("{half}HIT{half}");
    assert_eq!(window(&text, "HIT").len(), BEFORE + 3 + AFTER);
}

proptest! {
    /// Bytes outside the hit's 60 before and 100 after never change the window, so none is read.
    #[test]
    fn the_window_reads_only_its_bytes(
        text in "[a-cé\n🎉]{0,300}",
        at in any::<prop::sample::Index>(),
        len in 0usize..20,
    ) {
        let start = text.floor_char_boundary(at.index(text.len() + 1));
        let end = text.floor_char_boundary(start + len);
        let lo = text.floor_char_boundary(start.saturating_sub(BEFORE));
        let hi = text.ceil_char_boundary(end + AFTER);
        let window = around(&text[lo..hi], start - lo..end - lo);
        prop_assert_eq!(around(&text, start..end), window.start + lo..window.end + lo);
    }
}
