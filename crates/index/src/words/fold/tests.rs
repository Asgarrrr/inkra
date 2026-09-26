use std::collections::HashMap;
use std::iter;

use regex_syntax::hir::{Class, Hir, HirKind};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use super::{
    DIACRITICS, cap, fold, is_diacritic, is_word, orbit_representative, representative, word_spans,
};

/// Uppercase letters std lowercases and `regex-syntax` 0.8.11 finds caseless (Unicode 17).
const SKEW: usize = 28;

/// Chars `fold` drops as diacritics under `regex-syntax` 0.8.11's Unicode 16 tables.
const DIACRITIC_COUNT: usize = 705;

fn split(text: &str) -> Vec<(usize, &str)> {
    word_spans(text).collect()
}

#[test]
fn a_word_is_a_maximal_run_of_word_chars() {
    assert_eq!(split("l'été"), [(0, "l"), (2, "été")]);
    assert_eq!(split("2026-09"), [(0, "2026"), (5, "09")]);
    assert_eq!(split("snake_case"), [(0, "snake_case")]);
    assert_eq!(split("  a  b "), [(2, "a"), (5, "b")]);
    assert_eq!(split(""), []);
    assert_eq!(split("—!?"), []);
}

#[test]
fn apostrophes_straight_and_typographic_split_words() {
    assert_eq!(split("aujourd’hui"), [(0, "aujourd"), (10, "hui")]);
    assert_eq!(split("aujourd'hui"), [(0, "aujourd"), (8, "hui")]);
}

#[test]
fn a_combining_mark_stays_inside_its_word() {
    assert_eq!(split("e\u{301}te x"), [(0, "e\u{301}te"), (6, "x")]);
}

#[test]
fn an_emoji_is_not_a_word_char() {
    assert_eq!(split("a🎉b"), [(0, "a"), (5, "b")]);
}

#[test]
fn case_and_accents_fold_both_ways() {
    for word in ["Été", "ÉTÉ", "ete", "e\u{301}te", "été"] {
        assert_eq!(fold(word), "ete", "{word:?}");
    }
}

#[test]
fn diacritics_fold_away() {
    assert_eq!(fold("Nguyễn"), "nguyen");
    assert_eq!(fold("ёлка"), "елка");
    assert_eq!(fold("كَتَبَ"), "كتب", "Arabic harakat");
    assert_eq!(fold("أحمد"), "احمد", "Arabic hamza above");
    assert_eq!(fold("שָׁלוֹם"), "שלום", "Hebrew points");
}

#[test]
fn vowel_signs_and_tone_marks_stay() {
    assert_ne!(fold("किताब"), fold("कातिब"), "Devanagari vowel signs");
    assert_ne!(fold("คน"), fold("คืน"), "Thai vowel U+0E37");
    assert_ne!(fold("ไม่"), fold("ไม้"), "Thai tone marks");
    assert_ne!(fold("が"), fold("か"), "kana voicing mark");
}

/// Every char of `pattern`'s class, which must parse to a Unicode class.
fn class_chars(pattern: &str) -> Vec<char> {
    let Ok(hir) = regex_syntax::parse(pattern) else {
        panic!("{pattern:?} parses");
    };
    let HirKind::Class(Class::Unicode(class)) = hir.kind() else {
        panic!("{pattern:?} is a Unicode class");
    };
    class
        .ranges()
        .iter()
        .flat_map(|range| range.start()..=range.end())
        .collect()
}

#[test]
fn no_thai_or_devanagari_char_is_dropped() {
    let dropped: Vec<char> = class_chars(r"[\p{Script=Thai}\p{Script=Devanagari}]")
        .into_iter()
        .filter(|&c| {
            !fold(c.encode_utf8(&mut [0; 4]))
                .chars()
                .eq(iter::once(c).nfd())
        })
        .collect();
    assert_eq!(dropped, []);
}

#[test]
fn the_diacritic_table_has_the_known_size() {
    let count: usize = DIACRITICS
        .iter()
        .map(|&(start, end)| (start..=end).count())
        .sum();
    assert_eq!(
        count, DIACRITIC_COUNT,
        "regex-syntax's Unicode version changed; recheck which marks fold drops"
    );
}

