use super::{bm25, idf};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn idf_matches_a_hand_computed_value() {
    // ln(1 + (2 - 1 + 0.5) / (1 + 0.5)) = ln 2
    assert!(close(idf(1, 2), std::f64::consts::LN_2), "{}", idf(1, 2));
    // ln(1 + (10 - 4 + 0.5) / (4 + 0.5)) = ln(11 / 4.5)
    assert!(close(idf(4, 10), (11.0_f64 / 4.5).ln()), "{}", idf(4, 10));
}

#[test]
fn idf_falls_as_more_notes_hold_the_term_and_stays_positive() {
    for df in 1..100 {
        assert!(idf(df + 1, 100) < idf(df, 100), "df {df}");
    }
    assert!(
        idf(100, 100) > 0.0,
        "a term in every note still weighs a little"
    );
}

#[test]
fn bm25_matches_hand_computed_values() {
    // Once, at the mean length: tf (k1 + 1) / (tf + k1) = 2.2 / 2.2, so the idf itself.
    assert!(
        close(bm25(1.5, 1, 10, 10.0), 1.5),
        "{}",
        bm25(1.5, 1, 10, 10.0)
    );
    // 2 · 2 · 2.2 / (2 + 1.2 · (0.25 + 0.75 · 20 / 10)) = 8.8 / 4.1
    assert!(
        close(bm25(2.0, 2, 20, 10.0), 8.8 / 4.1),
        "{}",
        bm25(2.0, 2, 20, 10.0)
    );
}

#[test]
fn bm25_at_length_0_keeps_only_1_minus_b() {
    // 1 · 1 · 2.2 / (1 + 1.2 · (1 − 0.75)) = 2.2 / 1.3
    assert!(
        close(bm25(1.0, 1, 0, 10.0), 2.2 / 1.3),
        "{}",
        bm25(1.0, 1, 0, 10.0)
    );
}

#[test]
fn bm25_is_zero_without_the_term_and_grows_with_it() {
    assert!(close(bm25(2.0, 0, 30, 10.0), 0.0));
    for tf in 0..300 {
        assert!(
            bm25(2.0, tf + 1, 300, 100.0) > bm25(2.0, tf, 300, 100.0),
            "tf {tf}"
        );
    }
}

#[test]
fn bm25_falls_as_the_note_grows() {
    for len in 1..300 {
        assert!(
            bm25(2.0, 3, len + 1, 100.0) < bm25(2.0, 3, len, 100.0),
            "len {len}"
        );
    }
}
