//! `search` against models that share nothing with it but `str`.

use std::sync::atomic::AtomicBool;

use proptest::prelude::*;
use regex::{Regex, RegexBuilder};
use unicode_normalization::UnicodeNormalization;

use crate::test_support::corpus;
use crate::{HITS_PER_NOTE, Searched, TextQuery, search};

const ASCII: &[&str] = &["a", "b", "A", "B", " ", "\n", "-", "ab"];
const ANY: &[&str] = &[
    "a", "e", "é", "e\u{301}", "É", "🎉", "\u{212A}", "k", "K", "ſ", "s", "S", " ", "\n", ".", "_",
];

fn text(alphabet: &'static [&'static str], len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(alphabet), 0..len).prop_map(|parts| parts.concat())
}

fn texts(alphabet: &'static [&'static str]) -> impl Strategy<Value = Vec<Option<String>>> {
    prop::collection::vec(prop::option::weighted(0.8, text(alphabet, 40)), 0..8)
}

/// `ANY`, plus a non-word alphanumeric, a lone mark, Greek, Cyrillic, CJK and punctuation.
const SCRIPTS: &[&str] = &[
    "a", "e", "é", "e\u{301}", "É", "🎉", "\u{212A}", "k", "K", "ſ", "s", "S", " ", "\n", ".", "_",
    "½", "\u{301}", "σ", "ς", "Σ", "ά", "е", "Е", "ё", "的", ",", "-",
];

/// The v1 matcher: the escaped literal with `\b` at each word-char edge, in each form.
fn reference(needle: &str, sensitive: bool, word: bool) -> Option<Regex> {
    fn literal(text: &str, word: bool) -> String {
        use regex_syntax::is_word_character as w;
        let mut pattern = regex::escape(text);
        if word && text.chars().next().is_some_and(w) {
            pattern.insert_str(0, r"\b");
        }
        if word && text.chars().next_back().is_some_and(w) {
            pattern.push_str(r"\b");
        }
        pattern
    }
    if needle.trim().is_empty() {
        return None;
    }
    let nfc: String = needle.nfc().collect();
    let pattern = if needle == nfc {
        literal(&nfc, word)
    } else {
        format!("(?:{})|(?:{})", literal(needle, word), literal(&nfc, word))
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!sensitive)
        .build()
        .ok()
}

const WORDY: &[&str] = &[
    "a", "a", "a", "-", "é", "e\u{301}", "_", "½", " ", "\n", "🎉", "1", "\u{301}",
];

