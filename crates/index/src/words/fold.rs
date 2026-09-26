use std::borrow::Cow;
use std::cmp::Ordering;
use std::iter;
use std::sync::LazyLock;

use regex_syntax::hir::{Class, ClassUnicode, ClassUnicodeRange, HirKind};
use regex_syntax::is_word_character;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::{canonical_combining_class, is_combining_mark};

/// Each maximal run of word chars in `text`, with its byte offset: D4's word rule, so a
/// whole-word search and quick open agree on what a word is.
pub(crate) fn word_spans(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut chars = text.char_indices().peekable();
    iter::from_fn(move || {
        let (start, _) = chars.by_ref().find(|&(_, c)| is_word(c))?;
        let mut end = text.len();
        while let Some(&(at, c)) = chars.peek() {
            if !is_word(c) {
                end = at;
                break;
            }
            chars.next();
        }
        Some((start, &text[start..end]))
    })
}

/// `is_word_character`, without its table search for an ASCII char that is not a word char.
fn is_word(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric() || c == '_'
    } else {
        is_word_character(c)
    }
}

/// Lowercase, NFD, no diacritic, then one char per case-folding orbit (S1): `Été`,
/// `ETE` and `e\u{301}te` fold to `ete`, `ς` and `Σ` to `σ`.
pub(crate) fn fold(word: &str) -> Cow<'_, str> {
    if word.is_ascii() {
        return if word.bytes().any(|byte| byte.is_ascii_uppercase()) {
            Cow::Owned(word.to_ascii_lowercase())
        } else {
            Cow::Borrowed(word)
        };
    }
    let mut blocked = false;
    let folded: String = word
        .chars()
        .flat_map(char::to_lowercase)
        .nfd()
        .filter(|&c| {
            if !is_diacritic(c) {
                return true;
            }
            blocked |= canonical_combining_class(c) == 0;
            false
        })
        .map(representative)
        .collect();
    // A dropped mark of class 0, like U+034F, stopped the reorder of the marks around it.
    Cow::Owned(if blocked {
        folded.nfd().collect()
    } else {
        folded
    })
}

/// A mark S1 drops: an accent, Arabic harakat or a Hebrew point, never an Indic or Thai
/// vowel sign.
fn is_diacritic(c: char) -> bool {
    is_combining_mark(c)
        && DIACRITICS
            .binary_search_by(|&(start, end)| {
                if end < c {
                    Ordering::Less
                } else if c < start {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            })
            .is_ok()
}

/// The ranges of `DIACRITIC_CLASS`, sorted.
static DIACRITICS: LazyLock<Box<[(char, char)]>> = LazyLock::new(|| {
    let Ok(hir) = regex_syntax::parse(DIACRITIC_CLASS) else {
        panic!("{DIACRITIC_CLASS:?} parses");
    };
    let HirKind::Class(Class::Unicode(class)) = hir.kind() else {
        panic!("{DIACRITIC_CLASS:?} is a Unicode class");
    };
    class
        .ranges()
        .iter()
        .map(|range| (range.start(), range.end()))
        .collect()
});

/// Inherited-script marks but kana voicing, plus Hebrew points (S1); `Script`, never `scx`.
const DIACRITIC_CLASS: &str =
    r"[\p{M}&&[[\p{Script=Inherited}--[\x{3099}\x{309A}]]\p{Script=Hebrew}]]";

/// One char per simple case folding orbit, from the tables D4's `(?i)` search uses.
fn representative(c: char) -> char {
    // ASCII lowercase represents its orbit; fold lowercases first, so no titlecase char gets here.
    if c.is_ascii() || !(c.is_lowercase() || c.is_uppercase()) {
        return c;
    }
    REPRESENTATIVES
        .binary_search_by_key(&c, |&(from, _)| from)
        .map_or(c, |at| REPRESENTATIVES[at].1)
}

/// Every cased char whose representative is another char, sorted.
static REPRESENTATIVES: LazyLock<Box<[(char, char)]>> = LazyLock::new(|| {
    // No cased letter lies past the SMP; `the_table_agrees_with_the_orbit_rule` checks every char.
    ('\u{80}'..='\u{1FFFF}')
        .filter(|&c| c.is_lowercase() || c.is_uppercase())
        .map(|c| (c, orbit_representative(c)))
        .filter(|&(c, representative)| representative != c)
        .collect()
});

/// The orbit's smallest member that is not a combining mark, lowercased when that gives one char.
fn orbit_representative(c: char) -> char {
    let mut orbit = ClassUnicode::new([ClassUnicodeRange::new(c, c)]);
    if orbit.try_case_fold_simple().is_err() {
        return c;
    }
    // Iota's orbit starts with U+0345, a mark `fold` would drop on a second pass.
    let first = orbit
        .iter()
        .flat_map(|range| range.start()..=range.end())
        .find(|&member| !is_combining_mark(member))
        .unwrap_or(c);
    let mut lower = first.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => first,
    }
}

/// The first `chars` chars of `word`.
pub(crate) fn cap(word: &str, chars: usize) -> &str {
    word.char_indices()
        .nth(chars)
        .map_or(word, |(at, _)| &word[..at])
}

#[cfg(test)]
mod tests;
