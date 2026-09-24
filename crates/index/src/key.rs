use unicode_normalization::UnicodeNormalization;

/// A path or name as resolution compares it: NFC and lowercased. Callers strip `.md` once,
/// beforehand, so `x.md.md` keeps the key `x.md`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key(String);

impl Key {
    /// NFC again after lowercasing: `to_lowercase` can leave a string that is not NFC.
    pub(crate) fn fold(text: &str) -> Self {
        if text.is_ascii() {
            return Self(text.to_ascii_lowercase());
        }
        let composed: String = text.nfc().collect();
        Self(composed.to_lowercase().nfc().collect())
    }

    /// A `/`-separated path.
    pub(crate) fn path(path: &str) -> Self {
        Self::fold(path)
    }

    /// The last component of a `/`-separated path.
    pub(crate) fn stem(path: &str) -> Self {
        Self::fold(path.rsplit('/').next().unwrap_or(path))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// `path` without a trailing `.md` in any ASCII case.
pub(crate) fn strip_md(path: &str) -> &str {
    path.len()
        .checked_sub(3)
        .filter(|&at| path.is_char_boundary(at) && path[at..].eq_ignore_ascii_case(".md"))
        .map_or(path, |at| &path[..at])
}

#[cfg(test)]
mod tests;
