use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Embedded at build time: a missing file is a compile error, not a blank icon.
pub(crate) struct Assets;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IconName {
    ChevronDown,
    ChevronRight,
}

impl IconName {
    pub(crate) const ALL: &[IconName] = &[IconName::ChevronDown, IconName::ChevronRight];

    pub(crate) fn path(self) -> &'static str {
        match self {
            IconName::ChevronDown => "icons/chevron-down.svg",
            IconName::ChevronRight => "icons/chevron-right.svg",
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            IconName::ChevronDown => include_bytes!("../../assets/icons/chevron-down.svg"),
            IconName::ChevronRight => include_bytes!("../../assets/icons/chevron-right.svg"),
        }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(IconName::ALL
            .iter()
            .find(|name| name.path() == path)
            .map(|name| Cow::Borrowed(name.bytes())))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(IconName::ALL
            .iter()
            .map(|name| name.path())
            .filter(|asset| asset.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}
