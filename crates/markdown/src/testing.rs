//! Helpers the crate's tests share.

use inkra_text::{Bias, ByteOffset, Diff, Hunk, Version};

use crate::ListEdit;

/// `text` with `hunks`, in order and disjoint, applied.
pub(crate) fn apply(text: &str, hunks: &[Hunk]) -> String {
    let mut out = text.to_owned();
    for hunk in hunks.iter().rev() {
        out.replace_range(hunk.range.start.get()..hunk.range.end.get(), &hunk.text);
    }
    out
}

/// Where `at`, an offset before `edit`, lands once it applies: an anchor biased right, as the app
/// maps the caret. Panics on hunks out of order or overlapping, as `Diff::new` does.
pub(crate) fn landing(edit: &ListEdit, at: usize) -> usize {
    let diff = Diff::new(Version::default(), edit.hunks.clone());
    diff.map(ByteOffset::new(at), Bias::Right).get()
}

/// `text` with `edit` applied, and where its caret lands in it.
pub(crate) fn edited(text: &str, edit: &ListEdit) -> (String, usize) {
    let caret = landing(edit, edit.caret.get());
    (apply(text, &edit.hunks), caret)
}
