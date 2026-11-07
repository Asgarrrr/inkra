use std::path::Path;

use image::ImageFormat;
use inkra_vault::Kind;

#[cfg(test)]
mod tests;

/// What a tab can show of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Note,
    Image,
}

impl FileKind {
    /// `None` for a file Inkra cannot show; the name decides, never the bytes.
    pub(crate) fn of(path: &Path) -> Option<Self> {
        if Kind::of_file(path) == Kind::Note {
            return Some(Self::Note);
        }
        // The `image` features the build unifies decide, gpui's included (file-kinds spec, Types).
        ImageFormat::from_extension(path.extension()?)
            .is_some_and(|format| format.reading_enabled())
            .then_some(Self::Image)
    }
}
