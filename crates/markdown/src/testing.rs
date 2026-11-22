//! Helpers the crate's tests share.

use inkra_text::Hunk;

/// `text` with `hunks`, in order and disjoint, applied.
pub(crate) fn apply(text: &str, hunks: &[Hunk]) -> String {
    let mut out = text.to_owned();
    for hunk in hunks.iter().rev() {
        out.replace_range(hunk.range.start.get()..hunk.range.end.get(), &hunk.text);
    }
    out
}