#[test]
fn a_word_with_marks_folds_idempotently() {
    for word in [
        "किताब",
        "कातिब",
        "คืน",
        "ไม่",
        "が",
        "Nguyễn",
        "كَتَبَ",
        "أحمد",
        "שָׁלוֹם",
        "ёлка",
    ] {
        let once = fold(word).into_owned();
        assert_eq!(fold(&once), once, "{word:?}");
    }
}

#[test]
fn a_dropped_mark_between_kept_marks_leaves_them_in_canonical_order() {
    for (with_cgj, without) in [
        ("\u{915}\u{94D}\u{34F}\u{93C}", "\u{915}\u{94D}\u{93C}"),
        ("\u{E01}\u{E48}\u{34F}\u{E38}", "\u{E01}\u{E48}\u{E38}"),
        (
            "\u{915}\u{94D}\u{34F}\u{FE0F}\u{93C}",
            "\u{915}\u{94D}\u{93C}",
        ),
    ] {
        let once = fold(with_cgj).into_owned();
        assert_eq!(fold(&once), once, "{with_cgj:?}");
        assert_eq!(once, fold(without), "{with_cgj:?}");
    }
}

#[test]
fn special_letters_fold_as_the_spec_says() {
    assert_eq!(fold("İstanbul"), "istanbul");
    assert_eq!(fold("ı"), "ı", "dotless i is its own letter");
    assert_eq!(fold("\u{212A}elvin"), "kelvin");
    assert_eq!(fold("Straße"), "straße", "no compatibility folding");
    assert_eq!(fold("ﬁn"), "ﬁn");
    assert_eq!(
        fold("ǅ"),
        "ǆ",
        "a compatibility decomposition is not applied"
    );
    assert_eq!(
        fold("한"),
        "\u{1112}\u{1161}\u{11AB}",
        "NFD can lengthen a word"
    );
}

#[test]
fn case_variants_fold_by_simple_case_folding() {
    assert_eq!(fold("ſ"), "s");
    assert_eq!(fold("ΟΔΥΣΣΕΥΣ"), "οδυσσευσ");
    assert_eq!(fold("οδυσσευς"), "οδυσσευσ");
    assert_eq!(fold("ϐ"), fold("β"));
    assert_eq!(fold("µ"), fold("μ"), "micro sign and mu");
    assert_eq!(fold("ẞ"), "ß");
    assert_eq!(fold("漢字"), "漢字");
}

/// The Hir `regex-syntax` parses `(?i)x` into.
fn case_insensitive(x: char) -> Hir {
    let pattern = format!("(?i){}", regex::escape(x.encode_utf8(&mut [0; 4])));
    let Ok(hir) = regex_syntax::parse(&pattern) else {
        panic!("{pattern:?} parses");
    };
    hir
}

/// The letters of `sample` that `regex-syntax` puts in the class of `(?i)x`.
fn regex_class(x: char, sample: &[char]) -> Vec<char> {
    let hir = case_insensitive(x);
    match hir.kind() {
        HirKind::Literal(_) => vec![x],
        HirKind::Class(Class::Unicode(class)) => class
            .ranges()
            .iter()
            .flat_map(|range| {
                let from = sample.partition_point(|&c| c < range.start());
                let to = sample.partition_point(|&c| c <= range.end());
                &sample[from..to]
            })
            .copied()
            .collect(),
        kind => panic!("(?i){x} parses to {kind:?}"),
    }
}

/// Letters `fold` changes by case alone: no mark, no decomposition, a one-char lowercase.
fn letters() -> Vec<char> {
    ('\u{0}'..=char::MAX)
        .filter(|&c| c.is_alphabetic() && !is_combining_mark(c))
        .filter(|c| iter::once(*c).nfd().eq(iter::once(*c)))
        .filter(|c| c.to_lowercase().count() == 1)
        .collect()
}

/// A letter std lowercases but `regex-syntax` finds caseless: its tables predate std's Unicode.
fn is_version_skew(x: char) -> bool {
    !x.to_lowercase().eq([x]) && matches!(case_insensitive(x).kind(), HirKind::Literal(_))
}

#[test]
fn the_unicode_version_skew_is_known() {
    let skew: Vec<char> = letters()
        .into_iter()
        .filter(|&x| is_version_skew(x))
        .collect();
    assert_eq!(
        skew.len(),
        SKEW,
        "std or regex-syntax changed Unicode version; recheck fold against regex: {skew:?}"
    );
}