/// Leftmost-first matches, as a regex alternation: at each char, the first needle that matches.
fn scan(text: &str, match_at: impl Fn(usize) -> Option<usize>) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        if let Some(end) = match_at(at) {
            out.push(at..end);
            at = end;
        } else {
            at += text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// Whether `text[at..end]`, a match of `needle`, has a boundary at each word-char edge.
fn edges(text: &str, at: usize, end: usize, needle: &str, word: bool) -> bool {
    use regex_syntax::is_word_character as w;
    let first = needle.chars().next().is_some_and(w);
    let last = needle.chars().next_back().is_some_and(w);
    (!word || !first || !text[..at].chars().next_back().is_some_and(w))
        && (!word || !last || !text[end..].chars().next().is_some_and(w))
}

fn scan_model(text: &str, needles: &[&str], word: bool) -> Vec<std::ops::Range<usize>> {
    scan(text, |at| {
        needles.iter().find_map(|needle| {
            let end = at + needle.len();
            (text[at..].starts_with(needle) && edges(text, at, end, needle, word)).then_some(end)
        })
    })
}

const FOLDY: &[&str] = &[
    "a", "A", "ß", "ẞ", "σ", "ς", "Σ", "İ", "i", "I", "ı", "ǅ", "ǆ", "Ǆ", "\u{212A}", "k", "K",
    "ſ", "s", "S", "é", "É", "e\u{301}", " ", "-", "µ", "μ", "Μ", "Å", "å", "\u{212B}",
];

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// The simple case-fold class of `c`: the union of one-char lower and upper mappings, minus
/// the Turkic dotted and dotless `i`, which simple folding keeps apart.
fn fold_key(c: char) -> u32 {
    const MAX: u32 = 0x0010_FFFF;
    static ROOTS: std::sync::LazyLock<Vec<u32>> = std::sync::LazyLock::new(|| {
        let mut parent: Vec<u32> = (0..=MAX).collect();
        for x in (0..=MAX).filter_map(char::from_u32) {
            for mapped in [
                x.to_lowercase().collect::<Vec<_>>(),
                x.to_uppercase().collect(),
            ] {
                if let [one] = mapped[..]
                    && ![x, one].iter().any(|c| matches!(c, 'ı' | 'İ'))
                {
                    let (a, b) = (find(&mut parent, x as u32), find(&mut parent, one as u32));
                    parent[a.max(b) as usize] = a.min(b);
                }
            }
        }
        (0..=MAX).map(|x| find(&mut parent, x)).collect()
    });
    ROOTS[c as usize]
}

fn fold_model(text: &str, needles: &[&str], word: bool) -> Vec<std::ops::Range<usize>> {
    scan(text, |at| {
        needles.iter().find_map(|needle| {
            let mut chars = text[at..].char_indices();
            let end = needle
                .chars()
                .all(|want| {
                    chars
                        .next()
                        .is_some_and(|(_, c)| fold_key(c) == fold_key(want))
                })
                .then(|| at + chars.offset());
            end.filter(|&end| edges(text, at, end, needle, word))
        })
    })
}

proptest! {
    #[test]
    fn find_in_equals_the_v1_regex(
        (needle, text) in prop_oneof![
            text(SCRIPTS, 4),
            // Its edge recurs inside it, so a valid match can start inside a rejected one.
            (prop::sample::select(SCRIPTS), text(SCRIPTS, 2))
                .prop_map(|(edge, inner)| format!("{edge}{inner}{edge}")),
        ]
        .prop_flat_map(|needle| {
            let part = prop_oneof![
                3 => prop::sample::select(SCRIPTS).prop_map(str::to_owned),
                1 => Just(needle.clone()),
                // The needle then its own tail: a valid match inside a rejected one.
                2 => Just(format!(
                    "a{needle}{}",
                    &needle[needle.chars().next().map_or(0, char::len_utf8)..]
                )),
            ];
            (Just(needle), prop::collection::vec(part, 0..30).prop_map(|parts| parts.concat()))
        }),
        sensitive in any::<bool>(),
        word in any::<bool>(),
        limit in 0usize..12,
    ) {
        let query = TextQuery::new(&needle, sensitive, word);
        let v1 = reference(&needle, sensitive, word);
        prop_assert_eq!(query.is_some(), v1.is_some());
        let (Some(query), Some(v1)) = (query, v1) else {
            return Ok(());
        };
        let got: Vec<_> = query.find_in(&text, limit).into_iter().map(|r| r.start.0..r.end.0).collect();
        let expected: Vec<_> = v1.find_iter(&text).take(limit).map(|m| m.range()).collect();
        prop_assert_eq!(got, expected, "{:?} in {:?}", needle, text);
    }

    #[test]
    fn insensitive_hits_equal_a_simple_fold_model(
        text in text(FOLDY, 30),
        needle in text(FOLDY, 3),
        word in any::<bool>(),
    ) {
        let Some(query) = TextQuery::new(&needle, false, word) else {
            return Ok(());
        };
        let nfc: String = needle.nfc().collect();
        let got: Vec<_> = query.find_in(&text, usize::MAX).into_iter().map(|r| r.start.0..r.end.0).collect();
        prop_assert_eq!(got, fold_model(&text, &[&needle, &nfc], word), "{:?} in {:?}", needle, text);
    }

    #[test]
    fn sensitive_hits_equal_a_scan_model_past_the_per_note_cap(
        texts in prop::collection::vec(prop::option::weighted(0.8, text(WORDY, 400)), 0..5),
        needle in text(WORDY, 4),
        word in any::<bool>(),
        limit in 0usize..400,
    ) {
        let Some(query) = TextQuery::new(&needle, true, word) else {
            return Ok(());
        };
        let nfc: String = needle.nfc().collect();
        let refs: Vec<Option<&str>> = texts.iter().map(Option::as_deref).collect();
        let (ids, corpus) = corpus(&refs);
        let mut expected = Vec::new();
        for (id, text) in ids.iter().zip(&texts) {
            let Some(text) = text else { continue };
            expected.extend(scan_model(text, &[&needle, &nfc], word).into_iter().take(HITS_PER_NOTE).map(|r| (*id, r)));
        }
        let reached = expected.len() >= limit;
        expected.truncate(limit);
        let found = search(&corpus, 0..corpus.len(), &query, &AtomicBool::new(false), limit);
        let got: Vec<_> = found.hits.iter().map(|hit| (hit.note, hit.range.start.0..hit.range.end.0)).collect();
        prop_assert_eq!(got, expected);
        prop_assert_eq!(found.limit_reached, reached);
    }

    #[test]
    fn ascii_hits_equal_match_indices(
        texts in texts(ASCII),
        needle in text(&["a", "b", "A", "B", " ", "-"], 4),
        sensitive in any::<bool>(),
        limit in 0usize..30,
    ) {
        let Some(query) = TextQuery::new(&needle, sensitive, false) else {
            prop_assert!(needle.trim().is_empty());
            return Ok(());
        };
        let fold = |s: &str| if sensitive { s.to_owned() } else { s.to_ascii_lowercase() };
        let refs: Vec<Option<&str>> = texts.iter().map(Option::as_deref).collect();
        let (ids, corpus) = corpus(&refs);
        let mut expected = Vec::new();
        for (id, text) in ids.iter().zip(&texts) {
            let Some(text) = text else { continue };
            let text = fold(text);
            expected.extend(
                text.match_indices(&fold(&needle)).take(HITS_PER_NOTE).map(|(at, s)| (*id, at..at + s.len())),
            );
        }
        let reached = expected.len() >= limit;
        expected.truncate(limit);
        let found = search(&corpus, 0..corpus.len(), &query, &AtomicBool::new(false), limit);
        let got: Vec<_> = found.hits.iter().map(|hit| (hit.note, hit.range.start.0..hit.range.end.0)).collect();
        prop_assert_eq!(got, expected);
        prop_assert_eq!(found.limit_reached, reached);
    }

    #[test]
    fn every_hit_is_on_char_boundaries_inside_its_context(
        texts in texts(ANY),
        needle in text(ANY, 5),
        sensitive in any::<bool>(),
        word in any::<bool>(),
    ) {
        let Some(query) = TextQuery::new(&needle, sensitive, word) else {
            return Ok(());
        };
        check_hits(&texts, &query)?;
    }

    #[test]
    fn a_query_over_160_bytes_keeps_the_invariants(
        head in text(ANY, 20),
        tail in text(ANY, 20),
        planted in any::<bool>(),
        sensitive in any::<bool>(),
        word in any::<bool>(),
    ) {
        let needle = format!("{}{head}", "é".repeat(81));
        // The query is NFC and the text is not normalized (D4), so plant the NFC form.
        let nfc: String = needle.nfc().collect();
        let body = if planted { format!("{head}{nfc}{tail}") } else { format!("{head}{tail}") };
        let Some(query) = TextQuery::new(&needle, sensitive, word) else {
            panic!("a 162-byte query compiles");
        };
        let found = check_hits(&[Some(body)], &query)?;
        if planted && !word {
            prop_assert!(!found.hits.is_empty());
        }
    }

    #[test]
    fn merge_over_any_split_equals_one_range(
        texts in texts(ANY),
        needle in text(ANY, 3),
        cuts in prop::collection::vec(0usize..9, 0..4),
        limit in 0usize..12,
    ) {
        let Some(query) = TextQuery::new(&needle, false, false) else {
            return Ok(());
        };
        let refs: Vec<Option<&str>> = texts.iter().map(Option::as_deref).collect();
        let (_, corpus) = corpus(&refs);
        let mut cuts: Vec<usize> = cuts.into_iter().map(|cut| cut.min(corpus.len())).collect();
        cuts.push(0);
        cuts.push(corpus.len());
        cuts.sort_unstable();
        let cancel = AtomicBool::new(false);
        let parts = cuts
            .windows(2)
            .map(|w| search(&corpus, w[0]..w[1], &query, &cancel, limit))
            .collect();
        prop_assert_eq!(
            Searched::merge(parts, limit),
            search(&corpus, 0..corpus.len(), &query, &cancel, limit)
        );
    }
}

fn check_hits(texts: &[Option<String>], query: &TextQuery) -> Result<Searched, TestCaseError> {
    let refs: Vec<Option<&str>> = texts.iter().map(Option::as_deref).collect();
    let (ids, corpus) = corpus(&refs);
    let found = search(
        &corpus,
        0..corpus.len(),
        query,
        &AtomicBool::new(false),
        usize::MAX,
    );
    for hit in &found.hits {
        let Some(at) = ids.iter().position(|id| *id == hit.note) else {
            return Err(TestCaseError::fail("a hit names a note of the corpus"));
        };
        let Some(text) = &texts[at] else {
            return Err(TestCaseError::fail("a hit is in a loaded note"));
        };
        let (start, end) = (hit.range.start.0, hit.range.end.0);
        prop_assert!(start < end);
        prop_assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        prop_assert_eq!(&hit.context[hit.context_match.clone()], &text[start..end]);
        prop_assert!(text.contains(&hit.context));
        prop_assert!(hit.context.len() <= end - start + 160);
        let matched = &text[start..end];
        prop_assert_eq!(hit.context.contains('\n'), matched.contains('\n'));
    }
    Ok(found)
}
