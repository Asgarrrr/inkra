use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Embedded at build time: a missing file is a compile error, not a blank icon.
pub(crate) struct Assets;

/// One list of `Variant => "file-stem"` pairs, so no variant can be missing from `ALL`.
macro_rules! icons {
    ($($name:ident => $stem:literal),+ $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum IconName {
            $($name),+
        }

        impl IconName {
            pub(crate) const ALL: &[IconName] = &[$(Self::$name),+];

            pub(crate) fn path(self) -> &'static str {
                match self {
                    $(Self::$name => concat!("icons/", $stem, ".svg")),+
                }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$name => include_bytes!(concat!("../../assets/icons/", $stem, ".svg"))),+
                }
            }
        }
    };
}

icons! {
    ChevronDown => "chevron-down",
    ChevronRight => "chevron-right",
    PanelLeft => "panel-left",
    PanelRight => "panel-right",
    PenLine => "pen-line",
    Search => "search",
    X => "x",
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