/// `fold` must not drift from `(?i)` in D4's search: two letters fold alike exactly when
/// `regex-syntax` parses `(?i)x` into a class holding `y`.
#[test]
fn fold_agrees_with_regex_case_insensitive_classes() {
    let letters: Vec<char> = letters()
        .into_iter()
        .filter(|&x| !is_version_skew(x))
        .collect();
    let mut by_fold: HashMap<String, Vec<char>> = HashMap::new();
    for &c in &letters {
        by_fold
            .entry(fold(c.encode_utf8(&mut [0; 4])).into_owned())
            .or_default()
            .push(c);
    }
    let disagreements: Vec<(char, Vec<char>, &Vec<char>)> = letters
        .iter()
        .map(|&x| {
            let folded = fold(x.encode_utf8(&mut [0; 4])).into_owned();
            (x, regex_class(x, &letters), &by_fold[&folded])
        })
        .filter(|(_, class, alike)| class != *alike)
        .collect();
    assert_eq!(disagreements, [], "letter, regex class, same fold");
}

#[test]
fn iota_folds_to_a_letter_not_a_mark() {
    let iota = fold("ι");
    assert_eq!(fold("Ι"), iota);
    assert_eq!(fold("\u{1FBE}"), iota, "prosgegrammeni");
    assert!(!iota.chars().any(is_combining_mark), "{iota:?}");
    let dialytika_tonos = fold("ΐ");
    assert!(
        !dialytika_tonos.chars().any(is_combining_mark),
        "{dialytika_tonos:?}"
    );
}

#[test]
fn fold_is_idempotent() {
    let drifted: Vec<char> = letters()
        .into_iter()
        .filter(|x| {
            let once = fold(x.encode_utf8(&mut [0; 4])).into_owned();
            fold(&once) != once
        })
        .collect();
    assert_eq!(drifted, []);
}

#[test]
fn a_few_letters_agree_with_the_regex_engine() {
    for (x, y) in [
        ("ς", "σ"),
        ("Σ", "ς"),
        ("ſ", "S"),
        ("ϐ", "Β"),
        ("\u{212A}", "k"),
        ("ß", "ẞ"),
    ] {
        let Ok(re) = regex::Regex::new(&format!("(?i)^{}$", regex::escape(x))) else {
            panic!("{x:?} compiles");
        };
        assert!(re.is_match(y), "{x:?} {y:?}");
        assert_eq!(fold(x), fold(y), "{x:?} {y:?}");
    }
    assert_ne!(fold("ı"), fold("i"), "dotless i is its own letter");
}

#[test]
fn a_keycap_folds_to_its_digit() {
    // VS16 and the enclosing keycap are word chars and combining marks.
    assert_eq!(
        split("1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}"),
        [(0, "1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}")]
    );
    assert_eq!(fold("1\u{fe0f}\u{20e3}2\u{fe0f}\u{20e3}"), "12");
}

#[test]
fn a_lone_mark_folds_to_nothing() {
    assert_eq!(fold("\u{301}"), "");
}

#[test]
fn ascii_folds_without_allocating_when_already_lower() {
    assert!(matches!(fold("plan"), std::borrow::Cow::Borrowed("plan")));
    assert_eq!(fold("PLAN"), "plan");
}

#[test]
fn the_ascii_word_rule_is_the_regex_engine_s() {
    for c in (0..=0xFF_u8).map(char::from) {
        assert_eq!(is_word(c), regex_syntax::is_word_character(c), "{c:?}");
    }
}

#[test]
fn cap_cuts_on_a_char_boundary() {
    assert_eq!(cap("été", 2), "ét");
    assert_eq!(cap("été", 3), "été");
    assert_eq!(cap("été", 9), "été");
    assert_eq!(cap("", 1), "");
}

#[test]
fn the_table_agrees_with_the_orbit_rule() {
    // Only a char `to_lowercase` leaves alone and not a diacritic can reach `representative`.
    let drifted: Vec<char> = ('\u{0}'..=char::MAX)
        .filter(|&c| !is_diacritic(c) && c.to_lowercase().eq([c]))
        .filter(|&c| representative(c) != orbit_representative(c))
        .collect();
    assert_eq!(drifted, []);
}
