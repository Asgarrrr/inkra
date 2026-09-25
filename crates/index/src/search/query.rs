use std::ops::Range;

use inkra_text::ByteOffset;
use regex::{Regex, RegexBuilder};
use regex_syntax::is_word_character;
use unicode_normalization::UnicodeNormalization;

/// A longer paste would only make `regex` refuse the query.
const QUERY_MAX_CHARS: usize = 1_000;

/// A literal query, compiled once per keystroke. Clone it per thread: threads sharing one
/// `Regex` contend on its cache.
#[derive(Debug, Clone)]
pub struct TextQuery {
    regex: Regex,
    /// Word boundaries `find_in` checks itself, at the match start and end; `regex` has none.
    edges: (bool, bool),
}

impl TextQuery {
    /// Finds the first `QUERY_MAX_CHARS` typed chars of the query, as typed or NFC. `None` for
    /// an empty or blank query, or one too large for `regex` to compile.
    #[must_use]
    pub fn new(text: &str, case_sensitive: bool, whole_word: bool) -> Option<Self> {
        if text.trim().is_empty() {
            return None;
        }
        let raw: String = text.chars().take(QUERY_MAX_CHARS).collect();
        if raw.trim().is_empty() {
            return None;
        }
        let nfc: String = raw.nfc().collect();
        let (start, end) = word_edges(&nfc, whole_word);
        let first_ascii = nfc.chars().next().is_some_and(|c| c.is_ascii());
        let last_ascii = nfc.chars().next_back().is_some_and(|c| c.is_ascii());
        // regex's lazy DFA quits on a Unicode `\b` over non-ASCII text; ASCII edges stay (spec D4).
        let edges = if raw == nfc && ((start && !first_ascii) || (end && !last_ascii)) {
            (start, end)
        } else {
            (false, false)
        };
        let pattern = if edges != (false, false) {
            regex::escape(&nfc)
        } else if raw == nfc {
            literal(&nfc, whole_word)
        } else {
            // NFC maps singletons such as Kelvin `K` to another char, so a paste must match as is.
            format!(
                "(?:{})|(?:{})",
                literal(&raw, whole_word),
                literal(&nfc, whole_word)
            )
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .ok()?;
        Some(Self { regex, edges })
    }

    /// The first `limit` matches in `text`, in order, never overlapping.
    #[must_use]
    pub fn find_in(&self, text: &str, limit: usize) -> Vec<Range<ByteOffset>> {
        if self.edges == (false, false) {
            return self
                .regex
                .find_iter(text)
                .take(limit)
                .map(|found| ByteOffset(found.start())..ByteOffset(found.end()))
                .collect();
        }
        let mut hits = Vec::new();
        let mut at = 0;
        while hits.len() < limit
            && let Some(found) = self.regex.find_at(text, at)
        {
            let (start, end) = (found.start(), found.end());
            if self.edges_hold(text, start, end) {
                hits.push(ByteOffset(start)..ByteOffset(end));
                at = end;
            } else {
                // A valid match can start inside a rejected one: `é é` in `xé é é`.
                at = start + found.as_str().chars().next().map_or(1, char::len_utf8);
            }
        }
        hits
    }

    /// Whether `text[start..end]` has a word boundary at each edge in `edges`, as `\b` decides.
    fn edges_hold(&self, text: &str, start: usize, end: usize) -> bool {
        let word = |c: Option<char>| c.is_some_and(is_word_character);
        let matched = &text[start..end];
        (!self.edges.0 || word(text[..start].chars().next_back()) != word(matched.chars().next()))
            && (!self.edges.1
                || word(matched.chars().next_back()) != word(text[end..].chars().next()))
    }
}

/// Whether each edge of `text`, first then last char, is a word char needing `\b`.
fn word_edges(text: &str, whole_word: bool) -> (bool, bool) {
    let edge = |c: Option<char>| whole_word && c.is_some_and(is_word_character);
    (edge(text.chars().next()), edge(text.chars().next_back()))
}

/// `text` escaped, with `\b` at each edge whose char is a word char.
fn literal(text: &str, whole_word: bool) -> String {
    let mut pattern = regex::escape(text);
    let (start, end) = word_edges(text, whole_word);
    if start {
        pattern.insert_str(0, r"\b");
    }
    if end {
        pattern.push_str(r"\b");
    }
    pattern
}
