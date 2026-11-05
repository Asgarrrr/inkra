//! One-line text: a file name drawn in a row (macOS allows a newline in one) and
//! the text a single-line input admits.

use std::borrow::Cow;

#[cfg(test)]
mod tests;

/// Each ASCII control char becomes a space, one byte for one, so byte ranges into `text` stay valid.
pub(crate) fn one_line(text: &str) -> Cow<'_, str> {
    if text.contains(|c: char| c.is_ascii_control()) {
        Cow::Owned(text.replace(|c: char| c.is_ascii_control(), " "))
    } else {
        Cow::Borrowed(text)
    }
}
