use std::borrow::Cow;
use std::ops::Range;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::words::{NoteQuery, fold, word_spans};

pub(super) const CANCEL_EVERY: usize = 64 << 10;

/// Each ASCII char's fold, by its byte.
static ASCII_FOLDS: LazyLock<[Box<str>; 128]> = LazyLock::new(|| {
    std::array::from_fn(|byte| {
        let c = u8::try_from(byte).map_or('\0', char::from);
        fold(c.encode_utf8(&mut [0; 4])).into()
    })
});

pub(crate) struct Terms<'q> {
    pub(super) terms: &'q [Box<str>],
    /// Indexed by an ASCII byte: whether its fold starts some term.
    pub(super) starts: [bool; 128],
}

impl<'q> Terms<'q> {
    pub(crate) fn new(query: &'q NoteQuery) -> Self {
        let terms = query.terms();
        let starts = std::array::from_fn(|byte| {
            terms
                .iter()
                .any(|term| term.starts_with(&*ASCII_FOLDS[byte]))
        });
        Self { terms, starts }
    }

    pub(crate) fn matching<'w>(
        &self,
        word: &'w str,
    ) -> impl Iterator<Item = &'q str> + use<'q, 'w> {
        // An ASCII first char folds alone and stays first: NFD moves no starter, marks only drop.
        let skipped = word
            .as_bytes()
            .first()
            .and_then(|&first| self.starts.get(usize::from(first)))
            .is_some_and(|starts| !starts);
        let folded: Option<Cow<'w, str>> = (!skipped).then(|| fold(word));
        self.terms
            .iter()
            .filter(move |term| {
                folded
                    .as_deref()
                    .is_some_and(|word| word.starts_with(&***term))
            })
            .map(|term| &**term)
    }
}

/// Stops before a word at the bound `each` last returned; `None` once `cancel` is set.
pub(crate) fn each_word(
    text: &str,
    cancel: &AtomicBool,
    mut each: impl FnMut(Range<usize>) -> usize,
) -> Option<()> {
    let mut until = usize::MAX;
    // A word running into a block's end may go on in the next block.
    let mut open: Option<Range<usize>> = None;
    let mut from = 0;
    while from < text.len() && (from < until || open.is_some()) {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let to = text.ceil_char_boundary(from + CANCEL_EVERY);
        for (at, word) in word_spans(&text[from..to]) {
            let word = from + at..from + at + word.len();
            let word = match open.take() {
                Some(head) if head.end == word.start => head.start..word.end,
                Some(head) => {
                    until = each(head);
                    word
                }
                None => word,
            };
            if word.start >= until {
                return Some(());
            }
            if word.end == to {
                open = Some(word);
            } else {
                until = each(word);
            }
        }
        from = to;
    }
    if let Some(word) = open {
        each(word);
    }
    Some(())
}
